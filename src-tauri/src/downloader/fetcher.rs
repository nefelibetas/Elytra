use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use reqwest::Client;

/// 当前全局下载限速（bytes/s，0 = 无限制）。每次下载开始读取一次。
pub fn max_speed_bps() -> u64 {
    crate::persist::read::<crate::config::downloader::model::DownloaderConfig>(SECT_DOWNLOADER)
        .map(|c| c.max_speed)
        .unwrap_or(0)
}

use crate::constants::{SECT_DOWNLOADER, UA_DOWNLOAD};

fn build_client() -> Client {
    Client::builder()
        .user_agent(UA_DOWNLOAD)
        .timeout(std::time::Duration::from_secs(300))
        .pool_max_idle_per_host(16)
        .build()
        .expect("reqwest::Client")
}

pub fn http() -> &'static Client {
    static C: OnceLock<Client> = OnceLock::new();
    C.get_or_init(build_client)
}

pub struct DownloadResult {
    pub bytes: u64,
    pub speed: u64,
}

pub enum FetchError {
    Network(String),
    Io(String),
    InvalidUrl,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(e) => write!(f, "network: {}", e),
            FetchError::Io(e) => write!(f, "io: {}", e),
            FetchError::InvalidUrl => write!(f, "invalid url"),
        }
    }
}

pub async fn fetch(
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    fetch_with(http(), url, dest, progress).await
}

/// 依次尝试多个 URL（主源优先）：连接超时（8s）或失败即换下一个源（过慢降级）。
/// 单 URL 时走 `fetch_with`（保留 3 次重试 + 长超时）。
pub async fn fetch_fallback(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    if urls.len() <= 1 {
        let url = urls.first().map(String::as_str).unwrap_or("");
        return fetch_with(http(), url, dest, progress).await;
    }
    fetch_from_sources(urls, dest, &progress, None).await
}

/// Like `fetch` but uses a specific client (for dedicated connection pools per category).
/// 带重试（网络错误/5xx 重试最多 3 次）与 HTTP 状态检查；失败时清理残留文件。
/// 下载时按全局配置限速（max_speed，0 = 不限）。
pub async fn fetch_with(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    let max_speed = max_speed_bps();
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match fetch_once(client, url, dest, &progress, max_speed).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                // 失败清理残留（下一次尝试或后续下载会重新创建）
                let _ = std::fs::remove_file(dest);
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << attempt)))
                        .await;
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
}

/// 单文件多源下载：每源独立 client（连接 2s + 总超时 60s），
/// 主源失败/过慢/空响应立即换下一源。用于 Fabric API 等无 sha1 校验的小文件：
/// 避免单 URL 长时间假死（官方 CDN 国内不可达时快速降级镜像）。
pub async fn fetch_bounded(
    urls: &[String],
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    fetch_from_sources(urls, dest, &progress, Some(Duration::from_secs(60))).await
}

/// 多源循环：每源独立 client（连接 2s：连接缓慢即判"过慢"即换源），失败清理残留。
/// `total_timeout` 为 None 时传输不限时（大文件，慢速但稳定的源不被砍）；
/// 为 Some 时整请求限时（小文件，防 body 挂起假死）。
async fn fetch_from_sources<F: Fn(u64, u64)>(
    urls: &[String],
    dest: &Path,
    progress: &Option<F>,
    total_timeout: Option<Duration>,
) -> Result<DownloadResult, FetchError> {
    let max_speed = max_speed_bps();
    let mut last_err = None;
    for url in urls {
        let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(2));
        if let Some(t) = total_timeout {
            builder = builder.timeout(t);
        }
        let client = match builder.build() {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(FetchError::Network(e.to_string()));
                continue;
            }
        };
        match fetch_once(&client, url, dest, progress, max_speed).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                // 失败清理残留（下一源会重新创建）
                let _ = std::fs::remove_file(dest);
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
}

async fn fetch_once<F: Fn(u64, u64)>(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: &Option<F>,
    max_speed: u64,
) -> Result<DownloadResult, FetchError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;

    // HTTP 状态检查：4xx/5xx 视为失败
    let status = response.status();
    if !status.is_success() {
        return Err(FetchError::Network(format!("HTTP {status}")));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let start = Instant::now();

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| FetchError::Io(e.to_string()))?;
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| FetchError::Io(e.to_string()))?;

    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| FetchError::Network(e.to_string()))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| FetchError::Io(e.to_string()))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if let Some(cb) = progress.as_ref() {
            cb(downloaded, total);
        }
        // 限速：按已下载字节数控制期望耗时，超出则等待
        if max_speed > 0 {
            let expect = downloaded as f64 / max_speed as f64;
            let actual = start.elapsed().as_secs_f64();
            if expect > actual {
                tokio::time::sleep(Duration::from_secs_f64(expect - actual)).await;
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
        (downloaded as f64 / elapsed) as u64
    } else {
        0
    };

    // 空响应防护：0 字节文件视为失败（Minecraft 下载物无合法空文件）。
    // 此前服务端响应 headers 但 body 挂起/为空时会留下 0B 残留并误报成功。
    if downloaded == 0 {
        drop(file);
        let _ = std::fs::remove_file(dest);
        return Err(FetchError::Network("empty response body".into()));
    }

    Ok(DownloadResult {
        bytes: downloaded,
        speed,
    })
}
