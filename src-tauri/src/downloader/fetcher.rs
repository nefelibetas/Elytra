use std::path::Path;
use std::time::Instant;

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
    let response = reqwest::get(url)
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;

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
        if let Some(ref cb) = progress {
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
