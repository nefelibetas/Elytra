// JDK 配置 store：扫描/选择/保存 Java 运行时 + 下载缺失 JDK（提交任务中心）。
import {ref} from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useConfigSection } from "../utils/useConfigSection";
import { dlState, registerTaskDoneHook, type TaskKind } from "./download";
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
 *  完成后经 dlState 统一监听的 done 分发（registerTaskDoneHook）自动选中新装的 JDK。
 *  本函数只负责提交与预注册，不阻塞等待下载。 */
export async function installJdk(major: number): Promise<void> {
  loading.value = true;
  try {
    const taskId = await invoke<string>("submit_install_jdk", { major });
    // 完成钩子先于卡片预注册注册：极快终态事件也不会漏接
    pendingInstall.set(taskId, major);
    registerTaskDoneHook(taskId, () => {
      const m = pendingInstall.get(taskId);
      pendingInstall.delete(taskId);
      if (m !== undefined) onJdkInstalled(m);
    });
    // 预注册任务卡片（kind 标记：App.vue 的完成回调据此跳过 toast/跳转）
    // 并做统一竞态校正；silentFastExit：本函数已有"已提交"提示，不叠加中性提示。
    await dlState.adoptTask(
      taskId,
      { name: `下载 Java ${major}`, kind: "jdk" as TaskKind },
      true,
    );
    showToast(`已提交 Java ${major} 下载任务`);
  } finally {
    loading.value = false;
  }
}

// ── 完成接续：done → 加载配置并自动选中新装的 JDK（经 dlState 统一监听分发）──

/** 安装中的 JDK 任务（task_id → major），完成钩子消费后清理。 */
const pendingInstall = new Map<string, number>();

/** 从版本字符串解析主版本号（"17.0.8" → 17，"1.8.0_202" → 8；与后端 parse_java_major 语义一致）。 */
function parseMajor(version: string): number | null {
  const m = /^(?:1\.)?(\d+)/.exec(version);
  return m ? Number(m[1]) : null;
}

/** 安装完成：重新加载 JDK 列表（后端已注册 persist）并自动选中新装的版本。 */
async function onJdkInstalled(major: number) {
  try {
    await loadConfig();
    const info = jdkList.value.find((e) => parseMajor(e.version) === major);
    if (info) {
      autoMode.value = false;
      activeJdk.value = info;
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
  saveConfig();
}

export function selectJdk(env: JdkInfo) {
  autoMode.value = false;
  activeJdk.value = env;
  saveConfig();
}

