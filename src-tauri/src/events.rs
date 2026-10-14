//! 前后端共享的事件名常量（前端 src/store/events.ts 手写镜像，改这里必须同步）。

/// 下载任务进度推送（TaskProgress JSON）
pub const TASK_PROGRESS: &str = "task-progress";
/// 微软登录完成回调（{ ok, account, error }）
pub const MS_LOGIN_COMPLETE: &str = "ms-login-complete";
/// 游戏进程退出（[pid, version_label, exit_code, crash_report]；exit_code 非 0 = 异常退出，crash_report 为崩溃报告路径或 null）
pub const GAME_EXITED: &str = "game-exited";
/// 游戏主窗口可见（[pid, version_label] 元组；spawn 后后端轮询 EnumWindows 判定）
pub const GAME_WINDOW_VISIBLE: &str = "game-window-visible";
/// 批量文件下载失败挂起，等待用户决策（{ task_id, file_name, error }）
pub const TASK_WAITING_USER: &str = "task-waiting-user";
/// 联机状态快照（OnlineSnapshot：{ status, room_code, local_port, players }）
pub const ONLINE_STATE: &str = "online-state";
/// 工具箱下载器任务快照（ToolDlSnapshot：单任务全量状态，前端按 id 增量更新）
pub const TOOL_DL_PROGRESS: &str = "tooldl-progress";
/// 配置文件损坏已备份并重置（RecoveryNotice；运行期推送，启动期命令拉取兜底）
pub const CONFIG_RECOVERY: &str = "config-recovery";
/// 自动任务进度（AutoTaskProgress：{ version_id, phase, step_index, step_total, action, message, done }）
pub const AUTO_TASK_PROGRESS: &str = "auto-task-progress";
/// 自动任务阶段结束（AutoTasksFinished：{ version_id, phase, failures }，前端据此弹 dialog）
pub const AUTO_TASKS_FINISHED: &str = "auto-tasks-finished";
/// 版本列表变化（VersionsChanged：{ dir, versions }；`versions/` 一级条目或版本 json 变化）
pub const VERSIONS_CHANGED: &str = "versions-changed";
/// 聚焦版本目录内资源子目录变化（VersionDirChanged：{ version, kinds }）
pub const VERSION_DIR_CHANGED: &str = "version-dir-changed";
