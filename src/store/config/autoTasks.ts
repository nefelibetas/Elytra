// 自动任务 store：全局/实例自动任务读写、事件进度与失败汇总；IPC 只在此封装（组件不直接 invoke）。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AUTO_TASKS_FINISHED, AUTO_TASK_PROGRESS } from "../app/events";
import type { AutoTaskProgress } from "../../types/launcher/AutoTaskProgress";
import type { AutoTasksFinished } from "../../types/launcher/AutoTasksFinished";
import type { AutoTaskPhase } from "../../types/launcher/AutoTaskPhase";
import type { AutoTaskAction } from "../../types/launcher/AutoTaskAction";
import type { AutoTasks } from "../../types/launcher/AutoTasks";
import type { InstanceAutoTasksState } from "../../types/launcher/InstanceAutoTasksState";
import type { LaunchOptions } from "../../types/launcher/LaunchOptions";
import { t } from "../../i18n";

/** 备份默认保留份数（与 Rust `default_keep` 一致）。 */
export const DEFAULT_KEEP = 5;
/** 自定义命令默认超时秒数（与 Rust `default_timeout_secs` 一致）。 */
export const DEFAULT_TIMEOUT = 120;

/** 三个执行阶段（顺序即 UI 展示顺序）；label/desc 存 i18n key，渲染侧翻译。 */
export const phases: { key: AutoTaskPhase; labelKey: string; descKey: string }[] = [
  { key: "pre_launch", labelKey: "settings.autoTasks.phases.preLaunch.label", descKey: "settings.autoTasks.phases.preLaunch.desc" },
  { key: "on_crash", labelKey: "settings.autoTasks.phases.onCrash.label", descKey: "settings.autoTasks.phases.onCrash.desc" },
  { key: "post_exit", labelKey: "settings.autoTasks.phases.postExit.label", descKey: "settings.autoTasks.phases.postExit.desc" },
];

export const builtinCatalog: { kind: string; labelKey: string; descKey: string }[] = [
  { kind: "backup_saves", labelKey: "settings.autoTasks.builtin.backupSaves.label", descKey: "settings.autoTasks.builtin.backupSaves.desc" },
  { kind: "record_mods", labelKey: "settings.autoTasks.builtin.recordMods.label", descKey: "settings.autoTasks.builtin.recordMods.desc" },
  { kind: "pack_crash_logs", labelKey: "settings.autoTasks.builtin.packCrashLogs.label", descKey: "settings.autoTasks.builtin.packCrashLogs.desc" },
];

export function actionKind(action: AutoTaskAction): string {
  if (action === "RecordMods") return "record_mods";
  if (action === "PackCrashLogs") return "pack_crash_logs";
  if ("BackupSaves" in action) return "backup_saves";
  return "command";
}

export function actionLabel(action: AutoTaskAction): string {
  if (action === "RecordMods") return t("settings.autoTasks.builtin.recordMods.label");
  if (action === "PackCrashLogs") return t("settings.autoTasks.builtin.packCrashLogs.label");
  if ("BackupSaves" in action) {
    const { scope, keep } = action.BackupSaves;
    const scopeText = scope === "All"
      ? t("settings.autoTasks.scopeAll")
      : t("settings.autoTasks.scopeSelectedCount", { count: scope.Selected.worlds.length });
    return t("settings.autoTasks.backupSummary", { scope: scopeText, keep });
  }
  if ("Command" in action) return action.Command.command.trim() || t("settings.autoTasks.emptyCommand");
  return t("settings.autoTasks.unknownAction");
}

export function makeAction(kind: string): AutoTaskAction {
  switch (kind) {
    case "backup_saves":
      return { BackupSaves: { scope: "All", keep: DEFAULT_KEEP } };
    case "record_mods":
      return "RecordMods";
    case "pack_crash_logs":
      return "PackCrashLogs";
    default:
      return { Command: { command: "", timeout_secs: DEFAULT_TIMEOUT } };
  }
}

// ── 运行态（事件驱动）──

/** 最近一帧自动任务进度（null = 当前无进行中的自动任务）。 */
export const autoTaskProgress = ref<AutoTaskProgress | null>(null);
/** 最近一次阶段结束汇总（failures 非空 → 全局 dialog 提示）。 */
export const lastAutoTasksFinished = ref<AutoTasksFinished | null>(null);

let listening = false;
export function ensureAutoTasksListen(): void {
  if (listening) return;
  listening = true;
  void listen<AutoTaskProgress>(AUTO_TASK_PROGRESS, (e) => {
    autoTaskProgress.value = e.payload;
  });
  void listen<AutoTasksFinished>(AUTO_TASKS_FINISHED, (e) => {
    lastAutoTasksFinished.value = e.payload;
    autoTaskProgress.value = null;
  });
}

export async function getGlobalAutoTasks(): Promise<AutoTasks> {
  const cfg = await invoke<LaunchOptions>("get_launch_options");
  return cfg.auto_tasks;
}

/** 保存全局自动任务（后端读改写 launcher section，不影响其余字段）。 */
export async function saveGlobalAutoTasks(tasks: AutoTasks): Promise<void> {
  await invoke("save_global_auto_tasks", { tasks });
}

export async function getInstanceAutoTasks(versionId: string): Promise<InstanceAutoTasksState> {
  return await invoke<InstanceAutoTasksState>("get_instance_auto_tasks", { versionId });
}

/** 保存实例自动任务；`null` 表示删除实例配置（该实例回退全局）。 */
export async function saveInstanceAutoTasks(versionId: string, auto_tasks: AutoTasks | null): Promise<void> {
  // 键名须与后端形参 camelCase 对齐（后端为 `tasks`）；传错会让后端收到 None，静默走「删除实例配置」分支
  await invoke("save_instance_auto_tasks", { versionId, tasks: auto_tasks });
}

export async function runAutoTasksNow(versionId: string, phase: AutoTaskPhase): Promise<void> {
  await invoke("run_auto_tasks_now", { versionId, phase });
}
