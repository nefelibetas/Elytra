use crate::jdk::model::{JdkInfo, JdkOptions};
use crate::jdk::{self, scanner};

#[tauri::command]
pub fn auto_scan_jdk() -> Vec<JdkInfo> {
    let found = scanner::auto_scan();
    jdk::register_scanned(found.clone());
    found
}

#[tauri::command]
pub fn get_jdk_options() -> JdkOptions {
    jdk::read()
}

/// 已发现的本机 JDK 列表（扫描缓存，只读）。
#[tauri::command]
pub fn get_discovered_jdks() -> Vec<JdkInfo> {
    jdk::read_cache().jdks
}

#[tauri::command]
pub fn scan_custom_jdk(path: String) -> Result<Vec<JdkInfo>, String> {
    let found = scanner::scan_custom_dir(&path)?;
    jdk::register_scanned(found.clone());
    Ok(found)
}

#[tauri::command]
pub fn save_jdk_options(options: JdkOptions) -> Result<(), String> {
    jdk::save(&options)
}
