//! HTTP 下载公开入口：全部函数均为 [`FetchOptions`]（见 `core`）的薄包装，对外签名保持稳定。

use reqwest::Client;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::core::{download_core, FetchKind, FetchOptions};

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

#[cfg(test)]
mod tests {
    use super::super::core::test_support::{serve, temp_dest};
    use super::{fetch, fetch_verified, fetch_verified_c, FetchError};
    use crate::downloader::hash::{file_sha1_matches, sha1_hex};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// 取消 flag 预置时下载应立即返回 Cancelled 且不留文件
    /// （此前下载循环不检查取消，任务取消要等 read_timeout/换源才生效）。
    #[tokio::test]
    async fn fetch_cancelled_returns_immediately() {
        let body = vec![b'x'; 256 * 1024];
        let url = serve(body).await;
        let dest = temp_dest("fetch_cancelled");
        let cancel = Arc::new(AtomicBool::new(true)); // 预置已取消

        let t0 = std::time::Instant::now();
        let res = fetch_verified_c(&[url], &dest, None::<fn(u64, u64)>, "", Some(&cancel)).await;
        let elapsed = t0.elapsed();

        assert!(
            matches!(res, Err(FetchError::Cancelled)),
            "取消应返回 Cancelled，实际: {:?}",
            res
        );
        assert!(
            elapsed.as_secs() < 5,
            "取消应立即返回（{}s）",
            elapsed.as_secs_f64()
        );
        assert!(!dest.exists(), "取消后不应残留文件");
        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn fetch_verified_falls_back_to_second_source_on_checksum_mismatch() {
        let good = b"correct-content".to_vec();
        let wrong = serve(b"stale-mirror-content".to_vec()).await;
        let right = serve(good.clone()).await;
        // 防极端端口碰撞（内核 :0 分配理论极小概率相同）：相同则重建好源
        let right = if right == wrong {
            serve(good.clone()).await
        } else {
            right
        };
        let urls = vec![wrong, right];
        let dest = temp_dest("fetch_verified_recover");

        let res = fetch_verified(&urls, &dest, None::<fn(u64, u64)>, &sha1_hex(&good)).await;

        assert!(res.is_ok(), "应回退到第二源成功，实际: {:?}", res);
        assert!(
            file_sha1_matches(&dest, &sha1_hex(&good)),
            "落地文件内容应为正确字节"
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// 全部源均返回坏内容：报 Checksum 且不留坏文件（不误报成功）。
    #[tokio::test]
    async fn fetch_verified_fails_when_all_sources_return_wrong_content() {
        let bad1 = serve(b"bad-bytes-1".to_vec()).await;
        let bad2 = serve(b"bad-bytes-2".to_vec()).await;
        let urls = vec![bad1, bad2];
        let dest = temp_dest("fetch_verified_allbad");

        let res = fetch_verified(
            &urls,
            &dest,
            None::<fn(u64, u64)>,
            &sha1_hex(b"expected-content"),
        )
        .await;

        assert!(
            matches!(res, Err(FetchError::Checksum)),
            "应报 Checksum 错误，实际: {:?}",
            res
        );
        assert!(!dest.exists(), "校验失败后坏文件应被删除");
    }

    /// 期望 sha1 为空串时跳过校验（file_sha1_matches 语义），坏内容也视为成功。
    #[tokio::test]
    async fn fetch_verified_skips_verification_when_sha1_empty() {
        let srv = serve(b"whatever-bytes".to_vec()).await;
        let urls = vec![srv];
        let dest = temp_dest("fetch_verified_emptysha1");

        let res = fetch_verified(&urls, &dest, None::<fn(u64, u64)>, "").await;

        assert!(res.is_ok(), "空 sha1 应跳过校验: {:?}", res);
        let _ = std::fs::remove_file(&dest);
    }

    /// 混合失败（一源网络错误 + 一源内容不匹配）应报 Network 且信息包含每个源各自原因，
    /// 不被最后一个源的错误覆盖。
    #[tokio::test]
    async fn fetch_from_sources_reports_every_source_error() {
        // 未监听端口 → 连接失败（网络错误）；坏内容源 → Checksum
        let dead = "http://127.0.0.1:1/file.jar".to_string();
        let bad = serve(b"stale-content".to_vec()).await;
        let urls = vec![dead, bad];
        let dest = temp_dest("fetch_from_sources_mixed");

        let res = fetch_verified(
            &urls,
            &dest,
            None::<fn(u64, u64)>,
            &sha1_hex(b"expected-content"),
        )
        .await;

        let msg = match res {
            Err(FetchError::Network(m)) => m,
            other => panic!("应报 Network 错误，实际: {:?}", other),
        };
        assert!(
            msg.contains("127.0.0.1"),
            "信息应包含第一个源的失败原因，实际: {msg}"
        );
        assert!(
            msg.contains("sha1 校验失败") || msg.contains("Checksum") || msg.contains("校验"),
            "信息应包含内容校验失败的原因，实际: {msg}"
        );
        assert!(!dest.exists(), "失败后不应留文件");
    }

    /// `fetch`（单 URL 直连，无镜像/无 sha1）冒烟：本地 mock 服务正常取回内容。
    #[tokio::test]
    async fn fetch_single_url_writes_body() {
        let body = b"single-url-body".to_vec();
        let url = serve(body.clone()).await;
        let dest = temp_dest("fetch_single_url");

        let res = fetch(&url, &dest, None::<fn(u64, u64)>).await;
        assert!(res.is_ok(), "单 URL 下载应成功: {:?}", res);
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        let _ = std::fs::remove_file(&dest);
    }
}
