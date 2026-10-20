use crate::launcher::memory;
use crate::launcher::model;
use crate::persist;

/// 系统内存信息（MB）：总内存 + 当前可用内存。
#[tauri::command]
pub fn get_memory_info() -> (u64, u64) {
    memory::system_memory_mb()
}

#[tauri::command]
pub fn get_launch_options() -> model::LaunchOptions {
    persist::launch_options()
}

#[tauri::command]
pub fn save_launch_options(config: model::LaunchOptions) -> Result<(), String> {
    persist::save_launch_options(&config).map_err(common_err_ctx_key!("errors.config.write", "保存配置失败"))
}
