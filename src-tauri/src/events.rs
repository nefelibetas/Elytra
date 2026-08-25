//! 前后端共享的事件名常量（前端 src/store/events.ts 手写镜像，改这里必须同步）。

/// 下载任务进度推送（TaskProgress JSON）
pub const TASK_PROGRESS: &str = "task-progress";
/// 微软登录完成回调（{ ok, account, error }）
pub const MS_LOGIN_COMPLETE: &str = "ms-login-complete";
/// 游戏进程退出（[pid, version_label] 元组）
pub const GAME_EXITED: &str = "game-exited";
/// 游戏主窗口可见（[pid, version_label] 元组；spawn 后后端轮询 EnumWindows 判定）
pub const GAME_WINDOW_VISIBLE: &str = "game-window-visible";
/// 批量文件下载失败挂起，等待用户决策（{ task_id, file_name, error }）
pub const TASK_WAITING_USER: &str = "task-waiting-user";
/// 联机状态快照（OnlineSnapshot：{ status, room_code?, local_port? }）
pub const ONLINE_STATE: &str = "online-state";