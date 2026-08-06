use crate::config::downloader::model::DownloaderConfig;
use crate::constants;
use crate::downloader::manifest;
use crate::downloader::model::VersionManifest;
use crate::downloader::resolve_version_dir;
use crate::persist;

#[tauri::command]
pub async fn get_version_list() -> Result<VersionManifest, String> {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    manifest::fetch_version_list(&config).await
}

#[tauri::command]
pub fn get_downloader_config() -> DownloaderConfig {
    persist::read(constants::SECT_DOWNLOADER).unwrap_or_default()
}

#[tauri::command]
pub fn save_downloader_config(config: DownloaderConfig) -> Result<(), String> {
    persist::save(constants::SECT_DOWNLOADER, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_installed_versions() -> Result<Vec<String>, String> {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let versions_dir = config.minecraft_dir_abs().join("versions");
    if !versions_dir.exists() {
        return Ok(vec![]);
    }
    let mut items: Vec<String> = std::fs::read_dir(&versions_dir)
        .map_err(|e| format!("读取版本目录失败: {}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                Some(entry.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    items.sort();
    Ok(items)
}

#[tauri::command]
pub fn resolve_version_name(base_name: String) -> Result<String, String> {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let mc = config.minecraft_dir_abs();
    let (name, _) = resolve_version_dir(&mc, &base_name);
    Ok(name)
}
