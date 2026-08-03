//! 前后端共享的事件名常量（前端 src/store/events.ts 手写镜像，改这里必须同步）。

/// 下载任务进度推送（TaskProgress JSON）
pub const TASK_PROGRESS: &str = "task-progress";
/// 微软登录完成回调（{ ok, account, error }）
pub const MS_LOGIN_COMPLETE: &str = "ms-login-complete";
/// 游戏进程退出（[pid, version_label] 元组）
pub const GAME_EXITED: &str = "game-exited";
