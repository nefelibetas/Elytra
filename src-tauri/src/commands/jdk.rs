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

/// 下载缺失的 JDK（Mojang 官方 runtime → Azul Zulu 回退），完成后注册进 discovered_jdks。
/// 前端 SettingsJdk 的"下载缺失 JDK"按钮调用；下载在 async 命令内进行，不阻塞主线程。
#[tauri::command]
pub async fn install_jdk(major: u32) -> Result<JdkInfo, String> {
    let config: crate::config::downloader::model::DownloaderConfig =
        crate::persist::read(crate::constants::SECT_DOWNLOADER).unwrap_or_default();
    let info = crate::jdk::download::auto_install(major, &config).await?;
    merge_scanned(vec![info.clone()]);
    Ok(info)
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
