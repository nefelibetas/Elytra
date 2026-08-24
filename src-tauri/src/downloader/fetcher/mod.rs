//! HTTP 下载公开入口：9 个函数均为 [`FetchOptions`]（见 `core`）的薄包装，
//! 对外签名保持稳定。实现拆分为：
//! - `routes`：镜像路由表引用、健康度排序与候选 URL 生成；
//! - `redirect`：重定向黑洞黑名单、安全跟随与全局 client；
//! - `core`：多源下载核心（FetchOptions/download_core/fetch_with_retries）。

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use reqwest::Client;

use self::core::{FetchKind, FetchOptions, download_core};
mod core;
mod redirect;
mod routes;
#[cfg(test)]
mod tests;

pub use redirect::http;

#[derive(Debug)]
pub struct DownloadResult {
    pub bytes: u64,
    pub speed: u64,
}

#[derive(Debug)]
pub enum FetchError {
    Network(String),
    Io(String),
    InvalidUrl,
    /// 下载成功但内容与期望 sha1 不匹配（镜像缓存陈旧/回源错误内容）。
    /// 校验发生在逐源循环内：返回该错误即"此源内容不可信"，换下一源重试。
    Checksum,
    /// 任务被取消（下载循环每块检查 cancel flag，立即停止并清理残留）。
    Cancelled,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // 存储的字符串自带「中文首行\n技术细节」结构（见各处构造点），直接原样输出
            FetchError::Network(e) => write!(f, "{e}"),
            FetchError::Io(e) => write!(f, "{e}"),
            FetchError::InvalidUrl => write!(f, "下载地址无效"),
            FetchError::Checksum => write!(
                f,
                "sha1 校验失败（所有下载源返回的内容均与期望不一致，可能为镜像缓存问题）"
            ),
            FetchError::Cancelled => write!(f, "任务已取消"),
        }
    }
}

pub async fn fetch(
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    let urls = [url.to_string()];
    download_core(
        FetchOptions {
            urls: &urls,
            dest,
            expected_sha1: None,
            client: None,
            cancel: None,
            kind: FetchKind::Retry,
        },
        progress,
    )
    .await
}

/// 依次尝试多个 URL（主源优先）：连接超时（8s）或失败即换下一个源（过慢降级）。
/// 单 URL 时走 `fetch_with`（保留 3 次重试 + 长超时）。
pub async fn fetch_fallback(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    fetch_fallback_c(urls, dest, progress, None).await
}

/// `fetch_fallback` 的可取消变体（供下载任务使用）。
pub async fn fetch_fallback_c(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    cancel: Option<&AtomicBool>,
) -> Result<DownloadResult, FetchError> {
    download_core(
        FetchOptions {
            urls,
            dest,
            expected_sha1: None,
            client: None,
            cancel,
            kind: FetchKind::MultiSource {
                total_timeout: None,
                single_url_retry: true,
            },
        },
        progress,
    )
    .await
}

/// 多源下载并在每次成功后校验 sha1：内容不匹配视同该源失败，立即换下一源/下一轮重试，
/// 全部源均返回不匹配内容才报 `FetchError::Checksum`。
/// 防镜像缓存陈旧（Mojang 原地重传同版本构件后镜像未同步）导致整批 sha1 失败——
/// 此前校验在重试循环之外，一个源返回坏内容即整批任务失败，只能手动修复。
/// `expected_sha1` 为空串时跳过校验（与 `file_sha1_matches` 语义一致）。
pub async fn fetch_verified(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    expected_sha1: &str,
) -> Result<DownloadResult, FetchError> {
    fetch_verified_c(urls, dest, progress, expected_sha1, None).await
}

/// `fetch_verified` 的可取消变体：`cancel` 非空时下载循环每块检查，
/// 任务被取消立即停止（当前块读完即停，不等 read_timeout）。
/// 供下载任务（runner/repair）使用；无取消语义的调用方走 `fetch_verified`。
pub async fn fetch_verified_c(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    expected_sha1: &str,
    cancel: Option<&AtomicBool>,
) -> Result<DownloadResult, FetchError> {
    download_core(
        FetchOptions {
            urls,
            dest,
            expected_sha1: Some(expected_sha1),
            client: None,
            cancel,
            kind: FetchKind::MultiSource {
                total_timeout: None,
                single_url_retry: true,
            },
        },
        progress,
    )
    .await
}

/// Like `fetch` but uses a specific client (for dedicated connection pools per category).
/// 带重试（网络错误/5xx 重试最多 3 次）与 HTTP 状态检查；失败时清理残留文件。
/// `expected_sha1` 非空时逐次校验内容，不匹配按失败重试。
pub async fn fetch_with(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    expected_sha1: Option<&str>,
) -> Result<DownloadResult, FetchError> {
    fetch_with_c(client, url, dest, progress, expected_sha1, None).await
}

/// `fetch_with` 的可取消变体（供下载任务使用）。
pub async fn fetch_with_c(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    expected_sha1: Option<&str>,
    cancel: Option<&AtomicBool>,
) -> Result<DownloadResult, FetchError> {
    let urls = [url.to_string()];
    download_core(
        FetchOptions {
            urls: &urls,
            dest,
            expected_sha1,
            client: Some(client),
            cancel,
            kind: FetchKind::Retry,
        },
        progress,
    )
    .await
}

/// 单文件多源下载：每源独立 client（连接 2s + 总超时 60s），
/// 主源失败/过慢/空响应立即换下一源。用于 Fabric API 等无 sha1 校验的小文件：
/// 避免单 URL 长时间假死（官方 CDN 国内不可达时快速降级镜像）。
pub async fn fetch_bounded(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    fetch_bounded_c(urls, dest, progress, None).await
}

/// `fetch_bounded` 的可取消变体（供下载任务使用）。
pub async fn fetch_bounded_c(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    cancel: Option<&AtomicBool>,
) -> Result<DownloadResult, FetchError> {
    download_core(
        FetchOptions {
            urls,
            dest,
            expected_sha1: None,
            client: None,
            cancel,
            kind: FetchKind::MultiSource {
                total_timeout: Some(Duration::from_secs(60)),
                single_url_retry: false,
            },
        },
        progress,
    )
    .await
}
