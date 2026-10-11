use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::extract::{matches_rules, modern_native_matches, NativesKind};
use crate::config::downloader::DownloadSource;
use crate::downloader::batch::{self, BatchItem};
use crate::downloader::hash::file_sha1_matches;
use crate::downloader::progress::ProgressCounters;
use crate::downloader::reuse::{classify_local_artifact, LocalRuntimeSource};
use crate::game::version::model::Library;

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
    libraries
        .iter()
        .filter(|lib| {
            if !should_download(lib) {
                return false;
            }
            let Some(artifact) = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
                return false;
            };
            let dest = lib_dir.join(&artifact.path);
            // URL 为空的库（如 Forge client，由安装器 client.lzma 解压提供）无法下载，
            // 但必须存在且 sha1 匹配——缺失说明安装不完整，应提示重装而非放行
            if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
                return false;
            }
            true
        })
        .count()
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
    libraries
        .iter()
        .filter(|lib| {
            if !should_download(lib) {
                return false;
            }
            lib.downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .is_some()
        })
        .count()
}

/// 全部应下载库的字节合计（与 count_total 同过滤条件）。
pub fn total_lib_bytes(libraries: &[Library]) -> u64 {
    libraries
        .iter()
        .filter_map(|lib| {
            if !should_download(lib) {
                return None;
            }
            lib.downloads.as_ref()?.artifact.as_ref().map(|a| a.size)
        })
        .sum()
}

/// 本地已存在且 sha1 匹配的库字节合计（与 count_pending 同过滤条件，取反）。
pub fn existing_lib_bytes(libraries: &[Library], lib_dir: &Path) -> u64 {
    libraries
        .iter()
        .filter_map(|lib| {
            if !should_download(lib) {
                return None;
            }
            let artifact = lib.downloads.as_ref()?.artifact.as_ref()?;
            let dest = lib_dir.join(&artifact.path);
            if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
                Some(artifact.size)
            } else {
                None
            }
        })
        .sum()
}

/// 需下载库的字节合计（total_lib_bytes − existing_lib_bytes）。
/// fabric（LoaderLibs 分母预填）与 runner（阶段 C 字节统计）共用。
pub fn pending_lib_bytes(libraries: &[Library], lib_dir: &Path) -> u64 {
    total_lib_bytes(libraries).saturating_sub(existing_lib_bytes(libraries, lib_dir))
}

/// 批量下载支持库（多源回退 + sha1 校验）。
/// `reuse`：可选本地运行时复用根——dest 缺失且复用根有 size+sha1 匹配的同相对路径文件时，
/// 本地复制（计入进度）而非网络下载；未命中照常走网络（迁移用户省数百 MB）。
pub async fn download_libraries(
    libraries: &[Library],
    libraries_dir: &Path,
    concurrency: usize,
    sources: &[&DownloadSource],
    reuse: Option<LocalRuntimeSource>,
    pc: &ProgressCounters,
    cancel: Option<&Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), Vec<String>> {
    let mut items: Vec<BatchItem> = Vec::new();
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
        // 本地运行时复用：dest 缺失时先问复用根（size + sha1 双校验，绝不盲复制）。
        // 命中 → 复制（与网络下载同一进度/计数路径）；未命中 → 网络下载。
        let local = reuse.as_ref().and_then(|r| {
            classify_local_artifact(
                r,
                &format!("libraries/{}", artifact.path),
                Some(artifact.size),
                Some(&artifact.sha1),
            )
        });
        // 每个源各构造一个 URL，下载时主源过慢/失败自动降级；复用命中则无需网络候选
        let urls: Vec<String> = if local.is_some() {
            Vec::new()
        } else {
            sources
                .iter()
                .map(|s| s.mirror_url(&artifact.url))
                .collect()
        };
        let label = dest.display().to_string();
        items.push(BatchItem {
            urls,
            dest,
            sha1: artifact.sha1.clone(),
            size: 0,
            local,
            label,
        });
    }

    if !internal_missing.is_empty() {
        return Err(internal_missing);
    }

    // 本步骤的进度分母（待下载条目数）：任务层心跳据此算该步骤百分比
    pc.step_files_total.store(items.len() as u32, Ordering::Relaxed);
    let errors = batch::run(items, concurrency, pc, cancel, None).await;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
