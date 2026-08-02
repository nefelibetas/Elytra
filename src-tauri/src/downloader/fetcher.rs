use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use reqwest::Client;

fn build_client() -> Client {
    Client::builder()
        .user_agent("ElytraLauncher/1.0")
        .timeout(std::time::Duration::from_secs(300))
        .pool_max_idle_per_host(16)
        .build()
        .expect("reqwest::Client")
}

pub fn http() -> &'static Client {
    static C: OnceLock<Client> = OnceLock::new();
    C.get_or_init(build_client)
}

/// Client dedicated to library downloads (separate connection pool).
pub fn http_libs() -> &'static Client {
    static C: OnceLock<Client> = OnceLock::new();
    C.get_or_init(build_client)
}

/// Client dedicated to asset downloads (separate connection pool).
pub fn http_assets() -> &'static Client {
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

/// Like `fetch` but uses a specific client (for dedicated connection pools per category).
/// 带重试（网络错误/5xx 重试最多 3 次）与 HTTP 状态检查；失败时清理残留文件。
pub async fn fetch_with(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match fetch_once(client, url, dest, &progress).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                // 失败清理残留（下一次尝试或后续下载会重新创建）
                let _ = std::fs::remove_file(dest);
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << attempt))).await;
                }
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
) -> Result<DownloadResult, FetchError> {
    let response = client.get(url).send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;

    // HTTP 状态检查：4xx/5xx 视为失败（此前 404 会当作成功写入错误页面）
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
    }

    let elapsed = start.elapsed().as_secs_f64();
    let speed = if elapsed > 0.0 {
        (downloaded as f64 / elapsed) as u64
    } else {
        0
    };

    Ok(DownloadResult {
        bytes: downloaded,
        speed,
    })
}
