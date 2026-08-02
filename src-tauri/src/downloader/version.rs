use std::path::Path;

use crate::config::downloader::model::DownloadSource;
use crate::downloader::fetcher::{self, http};
use crate::downloader::model::VersionJson;
pub async fn download_version_json(
    version_url: &str,
    dest_dir: &Path,
    source: &DownloadSource,
) -> Result<VersionJson, String> {
    let url = source.mirror_url(version_url);
    let resp = http().get(&url).send().await
        .map_err(|e| format!("获取 version json 失败: {}", e))?;

    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 version json 失败: {}", e))?;

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
    source: &DownloadSource,
) -> Result<(), String> {
    let url = source.mirror_url(client_url);
    let jar_path = dest_dir.join(format!("{}.jar", version_name));
    fetcher::fetch(&url, &jar_path, None::<fn(u64, u64)>)
        .await
        .map_err(|e| format!("下载 client jar 失败: {}", e))?;
    Ok(())
}
