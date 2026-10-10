// 前后端共享的事件名常量与事件 payload 类型
// （Rust 侧 src-tauri/src/events.rs 手写镜像，改这里必须同步）。

/** 下载任务进度推送（ProgressPayload） */
export const TASK_PROGRESS = "task-progress";
/** 批量文件下载失败挂起，等待用户决策（{ task_id, file_name, error }） */
export const TASK_WAITING_USER = "task-waiting-user";
/** 微软登录完成回调（{ ok, account, error }） */
export const MS_LOGIN_COMPLETE = "ms-login-complete";
/** 游戏进程退出（[pid, versionLabel, exitCode, crashReport]；exitCode 非 0 = 异常退出，crashReport 为最新崩溃报告路径或 null） */
export const GAME_EXITED = "game-exited";
/** 游戏主窗口可见（[pid, versionLabel]；后端 EnumWindows 轮询判定） */
export const GAME_WINDOW_VISIBLE = "game-window-visible";
/** 联机状态变更（{ status: "idle"|"hosting"|"joining" }） */
export const ONLINE_STATE = "online-state";
/** 工具箱下载器任务快照（ToolDlSnapshot；单任务全量状态，按 id 增量更新） */
export const TOOL_DL_PROGRESS = "tooldl-progress";
/** 配置文件损坏已恢复（RecoveryNotice；运行期推送，启动期由命令拉取兜底） */
export const CONFIG_RECOVERY = "config-recovery";

// ── 事件 payload 类型（前端类型由 Rust 生成，见 src/types/）──

export type { StepStatus } from "../../types/task/StepStatus";
export type { StepInfo } from "../../types/task/StepInfo";
/** task-progress 事件 payload（生成名 TaskProgress；前端历史名 ProgressPayload）。 */
export type { TaskProgress as ProgressPayload } from "../../types/task/TaskProgress";

/** task-waiting-user 事件 payload（生成：types/task/WaitingUserPayload.ts）。 */
export type { WaitingUserPayload } from "../../types/task/WaitingUserPayload";

/** 工具箱下载任务状态（生成：types/tooldl/ToolDlStatus.ts）。 */
export type { ToolDlStatus } from "../../types/tooldl/ToolDlStatus";

/** tooldl-progress 事件 payload（生成：types/tooldl/ToolDlSnapshot.ts）。 */
export type { ToolDlSnapshot } from "../../types/tooldl/ToolDlSnapshot";

/** config-recovery 事件 / take_config_recovery_notice 返回值
 *  （镜像 Rust persist::config::RecoveryNotice）。 */
export interface RecoveryNotice {
  /** 损坏文件备份到的路径（null = 备份失败） */
  backup_path: string | null;
  reason: string;
}
