use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::model::VersionManifest;

pub async fn fetch_version_list(config: &DownloaderConfig) -> Result<VersionManifest, String> {
    let source = config.active_source().ok_or("no active source")?;
    let url = format!("{}/mc/game/version_manifest_v2.json", source.manifest_url);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("failed to fetch version list: {}", e))?;

    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;

    let manifest: VersionManifest =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse manifest: {}", e))?;
    Ok(manifest)
}
