//! 工具箱「多线程下载器」命令（转调 `tooldl` 域与 `persist`）。

use std::sync::Arc;

use tauri::AppHandle;

use crate::config::tooldl::ToolDlConfig;
use crate::task::EventSink;
use crate::tooldl::{self, ToolDlSnapshot};
use crate::persist;

#[tauri::command]
pub async fn tooldl_enqueue(
    app: AppHandle,
    urls: Vec<String>,
    save_dir: String,
    segments: u32,
) -> Result<Vec<String>, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app);
    tooldl::enqueue(sink, urls, save_dir, segments)
}

/// 暂停正在下载的任务（保留断点）。
#[tauri::command]
pub fn tooldl_pause(task_id: String) -> Result<(), String> {
    tooldl::pause(&task_id)
}

#[tauri::command]
pub async fn tooldl_resume(app: AppHandle, task_id: String) -> Result<(), String> {
    let sink: Arc<dyn EventSink> = Arc::new(app);
    tooldl::resume(sink, &task_id)
}

/// 取消任务并清理中间产物。
#[tauri::command]
pub fn tooldl_cancel(task_id: String) -> Result<(), String> {
    tooldl::cancel(&task_id)
}

#[tauri::command]
pub fn tooldl_remove(task_id: String) -> Result<(), String> {
    tooldl::remove(&task_id)
}

#[tauri::command]
pub fn tooldl_clear_finished() {
    tooldl::clear_finished();
}

#[tauri::command]
pub fn tooldl_list() -> Vec<ToolDlSnapshot> {
    tooldl::list()
}

#[tauri::command]
pub fn tooldl_get_config() -> ToolDlConfig {
    persist::tooldl_config()
}

/// 保存配置（保存目录 / 并发文件数 / 分段数）；并发上限同步到调度缓存。
#[tauri::command]
pub fn tooldl_save_config(config: ToolDlConfig) -> Result<(), String> {
    persist::save_tooldl_config(&config).map_err(common_err_ctx!("保存配置失败"))?;
    tooldl::set_max_files(config.max_files);
    Ok(())
}
