//! 启动选项配置命令 + 系统内存查询。

use crate::constants;
use crate::launcher::model::LauncherConfig;

/// 系统内存信息（MB）：总内存 + 当前可用内存。
#[tauri::command]
pub fn get_memory_info() -> (u64, u64) {
    crate::launcher::memory::system_memory_mb()
}

/// 读取启动选项配置（launcher section）。
#[tauri::command]
pub fn get_launch_options() -> LauncherConfig {
    crate::persist::read::<LauncherConfig>(constants::SECT_LAUNCHER).unwrap_or_default()
}

/// 保存启动选项配置（launcher section）。
#[tauri::command]
pub fn save_launch_options(config: LauncherConfig) -> Result<(), String> {
    crate::persist::save(constants::SECT_LAUNCHER, &config)
        .map_err(|e| format!("保存配置失败\n{e}"))
}
