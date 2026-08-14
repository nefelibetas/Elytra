// 前后端共享的事件名常量（Rust 侧 src-tauri/src/events.rs 手写镜像，改这里必须同步）。

/** 下载任务进度推送（ProgressPayload） */
export const EVT_TASK_PROGRESS = "task-progress";
/** 批量文件下载失败挂起，等待用户决策（{ task_id, file_name, error }） */
export const EVT_TASK_WAITING_USER = "task-waiting-user";
/** 微软登录完成回调（{ ok, account, error }） */
export const EVT_MS_LOGIN_COMPLETE = "ms-login-complete";
/** 游戏进程退出（[pid, versionLabel, exitCode, crashReport]；exitCode 非 0 = 异常退出，crashReport 为最新崩溃报告路径或 null） */
export const EVT_GAME_EXITED = "game-exited";
/** 游戏主窗口可见（[pid, versionLabel]；后端 EnumWindows 轮询判定） */
export const EVT_GAME_WINDOW_VISIBLE = "game-window-visible";
