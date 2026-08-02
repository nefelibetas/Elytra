use serde::Deserialize;

use crate::config::downloader::model::DownloaderConfig;
use crate::persist;

#[derive(Deserialize)]
struct ForgeVersion { version: String }

#[derive(Deserialize)]
struct FabricEntry { loader: FabricLoader }
#[derive(Deserialize)]
struct FabricLoader { version: String }

#[derive(Deserialize)]
struct QuiltEntry { loader: QuiltLoader }
#[derive(Deserialize)]
struct QuiltLoader { version: String }

#[derive(Deserialize)]
struct NeoForgeEntry { version: String }

#[derive(Deserialize)]
struct OptiFineEntry { patch: String, filename: String }

#[derive(Deserialize)]
struct LiteLoaderEntry { mcversion: String, version: String }

fn config() -> DownloaderConfig {
    persist::read("downloader").unwrap_or_default()
}

async fn fetch_versions<T: serde::de::DeserializeOwned>(
    make_url: impl Fn(&crate::config::downloader::model::DownloadSource) -> String,
) -> Result<Vec<T>, String> {
    let cfg = config();
    cfg.fetch_json_with_fallback::<Vec<T>>(make_url).await
}

#[tauri::command]
pub async fn get_forge_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<ForgeVersion>(|s| {
        s.url(&s.forge_list_url, &[("mc", &mc_version)])
    }).await?;
    let mut versions: Vec<String> = data.into_iter().map(|v| v.version).collect();
    versions.reverse();
    Ok(versions)
}

#[tauri::command]
pub async fn get_fabric_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<FabricEntry>(|s| {
        s.url(&s.fabric_list_url, &[("mc", &mc_version)])
    }).await?;
    Ok(data.into_iter().map(|e| e.loader.version).collect())
}

#[tauri::command]
pub async fn get_quilt_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<QuiltEntry>(|s| {
        s.url(&s.quilt_list_url, &[("mc", &mc_version)])
    }).await?;
    Ok(data.into_iter().map(|e| e.loader.version).collect())
}

#[tauri::command]
pub async fn get_neoforge_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<NeoForgeEntry>(|s| {
        s.url(&s.neoforge_list_url, &[("mc", &mc_version)])
    }).await?;
    let prefix = format!("{}-", mc_version);
    let versions: Vec<String> = data
        .into_iter()
        .filter_map(|e| {
            let v = e.version;
            if v.starts_with(&prefix) { Some(v[prefix.len()..].to_string()) }
            else if v == mc_version { None }
            else { Some(v) }
        })
        .collect();
    Ok(versions)
}

#[tauri::command]
pub async fn get_optifine_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<OptiFineEntry>(|s| {
        s.url(&s.optifine_list_url, &[("mc", &mc_version)])
    }).await?;
    Ok(data.into_iter().filter(|e| !e.filename.starts_with("preview_")).map(|e| e.patch).collect())
}

#[tauri::command]
pub async fn get_liteloader_versions(mc_version: String) -> Result<Vec<String>, String> {
    let cfg = config();
    let data = cfg.fetch_json_with_fallback::<Vec<LiteLoaderEntry>>(|s| {
        s.liteloader_list_url.clone()
    }).await?;
    Ok(data.into_iter().filter(|e| e.mcversion == mc_version).map(|e| e.version).collect())
}
