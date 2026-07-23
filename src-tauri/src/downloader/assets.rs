use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

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
    config: &DownloaderConfig,
    mc_dir: &Path,
) -> Result<(), Vec<String>> {
    let source = config.active_source().ok_or_else(|| vec!["no active source".into()])?;

    let index_text = reqwest::get(index_url)
        .await
        .map_err(|e| vec![format!("failed to fetch asset index: {}", e)])?
        .text()
        .await
        .map_err(|e| vec![format!("failed to read asset index: {}", e)])?;

    let indexes_dir = mc_dir.join("assets").join("indexes");
    std::fs::create_dir_all(&indexes_dir)
        .map_err(|e| vec![format!("failed to create indexes dir: {}", e)])?;
    std::fs::write(indexes_dir.join(format!("{}.json", index_id)), &index_text)
        .map_err(|e| vec![format!("failed to write asset index: {}", e)])?;

    let index: AssetsIndex = serde_json::from_str(&index_text)
        .map_err(|e| vec![format!("failed to parse asset index: {}", e)])?;

    let objects_dir = mc_dir.join("assets").join("objects");
    let sem = Arc::new(Semaphore::new(config.concurrency as usize));
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
        if dest.exists() {
            continue;
        }

        let url = format!("{}/assets/{}/{}", source.resource_url, sub, hash);
        let permit = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            fetcher::fetch(&url, &dest, None::<fn(u64, u64)>)
                .await
                .map_err(|e| format!("{}: {}", hash, e))
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
