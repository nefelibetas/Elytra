use std::path::Path;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::{Duration, Instant};

use reqwest::Client;
use tokio::sync::Semaphore;

/// 当前全局下载限速（bytes/s，0 = 无限制）。每次下载开始读取一次。
pub fn max_speed_bps() -> u64 {
    crate::persist::read::<crate::config::downloader::model::DownloaderConfig>(SECT_DOWNLOADER)
        .map(|c| c.max_speed)
        .unwrap_or(0)
}

use crate::constants::{SECT_DOWNLOADER, UA_DOWNLOAD};

/// 镜像源并发信号量（全进程共享）：限制同一时刻对镜像源（BMCLAPI/MCIM）的请求数。
/// 官方源实测 128 并发稳定无限流；镜像源 30 并发即触发 429，故镜像请求独立于
/// 任务并发（官方用用户配置并发，镜像恒为 MIRROR_CONCURRENCY）。
static MIRROR_SEM: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(crate::constants::MIRROR_CONCURRENCY)));

/// 该 URL 是否走镜像源（需受镜像并发限制）。镜像源域名变化时在此追加。
fn is_mirror_url(url: &str) -> bool {
    url.contains("bmclapi") || url.contains("mcimirror")
}

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
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(e) => write!(f, "network: {}", e),
            FetchError::Io(e) => write!(f, "io: {}", e),
            FetchError::InvalidUrl => write!(f, "invalid url"),
            FetchError::Checksum => write!(
                f,
                "sha1 校验失败（所有下载源返回的内容均与期望不一致，可能为镜像缓存问题）"
            ),
        }
    }
}

pub async fn fetch(
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
) -> Result<DownloadResult, FetchError> {
    fetch_with(http(), url, dest, progress, None).await
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
        return fetch_with(http(), url, dest, progress, None).await;
    }
    fetch_from_sources(urls, dest, &progress, None, None).await
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
    if urls.len() <= 1 {
        let url = urls.first().map(String::as_str).unwrap_or("");
        return fetch_with(http(), url, dest, progress, Some(expected_sha1)).await;
    }
    fetch_from_sources(urls, dest, &progress, None, Some(expected_sha1)).await
}

/// Like `fetch` but uses a specific client (for dedicated connection pools per category).
/// 带重试（网络错误/5xx 重试最多 3 次）与 HTTP 状态检查；失败时清理残留文件。
/// 下载时按全局配置限速（max_speed，0 = 不限）。
/// `expected_sha1` 非空时逐次校验内容，不匹配按失败重试。
pub async fn fetch_with(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: Option<impl Fn(u64, u64)>,
    expected_sha1: Option<&str>,
) -> Result<DownloadResult, FetchError> {
    let max_speed = max_speed_bps();
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match fetch_once(client, url, dest, &progress, max_speed, expected_sha1).await {
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
    fetch_from_sources(urls, dest, &progress, Some(Duration::from_secs(60)), None).await
}

/// 多源循环：每源独立 client（连接 2s：连接缓慢即判"过慢"即换源），失败清理残留。
/// 全源一轮失败后整体重试（最多 3 轮，退避 500ms×2^n）——抗单次网络抖动/瞬时故障，
/// 避免"每个源各失败一次就放弃"导致任务失败。
/// `total_timeout` 为 None 时传输不限时（大文件，慢速但稳定的源不被砍）；
/// 为 Some 时整请求限时（小文件，防 body 挂起假死）。
/// `expected_sha1` 非空时逐源校验：内容不匹配视同该源失败，换下一源/下一轮。
async fn fetch_from_sources<F: Fn(u64, u64)>(
    urls: &[String],
    dest: &Path,
    progress: &Option<F>,
    total_timeout: Option<Duration>,
    expected_sha1: Option<&str>,
) -> Result<DownloadResult, FetchError> {
    let max_speed = max_speed_bps();
    const MAX_ATTEMPTS: u32 = 3;
    // 收集每源/每轮全部错误（展示时去重），避免最后一个源覆盖前面的失败原因
    let mut errors: Vec<FetchError> = Vec::new();
    for attempt in 0..MAX_ATTEMPTS {
        for url in urls {
            let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(2));
            if let Some(t) = total_timeout {
                builder = builder.timeout(t);
            }
            let client = match builder.build() {
                Ok(c) => c,
                Err(e) => {
                    errors.push(FetchError::Network(format!("{url}: {e}")));
                    continue;
                }
            };
            match fetch_once(&client, url, dest, progress, max_speed, expected_sha1).await {
                Ok(r) => return Ok(r),
                Err(e) => {
                    // 失败清理残留（下一源/下一轮会重新创建）
                    let _ = std::fs::remove_file(dest);
                    // 非校验类错误附带源 URL，便于用户定位哪个源失败
                    errors.push(match e {
                        FetchError::Checksum => FetchError::Checksum,
                        other => FetchError::Network(format!("{url}: {other}")),
                    });
                }
            }
        }
        if attempt + 1 < MAX_ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << attempt))).await;
        }
    }
    // 全部源都是内容校验失败 → Checksum（镜像内容问题，重试无意义）
    if !errors.is_empty() && errors.iter().all(|e| matches!(e, FetchError::Checksum)) {
        return Err(FetchError::Checksum);
    }
    // 展示去重（同一源多轮报同一错误只保留首次），并附带每个源各自的失败 URL
    let mut seen: Vec<String> = Vec::new();
    for e in &errors {
        let s = e.to_string();
        if !seen.contains(&s) {
            seen.push(s);
        }
    }
    if seen.is_empty() {
        return Err(FetchError::InvalidUrl);
    }
    Err(FetchError::Network(format!(
        "所有下载源均失败（已重试 {MAX_ATTEMPTS} 轮），请检查网络或稍后重试:\n{}",
        seen.join("\n")
    )))
}

async fn fetch_once<F: Fn(u64, u64)>(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: &Option<F>,
    max_speed: u64,
    expected_sha1: Option<&str>,
) -> Result<DownloadResult, FetchError> {
    // 镜像源并发限制：镜像请求独立于任务信号量（官方源不限流、镜像源 30 并发即 429）。
    // permit 持有到本次传输结束，队列自然摊薄对镜像源的瞬时请求速率。
    let _mirror_permit = if is_mirror_url(url) {
        Some(
            MIRROR_SEM
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| FetchError::Network(e.to_string()))?,
        )
    } else {
        None
    };
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

    // 内容校验（逐源循环内）：镜像可能返回陈旧/错误内容，不匹配视同此源失败。
    // 调用方在循环内捕获 Checksum 后换下一源重试，全部源失败才最终报错。
    if let Some(sha1) = expected_sha1 {
        if !sha1.is_empty() {
            drop(file);
            if !crate::downloader::hash::file_sha1_matches(dest, sha1) {
                let _ = std::fs::remove_file(dest);
                return Err(FetchError::Checksum);
            }
        }
    }

    Ok(DownloadResult {
        bytes: downloaded,
        speed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::hash::sha1_hex;

    /// 本地 HTTP 服务器：每次请求返回固定 body（用于确定性校验失败/成功场景）。
    async fn serve(body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let body = body.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&body).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        format!("http://{addr}/file.jar")
    }

    fn temp_dest(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("{}_{}.jar", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    /// 主源返回坏内容时应换备用源重试并成功（镜像缓存陈旧场景的核心行为）。
    #[tokio::test]
    async fn fetch_verified_falls_back_to_second_source_on_checksum_mismatch() {
        let good = b"correct-content".to_vec();
        let wrong = serve(b"stale-mirror-content".to_vec()).await;
        let right = serve(good.clone()).await;
        let urls = vec![wrong, right];
        let dest = temp_dest("fetch_verified_recover");

        let res = fetch_verified(&urls, &dest, None::<fn(u64, u64)>, &sha1_hex(&good)).await;

        assert!(res.is_ok(), "应回退到第二源成功，实际: {:?}", res);
        assert!(
            crate::downloader::hash::file_sha1_matches(&dest, &sha1_hex(&good)),
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

    /// 镜像源判定：仅 BMCLAPI/MCIM 域名受限，官方源不受限。
    #[test]
    fn is_mirror_url_detects_mirror_domains() {
        assert!(is_mirror_url("https://bmclapi2.bangbang93.com/libraries/x.jar"));
        assert!(is_mirror_url("https://mod.mcimirror.top/data/x.jar"));
        assert!(!is_mirror_url("https://libraries.minecraft.net/x.jar"));
        assert!(!is_mirror_url("https://piston-data.mojang.com/x.jar"));
        assert!(!is_mirror_url("https://resources.download.minecraft.net/x"));
    }

    /// 镜像源（URL 含 bmclapi）请求并发必须被限制在 MIRROR_CONCURRENCY 以内：
    /// 20 个并发任务同时下载镜像文件，服务器观测到的同时在处理的连接数 ≤ 8。
    #[tokio::test]
    async fn mirror_requests_capped_at_mirror_concurrency() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (a2, m2) = (Arc::clone(&active), Arc::clone(&max_active));
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let (a3, m3) = (Arc::clone(&a2), Arc::clone(&m2));
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let cur = a3.fetch_add(1, Ordering::SeqCst) + 1;
                    m3.fetch_max(cur, Ordering::SeqCst);
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    // 模拟下载耗时，给并发统计留出窗口
                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                    let body = b"mirror-content";
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(body).await;
                    let _ = sock.shutdown().await;
                    a3.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        // URL 以 userinfo 携带 bmclapi 标记（is_mirror_url 判定为镜像），
        // host 仍是 127.0.0.1 保证可解析且直连本地服务器
        let url = format!("http://bmclapi@127.0.0.1:{}/file.jar", addr.port());
        let mut handles = Vec::new();
        for i in 0..20 {
            let u = url.clone();
            let d = temp_dest(&format!("mirror_cap_{i}"));
            handles.push(tokio::spawn(async move {
                let _ = fetch_verified(std::slice::from_ref(&u), &d, None::<fn(u64, u64)>, "").await;
                let _ = std::fs::remove_file(&d);
            }));
        }
        for h in handles {
            let _ = h.await;
        }

        let max = max_active.load(Ordering::SeqCst);
        assert!(
            max <= crate::constants::MIRROR_CONCURRENCY,
            "镜像并发应 ≤ {}，实际峰值 {}",
            crate::constants::MIRROR_CONCURRENCY,
            max
        );
        assert!(max > 1, "应存在真实并发（>1），实际 {}", max);
    }
}
