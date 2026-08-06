use crate::jdk::model::{JdkConfig, JdkInfo};
use crate::jdk::{self, scanner};

#[tauri::command]
pub fn auto_scan_jdk() -> Vec<JdkInfo> {
    let found = scanner::auto_scan();
    merge_scanned(found.clone());
    found
}

#[tauri::command]
pub fn get_jdk_config() -> JdkConfig {
    jdk::read()
}

#[tauri::command]
pub fn scan_custom_jdk(path: String) -> Result<Vec<JdkInfo>, String> {
    let found = scanner::scan_custom_dir(&path)?;
    merge_scanned(found.clone());
    Ok(found)
}

#[tauri::command]
pub fn save_jdk_config(config: JdkConfig) -> Result<(), String> {
    jdk::save(&config)
}

fn merge_scanned(found: Vec<JdkInfo>) {
    if found.is_empty() {
        return;
    }
    let mut cfg = jdk::read();
    for env in found {
        if !cfg.discovered_jdks.iter().any(|e| e.path == env.path) {
            cfg.discovered_jdks.push(env);
        }
    }
    let _ = jdk::save(&cfg);
}
