// 前后端共享事件名与 payload 类型（Rust 侧 src-tauri/src/events.rs 手写镜像，改动必须同步）。

export const TASK_PROGRESS = "task-progress";
export const TASK_WAITING_USER = "task-waiting-user";
export const MS_LOGIN_COMPLETE = "ms-login-complete";
/** 游戏进程退出（[pid, versionLabel, exitCode, crashReport]；exitCode 非 0 = 异常退出，crashReport 为最新崩溃报告路径或 null） */
export const GAME_EXITED = "game-exited";
export const GAME_WINDOW_VISIBLE = "game-window-visible";
export const ONLINE_STATE = "online-state";
export const TOOL_DL_PROGRESS = "tooldl-progress";
/** 配置文件损坏已恢复（RecoveryNotice；运行期推送，启动期由命令拉取兜底） */
export const CONFIG_RECOVERY = "config-recovery";
export const AUTO_TASK_PROGRESS = "auto-task-progress";
export const AUTO_TASKS_FINISHED = "auto-tasks-finished";
/** 版本列表变化（VersionsChanged：{ dir, versions }；`versions/` 一级条目或版本 json 变化） */
export const VERSIONS_CHANGED = "versions-changed";
/** 聚焦版本目录内资源子目录变化（VersionDirChanged：{ version, kinds }） */
export const VERSION_DIR_CHANGED = "version-dir-changed";

// ── 事件 payload 类型（前端类型由 Rust 生成，见 src/types/）──

export type { StepStatus } from "../../types/task/StepStatus";
export type { StepInfo } from "../../types/task/StepInfo";
export type { TaskProgress as ProgressPayload } from "../../types/task/TaskProgress";
export type { WaitingUserPayload } from "../../types/task/WaitingUserPayload";
export type { ToolDlStatus } from "../../types/tooldl/ToolDlStatus";
export type { ToolDlSnapshot } from "../../types/tooldl/ToolDlSnapshot";
export type { VersionsChanged } from "../../types/game/VersionsChanged";
export type { VersionDirChanged } from "../../types/game/VersionDirChanged";

/** config-recovery / take_config_recovery_notice 返回值（镜像 Rust persist::config::RecoveryNotice）。 */
export interface RecoveryNotice {
  /** 损坏文件备份到的路径（null = 备份失败） */
  backup_path: string | null;
  reason: string;
}

// ── 自动任务事件 payload（生成类型，见 src/types/launcher/）──

export type { AutoTaskProgress } from "../../types/launcher/AutoTaskProgress";
export type { AutoTasksFinished } from "../../types/launcher/AutoTasksFinished";
export type { AutoTaskFailureInfo } from "../../types/launcher/AutoTaskFailureInfo";
