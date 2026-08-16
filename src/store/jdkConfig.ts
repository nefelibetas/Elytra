// JDK 配置 store：扫描/选择/保存 Java 运行时 + 下载缺失 JDK（提交任务中心）。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useConfigSection } from "../utils/useConfigSection";
import { dlState, type ProgressPayload, type TaskKind } from "./download";
import { EVT_TASK_PROGRESS } from "./events";
import { showToast } from "../utils/toast";

export interface JdkInfo {
  path: string;
  version: string;
  vendor: string;
  bits: number;
}

type JdkSelection = { mode: "auto" } | { mode: "manual"; path: string };

interface JdkConfig {
  jdk_selection: JdkSelection;
  discovered_jdks: JdkInfo[];
}

const section = useConfigSection();
export const jdkError = section.error;

export const jdkList = ref<JdkInfo[]>([]);
export const autoMode = ref(true);
export const activeJdk = ref<JdkInfo | null>(null);
export const loading = ref(false);
export const pickerOpen = ref(false);

export async function loadConfig() {
  try {
    const cfg = await invoke<JdkConfig>("get_jdk_config");
    const sel = cfg.jdk_selection;
    jdkList.value = cfg.discovered_jdks ?? [];
    autoMode.value = sel.mode === "auto";
    activeJdk.value =
      sel.mode === "manual"
        ? cfg.discovered_jdks.find((e) => e.path === sel.path) ?? null
        : null;
  } catch {
    // 忽略
  }
}

export async function saveConfig() {
  await section.run(async () => {
    await invoke("save_jdk_config", {
      config: {
        jdk_selection: autoMode.value
          ? { mode: "auto" }
          : { mode: "manual", path: activeJdk.value?.path ?? "" },
        discovered_jdks: jdkList.value,
      },
    });
  });
}

export async function refreshJdks() {
  loading.value = true;
  await section.run(async () => {
    jdkList.value = await invoke<JdkInfo[]>("auto_scan_jdk");
  });
  loading.value = false;
}

export async function selectCustomDir() {
  loading.value = true;
  await section.run(async () => {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;
    jdkList.value = await invoke<JdkInfo[]>("scan_custom_jdk", { path: dir });
  });
  loading.value = false;
}

/** 提交缺失 JDK 下载任务到任务中心（官方 runtime → Azul 回退在后端），
 *  完成后经 task-progress done 事件自动注册进列表并选中（ensureJdkListen）。
 *  本函数只负责提交与预注册，不阻塞等待下载。 */
export async function installJdk(major: number): Promise<void> {
  loading.value = true;
  try {
    const taskId = await invoke<string>("submit_install_jdk", { major });
    // 预注册任务卡片：kind 标记（App.vue 的完成回调据此跳过 toast/跳转），
    // previousRoute 空串（done 回调 prev ?? "/" 保留空串，不触发跳转）
    dlState.addTask({
      taskId,
      name: `下载 Java ${major}`,
      status: "pending",
      progress: 0,
      speedBps: 0,
      etaSeconds: null,
      completedFiles: 0,
      totalFiles: 0,
      completedBytes: 0,
      totalBytes: 0,
      currentPhase: "等待中",
      steps: [],
      previousRoute: "",
      kind: "jdk" as TaskKind,
    });
    pendingInstall.set(taskId, major);
    ensureJdkListen();
    // 竞态校正：任务可能极快终态（error/done 事件先于预注册到达被忽略，
    // 卡片以 pending 残留且不再有终态事件），查不到即已结束——移除卡片。
    const exists = await invoke<boolean>("task_exists", { taskId });
    if (!exists) {
      pendingInstall.delete(taskId);
      dlState.removeTask(taskId);
    }
    showToast(`已提交 Java ${major} 下载任务`);
  } finally {
    loading.value = false;
  }
}

// ── 完成监听（幂等）：done → 加载配置并自动选中新装的 JDK ──

/** 预注册的 JDK 安装任务（task_id → major），终态事件后清理。 */
const pendingInstall = new Map<string, number>();

let jdkListening = false;

/** 从版本字符串解析主版本号（"17.0.8" → 17，"1.8.0_202" → 8；与后端 parse_java_major 语义一致）。 */
function parseMajor(version: string): number | null {
  const m = /^(?:1\.)?(\d+)/.exec(version);
  return m ? Number(m[1]) : null;
}

function ensureJdkListen() {
  if (jdkListening) return;
  jdkListening = true;
  listen<ProgressPayload>(EVT_TASK_PROGRESS, (ev) => {
    const p = ev.payload;
    const major = pendingInstall.get(p.task_id);
    if (major === undefined) return;
    if (p.status === "done") {
      pendingInstall.delete(p.task_id);
      onJdkInstalled(major);
    } else if (p.status === "error" || p.status === "cancelled") {
      pendingInstall.delete(p.task_id);
    }
  });
}

/** 安装完成：重新加载 JDK 列表（后端已注册 persist）并自动选中新装的版本。 */
async function onJdkInstalled(major: number) {
  try {
    await loadConfig();
    const info = jdkList.value.find((e) => parseMajor(e.version) === major);
    if (info) {
      autoMode.value = false;
      activeJdk.value = info;
      pickerOpen.value = false;
      await saveConfig();
      showToast(`Java ${major} 已安装并自动选择`);
    } else {
      showToast(`Java ${major} 已安装完成`);
    }
  } catch {
    showToast(`Java ${major} 已安装完成，自动选择失败`);
  }
}

export function selectAuto() {
  autoMode.value = true;
  activeJdk.value = null;
  pickerOpen.value = false;
  saveConfig();
}

export function selectJdk(env: JdkInfo) {
  autoMode.value = false;
  activeJdk.value = env;
  pickerOpen.value = false;
  saveConfig();
}

export const pickerLabel = computed(() => {
  if (autoMode.value) return "自动选择";
  return activeJdk.value ? activeJdk.value.version : "未选择";
});

export const pickerSub = computed(() => {
  if (autoMode.value) return "根据游戏版本需求自动匹配";
  return activeJdk.value?.path ?? "";
});
