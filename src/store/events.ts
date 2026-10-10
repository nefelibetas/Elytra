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

// ── 事件 payload 类型（集中定义，store 经 re-export 保持既有 import 路径兼容）──

export type StepStatus = "pending" | "running" | "done";

/** 进度推送中的单步骤状态。 */
export interface StepInfo {
  name: string
  status: StepStatus
  progress: number
}

/** task-progress 事件 payload（镜像 Rust task::progress::ProgressPayload）。 */
export interface ProgressPayload {
  task_id: string
  name: string
  status: string
  progress: number
  speed_bps: number
  eta_seconds: number | null
  completed_files: number
  total_files: number
  completed_bytes: number
  total_bytes: number
  current_phase: string
  steps: StepInfo[]
  error?: string | null
}

/** task-waiting-user 事件 payload（镜像 Rust task::files::WaitingUserPayload）。 */
export interface WaitingUserPayload {
  task_id: string;
  file_name: string;
  error: string;
}

/** 工具箱下载任务状态（镜像 Rust tooldl::model::ToolDlStatus）。 */
export type ToolDlStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "done"
  | "error"
  | "cancelled";

/** tooldl-progress 事件 payload（镜像 Rust tooldl::model::ToolDlSnapshot）。 */
export interface ToolDlSnapshot {
  id: string;
  url: string;
  file_name: string;
  /** 最终落地路径 */
  dest: string;
  save_dir: string;
  /** 总字节（0 = 未知） */
  total: number;
  downloaded: number;
  /** 字节/秒（0 = 未在下载） */
  speed: number;
  /** 是否走 HTTP Range 分段 */
  ranged: boolean;
  /** 实际分段数（1 = 单连接） */
  segments: number;
  status: ToolDlStatus;
  error: string | null;
  created_at: number;
}

/** config-recovery 事件 / take_config_recovery_notice 返回值
 *  （镜像 Rust persist::config::RecoveryNotice）。 */
export interface RecoveryNotice {
  /** 损坏文件备份到的路径（null = 备份失败） */
  backup_path: string | null;
  reason: string;
}
