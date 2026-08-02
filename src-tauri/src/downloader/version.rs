use std::path::Path;

use crate::config::downloader::model::DownloadSource;
use crate::downloader::fetcher::{self, http};
use crate::downloader::model::VersionJson;
pub async fn download_version_json(
    version_url: &str,
    dest_dir: &Path,
    sources: &[&DownloadSource],
) -> Result<VersionJson, String> {
    let urls: Vec<String> = sources.iter().map(|s| s.mirror_url(version_url)).collect();
    let mut last_err = String::new();
    let mut text = String::new();
    for url in &urls {
        match http().get(url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(t) => { text = t; break; }
                Err(e) => last_err = format!("读取 version json 失败: {}", e),
            },
            Err(e) => last_err = format!("获取 version json 失败: {}", e),
        }
    }
    if text.is_empty() {
        return Err(last_err);
    }

    let version_json: VersionJson =
        serde_json::from_str(&text).map_err(|e| format!("解析 version json 失败: {}", e))?;

    let json_path = dest_dir.join(format!("{}.json", version_json.id));
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    std::fs::write(&json_path, &text).map_err(|e| format!("写入 version json 失败: {}", e))?;

    Ok(version_json)
}

pub async fn download_client_jar(
    client_url: &str,
    version_name: &str,
    dest_dir: &Path,
    sources: &[&DownloadSource],
) -> Result<(), String> {
    let urls: Vec<String> = sources.iter().map(|s| s.mirror_url(client_url)).collect();
    let jar_path = dest_dir.join(format!("{}.jar", version_name));
    fetcher::fetch_fallback(&urls, &jar_path, None::<fn(u64, u64)>)
        .await
        .map_err(|e| format!("下载 client jar 失败: {}", e))?;
    Ok(())
}
