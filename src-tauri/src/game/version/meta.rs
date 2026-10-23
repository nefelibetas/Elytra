//! 版本元数据获取：拉取版本 JSON 原始文本（不写盘）与落盘。

use std::path::Path;

use crate::config::downloader::DownloaderConfig;
use crate::game::version::model::VersionJson;
use crate::game::version::source::CacheBehaviour;

/// 多源回退拉取版本 JSON 原始文本（不写盘）。下载流程与任务编排共用。
pub async fn fetch_version_json_text(
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<String, String> {
    config
        .fetch_with_fallback(config.game_source_mode, CacheBehaviour::DefaultTtl, |s| {
            s.mirror_url(version_url)
        })
        .await
        .map_err(common_err_ctx_key!(
            "errors.version.read",
            "获取 version json 失败"
        ))
}

/// 下载并落盘版本 JSON（保留原始文本，避免序列化丢失未知字段）。
pub async fn download_version_json(
    version_url: &str,
    dest_dir: &Path,
    config: &DownloaderConfig,
) -> Result<VersionJson, String> {
    let text = fetch_version_json_text(version_url, config).await?;

    let version_json: VersionJson = serde_json::from_str(&text).map_err(common_err_ctx_key!(
        "errors.version.parse",
        "解析 version json 失败"
    ))?;

    let json_path = dest_dir.join(format!("{}.json", version_json.id));
    std::fs::create_dir_all(dest_dir)
        .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    std::fs::write(&json_path, &text).map_err(common_err_ctx_key!(
        "errors.file.write",
        "写入 version json 失败"
    ))?;

    Ok(version_json)
}
