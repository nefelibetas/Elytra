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
        // URL 为空的库（如 Forge client，由安装器 client.lzma 解压提供）无法下载，
        // 但必须存在且 sha1 匹配——缺失说明安装不完整，应提示重装而非放行
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) { return false; }
        true
    }).count()
}

/// 缺失/校验失败的支持库路径清单（与 count_pending 同过滤条件）。
/// 供完整性检测/启动前检查展示具体缺什么，便于用户复制排查。
pub fn missing_libraries(libraries: &[Library], lib_dir: &Path) -> Vec<String> {
    libraries
        .iter()
        .filter_map(|lib| {
            if !should_download(lib) {
                return None;
            }
            let artifact = lib.downloads.as_ref()?.artifact.as_ref()?;
            let dest = lib_dir.join(&artifact.path);
            if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
                return None;
            }
            Some(artifact.path.clone())
        })
        .collect()
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
    step_bytes: Arc<AtomicU64>,
) -> Result<(), Vec<String>> {
    let mut tasks: Vec<(Vec<String>, std::path::PathBuf, String)> = Vec::new();
    // 安装器内嵌提供、无法网络下载的库（如 Forge client.lzma）缺失时的错误
    let mut internal_missing: Vec<String> = Vec::new();

    for lib in libraries {
        if !should_download(lib) {
            continue;
        }
        let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let dest = libraries_dir.join(&artifact.path);
        // URL 为空：由安装器内嵌（如 Forge client.lzma）提供，不在此下载；
        // 文件缺失时无法补救，明确报错提示重装（修复任务不再静默"完成"）
        if artifact.url.is_empty() {
            if !(dest.exists() && file_sha1_matches(&dest, &artifact.sha1)) {
                internal_missing.push(format!(
                    "{}: 该库由安装器内嵌提供，网络无法下载，请重新安装对应版本",
                    dest.display()
                ));
            }
            continue;
        }
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            continue;
        }
        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级
        let urls: Vec<String> = sources.iter().map(|s| s.mirror_url(&artifact.url)).collect();
        tasks.push((urls, dest, artifact.sha1.clone()));
    }

    if !internal_missing.is_empty() {
        return Err(internal_missing);
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
        let sb = step_bytes.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await.unwrap();
            // 跨任务互斥：不同版本任务共用 libraries 目录，防并发写同一文件互相踩踏
            let _file_lock = crate::task::wait_path_lock(&dest).await;
            // 字节级进度：把本文件已下载字节的增量累加进全局计数（而非完成时整体累加），
            // 让任务层能算出平滑的实时速度。重试时进度从 0 重新开始，saturating_sub 保证不减。
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
            // 逐源 sha1 校验下载：某源返回内容不匹配（镜像缓存陈旧）自动换下一源重试，
            // 全部源均不匹配才报错。校验失败时 fetch_verified 内部已删除坏文件。
            match fetcher::fetch_verified(&urls, &dest, Some(progress), &sha1).await {
                Ok(_dl) => {
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
