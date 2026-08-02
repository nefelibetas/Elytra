use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::installer;
use crate::persist;

#[tauri::command]
pub async fn install_fabric(mc_version: String, loader_version: String) -> Result<String, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let result = installer::install_fabric(&mc_version, &loader_version, &config).await?;
    Ok(result.version_dir_name)
}

#[tauri::command]
pub async fn install_forge(mc_version: String, forge_version: String) -> Result<String, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let result = installer::install_forge(&mc_version, &forge_version, &config).await?;
    Ok(result.version_dir_name)
}

#[tauri::command]
pub async fn install_neoforge(mc_version: String, neo_version: String) -> Result<String, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let result = installer::install_neoforge(&mc_version, &neo_version, &config).await?;
    Ok(result.version_dir_name)
}

#[tauri::command]
pub async fn install_optifine(
    mc_version: String,
    patch: String,
    of_type: String,
) -> Result<String, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let result = installer::install_optifine(&mc_version, &patch, &of_type, &config).await?;
    Ok(result.version_dir_name)
}
