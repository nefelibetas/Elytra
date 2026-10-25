//! HTTP 下载公开入口：均为 `FetchOptions`（见 `core`）的薄包装。

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
    /// 下载成功但内容与期望 sha1 不匹配（镜像缓存陈旧/回源错误）；逐源循环内返回即此源不可信，换下一源重试。
    Checksum,
    /// 任务被取消（每块检查 cancel flag，立即停止并清理残留）。
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
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
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

/// 依次尝试多个 URL（主源优先）：连接超时（8s）或失败即换下一源；单 URL 走 `fetch_with`（3 次重试 + 长超时）。
pub async fn fetch_fallback(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
) -> Result<DownloadResult, FetchError> {
    fetch_fallback_c(urls, dest, progress, None).await
}

pub async fn fetch_fallback_c(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
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

/// 多源下载并逐次校验 sha1：不匹配视同该源失败、换源/换轮重试，全部源均不匹配才报 `FetchError::Checksum`；`expected_sha1` 空串跳过校验。
pub async fn fetch_verified(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
    expected_sha1: &str,
) -> Result<DownloadResult, FetchError> {
    fetch_verified_c(urls, dest, progress, expected_sha1, None).await
}

pub async fn fetch_verified_c(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
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

/// 带重试（网络错误/5xx 最多 3 次）与 HTTP 状态检查；`expected_sha1` 非空时逐次校验，失败清理残留文件。
pub async fn fetch_with(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
    expected_sha1: Option<&str>,
) -> Result<DownloadResult, FetchError> {
    fetch_with_c(client, url, dest, progress, expected_sha1, None).await
}

pub async fn fetch_with_c(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
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

/// 单文件多源下载：每源独立 client（连接 2s + 总超时 60s），失败/过慢/空响应即换源；用于无 sha1 小文件（如 Fabric API）防假死。
pub async fn fetch_bounded(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
) -> Result<DownloadResult, FetchError> {
    fetch_bounded_c(urls, dest, progress, None).await
}

pub async fn fetch_bounded_c(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64) + Send + Sync>,
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
    use crate::downloader::hash::{file_sha1_matches, sha1_hex, SHA1_EMPTY};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    /// 清单声明的就是零字节对象（jre-legacy 的 `lib/security/trusted.libraries`）：空响应合法。
    #[tokio::test]
    async fn fetch_verified_writes_declared_empty_object() {
        let url = serve(Vec::new()).await;
        let dest = temp_dest("fetch_verified_empty");
        let res = fetch_verified(&[url], &dest, None::<fn(u64, u64)>, SHA1_EMPTY).await;
        assert!(res.is_ok(), "声明为零字节对象时应成功: {res:?}");
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), 0, "应为 0 字节");
        let _ = std::fs::remove_file(&dest);
    }

    /// 未声明零字节时，空响应仍是失败（黑洞防护不放松）。
    #[tokio::test]
    async fn empty_response_without_declaration_still_fails() {
        let url = serve(Vec::new()).await;
        let dest = temp_dest("fetch_verified_empty_undeclared");
        let res = fetch_verified(&[url], &dest, None::<fn(u64, u64)>, "").await;
        assert!(res.is_err(), "未声明的空响应应失败: {res:?}");
        assert!(!dest.exists(), "失败时不留空文件");
    }

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

    #[tokio::test]
    async fn fetch_verified_skips_verification_when_sha1_empty() {
        let srv = serve(b"whatever-bytes".to_vec()).await;
        let urls = vec![srv];
        let dest = temp_dest("fetch_verified_emptysha1");

        let res = fetch_verified(&urls, &dest, None::<fn(u64, u64)>, "").await;

        assert!(res.is_ok(), "空 sha1 应跳过校验: {:?}", res);
        let _ = std::fs::remove_file(&dest);
    }

    #[tokio::test]
    async fn fetch_from_sources_reports_every_source_error() {
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
