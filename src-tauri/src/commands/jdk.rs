use crate::jdk::model::{JdkInfo, JdkOptions};
use crate::jdk::{self, scanner};

#[tauri::command]
pub async fn auto_scan_jdk() -> Vec<JdkInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        let found = scanner::auto_scan();
        jdk::register_scanned(found.clone());
        found
    })
    .await
    .unwrap_or_default()
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
pub async fn scan_custom_jdk(path: String) -> Result<Vec<JdkInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let found = scanner::scan_custom_dir(&path)?;
        jdk::register_scanned(found.clone());
        Ok::<_, String>(found)
    })
    .await
    .map_err(common_err_ctx_key!("errors.jdk.scan", "JDK 扫描失败"))?
}

#[tauri::command]
pub fn save_jdk_options(options: JdkOptions) -> Result<(), String> {
    jdk::save(&options)
}
