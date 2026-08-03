// 前后端共享的事件名常量（Rust 侧 src-tauri/src/events.rs 手写镜像，改这里必须同步）。

/** 下载任务进度推送（ProgressPayload） */
export const EVT_TASK_PROGRESS = "task-progress";
/** 微软登录完成回调（{ ok, account, error }） */
export const EVT_MS_LOGIN_COMPLETE = "ms-login-complete";
/** 游戏进程退出（[pid, versionLabel] 元组） */
export const EVT_GAME_EXITED = "game-exited";
