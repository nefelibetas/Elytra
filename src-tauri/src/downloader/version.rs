use std::path::Path;

use crate::downloader::fetcher;
use crate::downloader::model::VersionJson;

pub async fn download_version_json(
    version_url: &str,
    dest_dir: &Path,
) -> Result<VersionJson, String> {
    let resp = reqwest::get(version_url)
        .await
        .map_err(|e| format!("failed to fetch version json: {}", e))?;

    let text = resp
        .text()
        .await
        .map_err(|e| format!("failed to read version json: {}", e))?;

    let version_json: VersionJson =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse version json: {}", e))?;

    let json_path = dest_dir.join(format!("{}.json", version_json.id));
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("failed to create dir: {}", e))?;
    std::fs::write(&json_path, &text)
        .map_err(|e| format!("failed to write version json: {}", e))?;

    Ok(version_json)
}

pub async fn download_client_jar(
    client_url: &str,
    version_name: &str,
    dest_dir: &Path,
) -> Result<(), String> {
    let jar_path = dest_dir.join(format!("{}.jar", version_name));
    fetcher::fetch(client_url, &jar_path, None::<fn(u64, u64)>)
        .await
        .map_err(|e| format!("failed to download client jar: {}", e))?;
    Ok(())
}
