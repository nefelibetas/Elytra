use std::path::Path;

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher;
use crate::downloader::model::VersionJson;

/// 多源回退拉取版本 JSON 原始文本（不写盘）。下载流程与任务编排共用。
pub async fn fetch_version_json_text(
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<String, String> {
    config
        .fetch_with_fallback(config.file_source_mode, |s| s.mirror_url(version_url))
        .await
        .map_err(|e| format!("获取 version json 失败: {e}"))
}

/// 下载并落盘版本 JSON（保留原始文本，避免序列化丢失未知字段）。
pub async fn download_version_json(
    version_url: &str,
    dest_dir: &Path,
    config: &DownloaderConfig,
) -> Result<VersionJson, String> {
    let text = fetch_version_json_text(version_url, config).await?;

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
