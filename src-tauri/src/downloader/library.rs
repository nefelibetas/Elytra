use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::extract::{matches_rules, modern_native_matches, NativesKind};
use super::hash::file_sha1_matches;
use crate::config::downloader::model::DownloadSource;
use crate::downloader::fetcher;
use crate::downloader::model::Library;

fn should_download(lib: &Library) -> bool {
    if let Some(rules) = &lib.rules {
        if !matches_rules(rules) {
            return false;
        }
    }
    // 新式 natives：库名后缀过滤非当前平台的架构变体；旧式/普通库无需额外判定
    if lib.natives_kind() == Some(NativesKind::Modern) && !modern_native_matches(&lib.name) {
        return false;
    }
    true
}

/// Count how many libraries still need downloading (filtering + existence + sha1 check).
pub fn count_pending(libraries: &[Library], lib_dir: &Path) -> usize {
    libraries.iter().filter(|lib| {
        if !should_download(lib) { return false; }
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else { return false; };
        let dest = lib_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) { return false; }
        true
    }).count()
}

/// Count libraries that need to be present (passes filter + has artifact), regardless of local state.
pub fn count_total(libraries: &[Library]) -> usize {
    libraries.iter().filter(|lib| {
        if !should_download(lib) { return false; }
        lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()).is_some()
    }).count()
}

/// 全部应下载库的字节合计（与 count_total 同过滤条件）。
pub fn total_lib_bytes(libraries: &[Library]) -> u64 {
    libraries.iter().filter_map(|lib| {
        if !should_download(lib) { return None; }
        lib.downloads.as_ref()?.artifact.as_ref().map(|a| a.size)
    }).sum()
}

/// 本地已存在且 sha1 匹配的库字节合计（与 count_pending 同过滤条件，取反）。
pub fn existing_lib_bytes(libraries: &[Library], lib_dir: &Path) -> u64 {
    libraries.iter().filter_map(|lib| {
        if !should_download(lib) { return None; }
        let artifact = lib.downloads.as_ref()?.artifact.as_ref()?;
        let dest = lib_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            Some(artifact.size)
        } else {
            None
        }
    }).sum()
}

pub async fn download_libraries(
    libraries: &[Library],
    libraries_dir: &Path,
    concurrency: usize,
    sources: &[&DownloadSource],
    counter: Arc<AtomicU32>,
    bytes_counter: Arc<AtomicU64>,
) -> Result<(), Vec<String>> {
    let mut tasks: Vec<(Vec<String>, std::path::PathBuf, String)> = Vec::new();

    for lib in libraries {
        if !should_download(lib) {
            continue;
        }
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let dest = libraries_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            continue;
        }
        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级
        let urls: Vec<String> = sources.iter().map(|s| s.mirror_url(&artifact.url)).collect();
        tasks.push((urls, dest, artifact.sha1.clone()));
    }

    if tasks.is_empty() {
        return Ok(());
    }

    let sem = Arc::new(Semaphore::new(concurrency));
    let mut errors = Vec::new();
    let mut handles = Vec::new();

    for (urls, dest, sha1) in tasks {
        let permit = sem.clone();
        let ctr = counter.clone();
        let bc = bytes_counter.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            // 字节级进度：把本文件已下载字节的增量累加进全局计数（而非完成时整体累加），
            // 让任务层能算出平滑的实时速度。重试时进度从 0 重新开始，saturating_sub 保证不减。
            let progress = {
                let file_done = Arc::new(AtomicU64::new(0));
                let fd = file_done.clone();
                let b = bc.clone();
                move |downloaded: u64, _total: u64| {
                    let prev = fd.swap(downloaded, Ordering::Relaxed);
                    b.fetch_add(downloaded.saturating_sub(prev), Ordering::Relaxed);
                }
            };
            match fetcher::fetch_fallback(&urls, &dest, Some(progress)).await {
                Ok(_dl) => {
                    // 下载后 sha1 校验（镜像源可能返回坏内容）：不匹配删除并报错，
                    // 下次修复/下载会重新获取
                    if !file_sha1_matches(&dest, &sha1) {
                        let _ = std::fs::remove_file(&dest);
                        return Err(format!("{}: sha1 校验失败", dest.display()));
                    }
                    ctr.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(e) => Err(format!("{}: {}", dest.display(), e)),
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
