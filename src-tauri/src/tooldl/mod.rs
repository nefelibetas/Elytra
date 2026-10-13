//! 工具箱「多线程下载器」域：独立于启动器下载任务（`task/`）的一组 URL 下载任务。
//!
//! 实现划分（分段内核复用 [`crate::downloader::segmented`]）：[`model`] 任务状态与对外快照 /
//! [`manager`] 注册表、并发槽位调度与状态操作 / [`runner`] 单任务执行体（分段下载 + 进度心跳 + 终态收口）。
//! 进度以单任务快照事件 [`crate::events::TOOL_DL_PROGRESS`] 推送，前端按 id 增量更新卡片。

mod manager;
mod model;
mod runner;

pub use manager::{
    cancel, clear_finished, enqueue, init, list, pause, remove, resume, set_max_files,
};
pub use model::{ToolDlSnapshot, ToolDlStatus};
