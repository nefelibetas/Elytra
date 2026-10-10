use crate::config::downloader::DownloaderConfig;
use crate::downloader::fetcher;
use crate::game::version::dir;
use crate::game::version::dir::resolve_version_dir;
use crate::game::version::manifest;
use crate::game::version::model::VersionManifest;
use crate::persist::{self, downloader_config};

#[tauri::command]
pub async fn get_version_list() -> Result<VersionManifest, String> {
    let config: DownloaderConfig = downloader_config();
    manifest::fetch_version_list(&config).await
}

#[tauri::command]
pub fn get_downloader_config() -> DownloaderConfig {
    downloader_config()
}

#[tauri::command]
pub fn save_downloader_config(config: DownloaderConfig) -> Result<(), String> {
    let old: DownloaderConfig = downloader_config();
    persist::save_downloader_config(&config).map_err(|e| format!("保存配置失败\n{e}"))?;
    fetcher::set_speed_limit(config.max_speed);
    if old.network_mode != config.network_mode {
        fetcher::reset_http_client();
    }
    Ok(())
}

/// 已安装版本列表（枚举/loader 推断在 downloader/version.rs）。
#[tauri::command]
pub fn get_installed_versions() -> Result<Vec<dir::InstalledVersionInfo>, String> {
    dir::list_installed()
}

#[tauri::command]
pub fn resolve_version_name(base_name: String) -> Result<String, String> {
    let config: DownloaderConfig = downloader_config();
    let mc = config.minecraft_dir_abs();
    let (name, _) = resolve_version_dir(&mc, &base_name);
    Ok(name)
}
