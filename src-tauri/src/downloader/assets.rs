use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
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

    let indexes_dir = mc_dir.join("assets").join("indexes");
    let index_path = indexes_dir.join(format!("{}.json", index_id));

    let index_text = match index_sha1 {
        Some(sha1) if index_path.exists() && file_sha1_matches(&index_path, sha1) => {
            std::fs::read_to_string(&index_path)
                .map_err(|e| vec![format!("failed to read cached asset index: {}", e)])?
        }
        _ => {
            // 资源索引：主源过慢（5s）自动降级到备用源
            let text = config
                .fetch_with_fallback(config.file_source_mode, |s| s.mirror_url(index_url))
                .await
                .map_err(|e| vec![format!("failed to fetch asset index: {}", e)])?;
            std::fs::create_dir_all(&indexes_dir)
                .map_err(|e| vec![format!("failed to create indexes dir: {}", e)])?;
            std::fs::write(&index_path, &text)
                .map_err(|e| vec![format!("failed to write asset index: {}", e)])?;
            text
        }
    };

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
                if attempt > 0 { sleep(Duration::from_millis(200)).await; }
                match fetcher::fetch_fallback(&urls, &dest, None::<fn(u64, u64)>).await {
                    Ok(dl) => {
                        if let Some(ref c) = ctr { c.fetch_add(1, Ordering::Relaxed); }
                        if let Some(ref b) = bc { b.fetch_add(dl.bytes, Ordering::SeqCst); }
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
