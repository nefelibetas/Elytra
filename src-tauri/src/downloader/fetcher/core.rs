//! 多源下载核心：9 个公开入口收敛于 [`download_core`]，按形态分流到
//! 单 URL 重试路径（[`fetch_with_retries`]）或多源回退路径（[`fetch_from_sources`]）。

use reqwest::Client;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use super::redirect::{http, mirror_safe_redirect};
use super::routes::{
    record_route_failure, record_route_success, resolve_download_routes, source_mode,
};
use super::{DownloadResult, FetchError};
use crate::downloader::proxy::apply_system_proxy;

/// 镜像源并发信号量（全进程共享）：限制同一时刻对镜像源（BMCLAPI/MCIM）的请求数。
/// 官方源实测 128 并发稳定无限流；镜像源 30 并发即触发 429，故镜像请求独立于
/// 任务并发（官方用用户配置并发，镜像恒为 MIRROR_CONCURRENCY）。
static MIRROR_SEM: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(crate::constants::MIRROR_CONCURRENCY)));

/// 该 URL 是否走限流（镜像源 + 高并发不稳定的官方 maven，统一限 MIRROR_CONCURRENCY）。
/// 镜像源域名变化时在此追加；maven.neoforged.net / maven.quiltmc.org / maven.minecraftforge.net
/// 实测高并发下连接失败（服务器限流），无镜像兜底的源（quilt）尤其依赖此保护。
pub(super) fn is_mirror_url(url: &str) -> bool {
    url.contains("bmclapi")
        || url.contains("mcimirror")
        || url.contains("maven.neoforged.net")
        || url.contains("maven.quiltmc.org")
        || url.contains("maven.minecraftforge.net")
}

/// 入口形态（分流到两条既有路径）：
/// - `Retry`：单 URL 重试路径——用 `FetchOptions::client`（缺省全局 `http()`），
///   网络错误/5xx 重试最多 3 次（500ms×2ⁿ 退避）；`fetch` / `fetch_with` 系。
/// - `MultiSource`：多源回退路径——每源独立短连接 client + 健康度路由
///   （`fetch_fallback` / `fetch_verified` / `fetch_bounded` 系）；
///   `total_timeout` 为整请求限时（None 不限时）；`single_url_retry` 表示列表仅一个
///   URL 时快捷改走 Retry 路径（fallback/verified 既有行为：保留 3 次重试与不限时
///   传输；bounded 关闭，维持"小文件防 body 假死"的限时语义）。
#[derive(Debug, Clone, Copy)]
pub(super) enum FetchKind {
    Retry,
    MultiSource {
        total_timeout: Option<Duration>,
        single_url_retry: bool,
    },
}

/// 共享下载核心参数：对外公开签名一个不变，9 个入口全部转发到此结构。
pub(super) struct FetchOptions<'a> {
    /// 候选 URL 列表（Retry 取首个；MultiSource 全量参与去重与镜像路由）。
    pub(super) urls: &'a [String],
    /// 落地目标文件。
    pub(super) dest: &'a Path,
    /// 非空时逐次/逐源校验内容 sha1，不匹配视同该次失败。
    pub(super) expected_sha1: Option<&'a str>,
    /// Retry 路径专用 client；None 用全局 `http()`。MultiSource 每源自建，忽略此字段。
    pub(super) client: Option<&'a Client>,
    /// 取消令牌：非空时每次读块检查，任务取消立即停止。
    pub(super) cancel: Option<&'a AtomicBool>,
    /// 入口形态。
    pub(super) kind: FetchKind,
}

/// 共享核心：按形态分流到单 URL 重试路径或多源回退路径。
pub(super) async fn download_core<F: Fn(u64, u64)>(
    FetchOptions {
        urls,
        dest,
        expected_sha1,
        client,
        cancel,
        kind,
    }: FetchOptions<'_>,
    progress: Option<F>,
) -> Result<DownloadResult, FetchError> {
    match kind {
        FetchKind::Retry => {
            // 局部持 Arc（http() 返回 Arc，客户端重建时旧引用计数释放）：
            // 借用内部 Client 传给重试路径，Arc 活过整个 await。
            let global = http();
            let c = client.unwrap_or(global.as_ref());
            let url = urls.first().map(String::as_str).unwrap_or("");
            fetch_with_retries(c, url, dest, &progress, expected_sha1, cancel).await
        }
        FetchKind::MultiSource {
            total_timeout,
            single_url_retry,
        } => {
            if single_url_retry && urls.len() == 1 {
                let url = urls.first().map(String::as_str).unwrap_or("");
                let global = http();
                return fetch_with_retries(global.as_ref(), url, dest, &progress, expected_sha1, cancel)
                    .await;
            }
            fetch_from_sources(urls, dest, &progress, total_timeout, expected_sha1, cancel).await
        }
    }
}

/// 单 URL 重试路径共享实现：网络错误/5xx 重试最多 3 次（500ms×2ⁿ 退避），
/// 取消立即传播（不重试、不换源）；失败清理残留文件（下次尝试或后续下载重新创建）。
async fn fetch_with_retries<F: Fn(u64, u64)>(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: &Option<F>,
    expected_sha1: Option<&str>,
    cancel: Option<&AtomicBool>,
) -> Result<DownloadResult, FetchError> {
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match fetch_once(client, url, dest, progress, expected_sha1, cancel).await {
            Ok(out) => return Ok(out.dl),
            Err(e) => {
                // 取消立即传播（不重试、不换源）
                if matches!(e, FetchError::Cancelled) {
                    return Err(e);
                }
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

/// 单次请求结果（含 TTFB 供健康度记录）。
struct FetchOnceOutput {
    dl: DownloadResult,
    /// 首字节耗时（毫秒，发送请求到响应头到达）。
    ttfb_ms: f64,
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
    cancel: Option<&AtomicBool>,
) -> Result<DownloadResult, FetchError> {
    const MAX_ATTEMPTS: u32 = 3;
    // 自适应路由：去重 + 自动镜像候选（官方保底）+ mirror_first_loader/健康度排序。
    // 调用方传入的源顺序在非 Auto 模式下保持不变（向后兼容）。
    let routes = resolve_download_routes(urls, source_mode());
    // 收集每源/每轮全部错误（展示时去重），避免最后一个源覆盖前面的失败原因
    let mut errors: Vec<FetchError> = Vec::new();
    for attempt in 0..MAX_ATTEMPTS {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(FetchError::Cancelled);
        }
        for route in &routes {
            let mut builder = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                // 读黑洞防挂死：连接建立后 30s 无数据（镜像假死/TCP 黑洞）即掐断换源；
                // 持续有数据（慢速但稳定）的源不受影响。此前大文件路径无任何超时，
                // 挂起会永久卡死下载且取消无效。
                .read_timeout(Duration::from_secs(30))
                // 302 到黑洞节点（749333.xyz）直接拒绝，不等超时
                .redirect(mirror_safe_redirect());
            if let Some(t) = total_timeout {
                builder = builder.timeout(t);
            }
            // 系统代理（网络模式 auto/system；direct 直连）——本机开启 Clash 等时官方经代理可达
            builder = apply_system_proxy(builder);
            let client = match builder.build() {
                Ok(c) => c,
                Err(e) => {
                    errors.push(FetchError::Network(format!("{}: {e}", route.url)));
                    continue;
                }
            };
            match fetch_once(&client, &route.url, dest, progress, expected_sha1, cancel).await {
                Ok(out) => {
                    record_route_success(&route.url, out.ttfb_ms);
                    return Ok(out.dl);
                }
                Err(e) => {
                    // 取消立即传播（不换源、不重试）
                    if matches!(e, FetchError::Cancelled) {
                        return Err(e);
                    }
                    // 失败清理残留（下一源/下一轮会重新创建）
                    let _ = std::fs::remove_file(dest);
                    record_route_failure(&route.url);
                    eprintln!("[fetch] 源失败 轮{attempt}: {} → {e}", route.url);
                    // 非校验类错误附带源 URL，便于用户定位哪个源失败
                    errors.push(match e {
                        FetchError::Checksum => FetchError::Checksum,
                        other => FetchError::Network(format!("{}: {other}", route.url)),
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
    let mut msg = format!(
        "所有下载源均失败（已重试 {MAX_ATTEMPTS} 轮），请检查网络或稍后重试:\n{}",
        seen.join("\n")
    );
    // 404 = 文件在该源不存在（常见于镜像缓存尚未同步新版本/新构件），引导换版本
    if seen.iter().any(|s| s.contains("HTTP 404")) {
        msg.push_str(
            "\n\n提示：部分源返回 404（该文件在镜像源可能尚未同步，或版本不存在），可稍后重试或选择其他版本",
        );
    }
    Err(FetchError::Network(msg))
}

async fn fetch_once<F: Fn(u64, u64)>(
    client: &Client,
    url: &str,
    dest: &Path,
    progress: &Option<F>,
    expected_sha1: Option<&str>,
    cancel: Option<&AtomicBool>,
) -> Result<FetchOnceOutput, FetchError> {
    // 镜像源并发限制：镜像请求独立于任务信号量（官方源不限流、镜像源 30 并发即 429）。
    // permit 持有到本次传输结束，队列自然摊薄对镜像源的瞬时请求速率。
    let _mirror_permit = if is_mirror_url(url) {
        Some(
            MIRROR_SEM
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| FetchError::Network(format!("请求失败\n{e}")))?,
        )
    } else {
        None
    };
    let request_start = Instant::now();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(format!("请求失败\n{e}")))?;
    let ttfb_ms = request_start.elapsed().as_secs_f64() * 1000.0;

    // HTTP 状态检查：4xx/5xx 视为失败
    let status = response.status();
    eprintln!("[fetch] {url} → HTTP {status}");
    if !status.is_success() {
        return Err(FetchError::Network(format!("请求失败\nHTTP {status}")));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let start = Instant::now();

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| FetchError::Io(format!("创建目录失败\n{e}")))?;
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| FetchError::Io(format!("创建文件失败\n{e}")))?;

    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        // 取消立即生效：当前块读完即停（不等 read_timeout/重试），清理残留由调用方负责
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            drop(stream);
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err(FetchError::Cancelled);
        }
        let chunk = chunk.map_err(|e| FetchError::Network(format!("请求失败\n{e}")))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| FetchError::Io(format!("写入文件失败\n{e}")))?;
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

    // 空响应防护：0 字节文件视为失败（Minecraft 下载物无合法空文件）。
    // 此前服务端响应 headers 但 body 挂起/为空时会留下 0B 残留并误报成功。
    if downloaded == 0 {
        drop(file);
        let _ = std::fs::remove_file(dest);
        return Err(FetchError::Network("响应内容为空".into()));
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

    Ok(FetchOnceOutput {
        dl: DownloadResult {
            bytes: downloaded,
            speed,
        },
        ttfb_ms,
    })
}
