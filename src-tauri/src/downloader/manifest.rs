use crate::downloader::model::VersionManifest;

pub async fn fetch_version_list(mirror_url: &str) -> Result<VersionManifest, String> {
    let url = format!("{}/mc/game/version_manifest_v2.json", mirror_url);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("failed to fetch version list: {}", e))?;

    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read response: {}", e))?;

    let manifest: VersionManifest =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse manifest: {}", e))?;
    println!(
        "release: {}, snapshot: {}",
        manifest.latest.release, manifest.latest.snapshot
    );
    Ok(manifest)
}
