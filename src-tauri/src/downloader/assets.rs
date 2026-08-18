use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::hash::file_sha1_matches;
use super::reuse::{classify_local_artifact, copy_verified_local, LocalRuntimeSource};
use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::fetcher;

/// 资源下载失败止损阈值：超过即停（资源数庞大，大量失败通常是源整体不可达，
/// 继续下发只是空耗——与 libraries 的"收齐错误"语义相反，那是可修复的个别缺失）。
const MAX_ASSET_ERRORS: usize = 10;

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
        // 单次读取 + 校验：命中直接用读到的内容返回（file_sha1_matches 已读一次，
        // 避免二次读取；内容不匹配或读取失败走网络）
        if path.exists() {
            if let Ok(data) = std::fs::read(path) {
                if crate::downloader::hash::sha1_hex(&data) == sha1 {
                    eprintln!("[assets] 缓存命中: {}", path.display());
                    return String::from_utf8(data)
                        .map_err(|e| format!("读取缓存资源索引失败\n{e}"));
                }
            }
        }
        eprintln!("[assets] 缓存无效或无缓存: {}", path.display());
    }

    // 资源索引：主源过慢（5s）自动降级到备用源
    eprintln!("[assets] 无有效缓存，走网络 fetch_with_fallback");
    let text = config
        .fetch_with_fallback(
            config.file_source_mode,
            crate::downloader::source::CacheBehaviour::DefaultTtl,
            |s| s.mirror_url(index_url),
        )
        .await
        .map_err(|e| format!("获取资源索引失败\n{e}"))?;
    eprintln!("[assets] 索引文本获取完成: {}B", text.len());

    if let Some(path) = &index_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建索引目录失败\n{}", e))?;
        }
        std::fs::write(path, &text).map_err(|e| format!("写入资源索引失败\n{}", e))?;
    }
    Ok(text)
}

/// 批量下载资源文件（objects/{sub}/{hash}，逐文件 sha1 校验）。
/// `reuse`：可选本地运行时复用根——dest 缺失且复用根有 size+sha1 匹配的同 hash 文件时
/// 本地复制（计入进度）而非网络下载（迁移场景 assets 数百 MB 免重下）。
#[allow(clippy::too_many_arguments)]
pub async fn download_assets(
    index_url: &str,
    index_id: &str,
    index_sha1: &str,
    config: &DownloaderConfig,
    concurrency: usize,
    reuse: Option<LocalRuntimeSource>,
    pc: &crate::downloader::progress::ProgressCounters,
    cancel: Option<&Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), Vec<String>> {
    let sources: Vec<&crate::config::downloader::model::DownloadSource> =
        config.file_sources_list();
    let mc_dir = config.minecraft_dir_abs();

    let index_text = fetch_asset_index(config, &mc_dir, index_url, Some(index_id), Some(index_sha1))
        .await
        .map_err(|e| vec![e])?;

    let index: AssetsIndex = serde_json::from_str(&index_text)
        .map_err(|e| vec![format!("解析资源索引失败\n{}", e)])?;

    let objects_dir = mc_dir.join("assets").join("objects");
    let sem = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::new();
    let mut errors = Vec::new();

    for obj in index.objects.values() {
        if errors.len() >= MAX_ASSET_ERRORS {
            errors.push("too many errors, stopping".into());
            break;
        }
        let hash = obj.hash.clone();
        let obj_size = obj.size; // 提前取值：spawn 的 async move 不能借用 index
        let sub = hash[..2].to_string();
        let dest = objects_dir.join(&sub).join(&hash);
        if dest.exists() && file_sha1_matches(&dest, &hash) {
            continue;
        }

        // 本地运行时复用：dest 缺失时先问复用根（hash 即 sha1，size 校验 + 流式 sha1）。
        // 命中 → 复制；未命中 → 网络下载（多源 URL 照常构造）。
        let local = reuse.as_ref().and_then(|r| {
            classify_local_artifact(r, &format!("assets/objects/{}/{}", sub, hash), Some(obj_size), Some(&hash))
        });
        let urls: Vec<String> = sources
            .iter()
            .map(|s| format!("{}/{}/{}", s.resource_url, sub, hash))
            .collect();
        let ctr = pc.files.clone();
        let bc = pc.bytes.clone();
        let sb = pc.step_bytes.clone();
        let permit = sem.clone();
        let cancel = cancel.cloned();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            // 跨任务互斥：多任务共用 assets 目录，防并发写同一资源互相踩踏
            let _file_lock = crate::task::wait_path_lock(&dest).await;
            // 字节级进度：增量累加进全局计数（重试从 0 重新开始，增量不会为负）
            let progress = {
                let file_done = Arc::new(AtomicU64::new(0));
                let fd = file_done.clone();
                let b = bc.clone();
                let s = sb.clone();
                move |downloaded: u64, _total: u64| {
                    let prev = fd.swap(downloaded, Ordering::Relaxed);
                    let delta = downloaded.saturating_sub(prev);
                    b.fetch_add(delta, Ordering::Relaxed);
                    s.fetch_add(delta, Ordering::Relaxed);
                }
            };
            // 复用分支：本地复制；失败降级为网络下载，不致命
            if let Some(src) = local {
                match copy_verified_local(&src, &dest, obj_size, Some(progress.clone())).await {
                    Ok(_) => {
                        ctr.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(e) => {
                        match fetcher::fetch_verified_c(
                            &urls,
                            &dest,
                            Some(progress),
                            &hash,
                            cancel.as_deref(),
                        )
                        .await
                        {
                            Ok(_) => {
                                ctr.fetch_add(1, Ordering::Relaxed);
                                Ok(())
                            }
                            Err(e2) => Err(format!("{}: {}（本地复用失败: {}）", hash, e2, e)),
                        }
                    }
                }
            } else {
                // 逐源 sha1 校验下载：内容不匹配自动换下一源/下一轮重试（镜像缓存陈旧场景）
                match fetcher::fetch_verified_c(
                    &urls,
                    &dest,
                    Some(progress),
                    &hash,
                    cancel.as_deref(),
                )
                .await
                {
                    Ok(_) => {
                        ctr.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(e) => Err(format!("{}: {}", hash, e)),
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::downloader::model::DownloaderConfig;

    /// 端到端（B2 路径）：fetch_asset_index 经系统代理/直连拉取官方资源索引应成功。
    /// 网络测试（与 fetch_manifest_works 同惯例）：开发环境可达；离线/黑洞环境超时报错。
    #[tokio::test]
    async fn fetch_asset_index_end_to_end() {
        let cfg = DownloaderConfig::default();
        let mc = std::env::temp_dir().join(format!("elytra_assets_e2e_{}", std::process::id()));
        let url =
            "https://piston-meta.mojang.com/v1/packages/cf75b185cb35b32e299b0c8e674fa202d7911a3c/32.json";
        let sha1 = "cf75b185cb35b32e299b0c8e674fa202d7911a3c";
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            fetch_asset_index(&cfg, &mc, url, Some("32"), Some(sha1)),
        )
        .await;
        let text = match res {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => panic!("asset index 获取失败: {e}"),
            Err(_) => panic!("asset index 20s 超时（网络或代理不可达）"),
        };
        assert!(text.len() > 400_000, "asset index 应完整（约 586KB）: {}B", text.len());
        // 清理测试目录（避免留缓存）
        let _ = std::fs::remove_dir_all(&mc);
    }
}
