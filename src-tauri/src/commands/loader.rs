//! ModLoader 版本查询命令（薄胶水，业务在 downloader/mod_loader/version_query.rs）。

use crate::downloader::mod_loader::version_query as vq;

#[tauri::command]
pub async fn get_forge_versions(mc_version: String) -> Result<Vec<String>, String> {
    vq::forge_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_fabric_versions(mc_version: String) -> Result<Vec<String>, String> {
    vq::fabric_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_quilt_versions(mc_version: String) -> Result<Vec<String>, String> {
    vq::quilt_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_neoforge_versions(mc_version: String) -> Result<Vec<String>, String> {
    vq::neoforge_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_optifine_versions(mc_version: String) -> Result<Vec<String>, String> {
    vq::optifine_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_fabric_api_versions(mc_version: String) -> Result<Vec<vq::FabricApiVersion>, String> {
    vq::fabric_api_versions(&mc_version).await
}
