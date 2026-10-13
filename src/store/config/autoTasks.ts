// 自动任务 store：全局/实例自动任务读写、事件进度与失败汇总（实例设置页与全局提示共用）。
// IPC 只在此封装（组件不直接 invoke）。
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

/** 备份默认保留份数（与 Rust `default_keep` 一致）。 */
export const DEFAULT_KEEP = 5;
/** 自定义命令默认超时秒数（与 Rust `default_timeout_secs` 一致）。 */
export const DEFAULT_TIMEOUT = 120;

/** 三个执行阶段（顺序即 UI 展示顺序）。 */
export const phases: { key: AutoTaskPhase; label: string; desc: string }[] = [
  { key: "pre_launch", label: "启动前", desc: "启动游戏前执行；失败是否中止由下方开关决定" },
  { key: "on_crash", label: "崩溃时", desc: "检测到崩溃（非零退出且有崩溃证据）后执行" },
  { key: "post_exit", label: "退出后", desc: "游戏退出后总是执行" },
];

/** 内置动作目录。 */
export const builtinCatalog: { kind: string; label: string; desc: string }[] = [
  { kind: "backup_saves", label: "备份存档", desc: "按时间戳备份存档到 back/saves，保留最近 N 份" },
  { kind: "record_mods", label: "记录 Mod 清单", desc: "写 mods.json（文件名 + 大小 + sha1）" },
  { kind: "pack_crash_logs", label: "打包崩溃日志", desc: "崩溃报告 + 游戏/启动器日志打包（已脱敏）" },
];

/** 动作种类（用于选择图标与文案）。 */
export function actionKind(action: AutoTaskAction): string {
  if (action === "RecordMods") return "record_mods";
  if (action === "PackCrashLogs") return "pack_crash_logs";
  if ("BackupSaves" in action) return "backup_saves";
  return "command";
}

/** 动作显示文案。 */
export function actionLabel(action: AutoTaskAction): string {
  if (action === "RecordMods") return "记录 Mod 清单";
  if (action === "PackCrashLogs") return "打包崩溃日志";
  if ("BackupSaves" in action) {
    const { scope, keep } = action.BackupSaves;
    const scopeText = scope === "All" ? "全部世界" : `指定 ${scope.Selected.worlds.length} 个世界`;
    return `备份存档（${scopeText}，保留 ${keep} 份）`;
  }
  if ("Command" in action) return action.Command.command.trim() || "（空命令）";
  return "未知动作";
}

/** 构造动作（添加菜单用）。 */
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
/** 幂等注册自动任务事件监听（App 启动时调用一次）。 */
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

/** 读取全局自动任务（设置页编辑用）。 */
export async function getGlobalAutoTasks(): Promise<AutoTasks> {
  const cfg = await invoke<LaunchOptions>("get_launch_options");
  return cfg.auto_tasks;
}

/** 保存全局自动任务（后端读改写 launcher section，不影响其余字段）。 */
export async function saveGlobalAutoTasks(tasks: AutoTasks): Promise<void> {
  await invoke("save_global_auto_tasks", { tasks });
}

/** 读取实例自动任务状态（全局 + 实例）。 */
export async function getInstanceAutoTasks(versionId: string): Promise<InstanceAutoTasksState> {
  return await invoke<InstanceAutoTasksState>("get_instance_auto_tasks", { versionId });
}

/** 保存实例自动任务；`null` 表示删除实例配置（该实例回退全局）。 */
export async function saveInstanceAutoTasks(versionId: string, auto_tasks: AutoTasks | null): Promise<void> {
  // 键名必须与后端形参 camelCase 对齐（后端是 `tasks`）——传错会让后端收到 None，
  // 静默走「删除实例配置」分支。
  await invoke("save_instance_auto_tasks", { versionId, tasks: auto_tasks });
}

/** 立即执行一次某阶段自动任务（使用当前生效配置）。 */
export async function runAutoTasksNow(versionId: string, phase: AutoTaskPhase): Promise<void> {
  await invoke("run_auto_tasks_now", { versionId, phase });
}
