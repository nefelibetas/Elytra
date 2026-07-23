mod error;
mod model;
mod scanner;
mod storage;

pub use model::{JavaConfig, JavaEnv};

#[tauri::command]
pub fn auto_scan_jdk() -> Vec<JavaEnv> {
    scanner::auto_scan()
}

#[tauri::command]
pub fn get_jdk_config() -> JavaConfig {
    storage::read()
}

#[tauri::command]
pub fn scan_custom_jdk(path: String) -> Result<Vec<JavaEnv>, String> {
    scanner::scan_custom_dir(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_jdk_config(config: JavaConfig) -> Result<(), String> {
    storage::save(&config).map_err(|e| e.to_string())
}
