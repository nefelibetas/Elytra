//! ModLoader 版本查询命令（薄胶水，业务在 downloader/mod_loader/version_query.rs）。

use crate::game::loader::version_query;

#[tauri::command]
pub async fn get_forge_versions(mc_version: String) -> Result<Vec<String>, String> {
    version_query::forge_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_fabric_versions(mc_version: String) -> Result<Vec<String>, String> {
    version_query::fabric_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_quilt_versions(mc_version: String) -> Result<Vec<String>, String> {
    version_query::quilt_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_neoforge_versions(mc_version: String) -> Result<Vec<String>, String> {
    version_query::neoforge_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_optifine_versions(mc_version: String) -> Result<Vec<String>, String> {
    version_query::optifine_versions(&mc_version).await
}

#[tauri::command]
pub async fn get_fabric_api_versions(
    mc_version: String,
) -> Result<Vec<version_query::FabricApiVersion>, String> {
    version_query::fabric_api_versions(&mc_version).await
}
