use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use super::hash::file_sha1_matches;
use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::fetcher;

#[derive(Debug, Serialize, Deserialize)]
pub struct AssetsIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// 拉取并缓存资源索引（本地缓存 sha1 匹配则直接读，不重新拉取）。
/// `index_id` 为 None 时不读缓存也不写缓存（仅拉取，供缺失统计等场景）。
pub async fn fetch_asset_index(
    config: &DownloaderConfig,
    mc_dir: &Path,
    index_url: &str,
    index_id: Option<&str>,
    index_sha1: Option<&str>,
) -> Result<String, String> {
    let index_path = index_id.map(|id| {
        mc_dir
            .join("assets")
            .join("indexes")
            .join(format!("{}.json", id))
    });
    if let (Some(path), Some(sha1)) = (&index_path, index_sha1) {
        if path.exists() && file_sha1_matches(path, sha1) {
            return std::fs::read_to_string(path)
                .map_err(|e| format!("读取缓存资源索引失败: {}", e));
        }
    }

    // 资源索引：主源过慢（5s）自动降级到备用源
    let text = config
        .fetch_with_fallback(config.file_source_mode, |s| s.mirror_url(index_url))
        .await
        .map_err(|e| format!("获取资源索引失败: {e}"))?;

    if let Some(path) = &index_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建索引目录失败: {}", e))?;
        }
        std::fs::write(path, &text).map_err(|e| format!("写入资源索引失败: {}", e))?;
    }
    Ok(text)
}

pub async fn download_assets(
    index_url: &str,
    index_id: &str,
    index_sha1: Option<&str>,
    config: &DownloaderConfig,
    mc_dir: &Path,
    counter: Option<Arc<AtomicU32>>,
    bytes_counter: Option<Arc<AtomicU64>>,
) -> Result<(), Vec<String>> {
    let sources: Vec<&crate::config::downloader::model::DownloadSource> =
        config.file_sources_list();

    let index_text = fetch_asset_index(config, mc_dir, index_url, Some(index_id), index_sha1)
        .await
        .map_err(|e| vec![e])?;

    let index: AssetsIndex = serde_json::from_str(&index_text)
        .map_err(|e| vec![format!("failed to parse asset index: {}", e)])?;

    let objects_dir = mc_dir.join("assets").join("objects");
    let mut handles = Vec::new();
    let mut errors = Vec::new();

    for obj in index.objects.values() {
        if errors.len() >= 10 {
            errors.push("too many errors, stopping".into());
            break;
        }
        let hash = obj.hash.clone();
        let sub = hash[..2].to_string();
        let dest = objects_dir.join(&sub).join(&hash);
        if dest.exists() && file_sha1_matches(&dest, &hash) {
            continue;
        }

        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级
        let urls: Vec<String> = sources
            .iter()
            .map(|s| format!("{}/{}/{}", s.resource_url, sub, hash))
            .collect();
        let ctr = counter.clone();
        let bc = bytes_counter.clone();
        handles.push(tokio::spawn(async move {
            let mut last_err = String::new();
            for attempt in 0..3 {
                if attempt > 0 {
                    sleep(Duration::from_millis(200)).await;
                }
                // 字节级进度：增量累加进全局计数（每次重试从 0 重新开始，增量不会为负）
                let progress: Option<_> = bc.as_ref().map(|_| {
                    let file_done = Arc::new(AtomicU64::new(0));
                    let fd = file_done.clone();
                    let b = bc.clone();
                    move |downloaded: u64, _total: u64| {
                        let prev = fd.swap(downloaded, Ordering::Relaxed);
                        if let Some(b) = &b {
                            b.fetch_add(downloaded.saturating_sub(prev), Ordering::Relaxed);
                        }
                    }
                });
                match fetcher::fetch_fallback(&urls, &dest, progress).await {
                    Ok(_dl) => {
                        if let Some(c) = &ctr {
                            c.fetch_add(1, Ordering::Relaxed);
                        }
                        return Ok(());
                    }
                    Err(e) => last_err = format!("{}: {}", hash, e),
                }
            }
            Err(last_err)
        }));
    }

    for handle in handles {
        if let Err(e) = handle.await.unwrap() {
            errors.push(e);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
