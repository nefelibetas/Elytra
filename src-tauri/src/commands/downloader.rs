use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::manifest;
use crate::downloader::model::VersionManifest;
use crate::persist;

#[tauri::command]
pub async fn get_version_list() -> Result<VersionManifest, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    manifest::fetch_version_list(&config).await
}

#[tauri::command]
pub async fn download_version(version_id: String, version_url: String) -> Result<(), String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    crate::downloader::download_version(&version_id, &version_url, &config)
        .await
        .map_err(|errors| errors.join("\n"))
}

#[tauri::command]
pub fn get_downloader_config() -> DownloaderConfig {
    persist::read("downloader").unwrap_or_default()
}

#[tauri::command]
pub fn save_downloader_config(config: DownloaderConfig) -> Result<(), String> {
    persist::save("downloader", &config).map_err(|e| e.to_string())
}
