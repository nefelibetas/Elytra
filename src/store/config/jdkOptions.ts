// JDK 配置 store：扫描/选择/保存 Java 运行时 + 下载缺失 JDK（提交任务中心）。
import {ref} from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useConfigSection } from "../../utils/useConfigSection";
import { dlState, registerTaskDoneHook, type TaskKind } from "../task/download";
import { showToast } from "../../utils/toast";
import { t } from "../../i18n";

import type { JdkInfo } from "../../types/jdk/JdkInfo";
import type { JdkOptions } from "../../types/jdk/JdkOptions";
export type { JdkInfo, JdkOptions };

const section = useConfigSection();
export const jdkError = section.error;

export const jdkList = ref<JdkInfo[]>([]);
export const autoMode = ref(true);
export const activeJdk = ref<JdkInfo | null>(null);
export const loading = ref(false);

export async function loadConfig() {
  try {
    // 配置与扫描缓存分属两个 section：配置读 jdk，列表读 jdk_scanned
    const [options, discovered] = await Promise.all([
      invoke<JdkOptions>("get_jdk_options"),
      invoke<JdkInfo[]>("get_discovered_jdks"),
    ]);
    const sel = options.selection;
    jdkList.value = discovered ?? [];
    autoMode.value = sel.mode === "auto";
    activeJdk.value =
      sel.mode === "manual"
        ? discovered.find((e) => e.path === sel.path) ?? null
        : null;
  } catch {
  }
}

export async function saveConfig() {
  await section.run(async () => {
    await invoke("save_jdk_options", {
      options: {
        selection: autoMode.value
          ? { mode: "auto" }
          : { mode: "manual", path: activeJdk.value?.path ?? "" },
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

/** 提交缺失 JDK 下载任务（官方 runtime→Azul 回退在后端）；完成后经 dlState 的 done 钩子自动选中。 */
export async function installJdk(major: number): Promise<void> {
  loading.value = true;
  try {
    const taskId = await invoke<string>("submit_install_jdk", { major });
    // 完成钩子先于卡片预注册注册：极快终态事件也不会漏接
    pendingInstall.set(taskId, major);
    registerTaskDoneHook(taskId, () => {
      const m = pendingInstall.get(taskId);
      pendingInstall.delete(taskId);
      if (m !== undefined) void onJdkInstalled(m);
    });
    // 预注册任务卡片（kind=jdk：App.vue 完成回调据此跳过 toast/跳转）并做统一竞态校正；silentFastExit：已有「已提交」提示，不叠加中性提示。
    await dlState.adoptTask(
      taskId,
      { name: t("settings.jdk.taskName", { major }), kind: "jdk" as TaskKind },
      true,
    );
    showToast(t("settings.jdk.submitted", { major }));
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

async function onJdkInstalled(major: number) {
  try {
    await loadConfig();
    const info = jdkList.value.find((e) => parseMajor(e.version) === major);
    if (info) {
      autoMode.value = false;
      activeJdk.value = info;
      await saveConfig();
      showToast(t("settings.jdk.installedSelected", { major }));
    } else {
      showToast(t("settings.jdk.installed", { major }));
    }
  } catch {
    showToast(t("settings.jdk.installedSelectFailed", { major }));
  }
}

export function selectAuto() {
  autoMode.value = true;
  activeJdk.value = null;
  void saveConfig();
}

export function selectJdk(env: JdkInfo) {
  autoMode.value = false;
  activeJdk.value = env;
  void saveConfig();
}

