use crate::config::jdk::model::JavaConfig;
use crate::config::jdk::model::JavaEnv;
use crate::config::jdk::{scanner, storage};

#[tauri::command]
pub fn auto_scan_jdk() -> Vec<JavaEnv> {
    let found = scanner::auto_scan();
    storage::merge_scanned(found.clone());
    found
}

#[tauri::command]
pub fn get_jdk_config() -> JavaConfig {
    storage::read()
}

#[tauri::command]
pub fn scan_custom_jdk(path: String) -> Result<Vec<JavaEnv>, String> {
    let found = scanner::scan_custom_dir(&path)?;
    storage::merge_scanned(found.clone());
    Ok(found)
}

#[tauri::command]
pub fn save_jdk_config(config: JavaConfig) -> Result<(), String> {
    storage::save(&config).map_err(|e| e.to_string())
}
