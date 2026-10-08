//! 多源下载核心：9 个公开入口收敛于 [`download_core`]，按形态分流到
//! 单 URL 重试路径（[`fetch_with_retries`]）或多源回退路径（[`fetch_from_sources`]）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use reqwest::Client;
use tokio::sync::Semaphore;

use super::redirect::{http, mirror_safe_redirect};
use super::routes::{
    record_route_failure, record_route_success, resolve_download_routes, route_health, source_mode,
    DownloadRoute,
};
use super::{DownloadResult, FetchError};
use crate::config::downloader::{infer_resource_class, DownloadSourceMode, ResourceClass};
use crate::constants;
use crate::downloader::hash::file_sha1_matches;
use crate::downloader::probe::{probe_domain, ProbeResult};
use crate::downloader::proxy::apply_system_proxy;

/// 镜像源并发信号量（全进程共享）：限制同一时刻对镜像源（BMCLAPI/MCIM）的请求数。
/// 官方源实测 128 并发稳定无限流；镜像源 30 并发即触发 429，故镜像请求独立于
/// 任务并发（官方用用户配置并发，镜像恒为 MIRROR_CONCURRENCY）。
/// 全局限速（bytes/s，0 = 不限）。由配置保存/任务启动时同步。
static SPEED_LIMIT: AtomicU64 = AtomicU64::new(0);

/// 设置全局限速（bytes/s，0 关闭）。
pub fn set_speed_limit(bytes: u64) {
    SPEED_LIMIT.store(bytes, Ordering::Relaxed);
}

static MIRROR_SEM: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(constants::MIRROR_CONCURRENCY)));

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
                return fetch_with_retries(
                    global.as_ref(),
                    url,
                    dest,
                    &progress,
                    expected_sha1,
                    cancel,
                )
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
                    tokio::time::sleep(Duration::from_millis(500 * (1 << attempt))).await;
                }
                last_err = Some(e);
            }
        }
    }
    // MAX_ATTEMPTS ≥ 1 保证循环至少执行一次并赋值；显式处理 None 以免 panic 路径
    match last_err {
        Some(e) => Err(e),
        None => Err(FetchError::Network(
            "下载重试未产生错误记录（内部状态异常）".into(),
        )),
    }
}

/// 冷启动预探测排序（async）：并发对每个候选做域级 HEAD（TTL 缓存，同域文件共享），
/// Loader 镜像先行 → 探测失败排后 → TTFB 升序。探测失败仍保留候选（下载期兜底）。
async fn sort_routes_by_probes(routes: &mut [DownloadRoute], class: ResourceClass) {
    let probes = futures_util::future::join_all(
        routes
            .iter()
            .map(|r| async move { (r.url.clone(), probe_domain(&r.url).await) }),
    )
    .await
    .into_iter()
    .collect::<HashMap<_, _>>();
    sort_routes_with_probes(routes, &probes, class);
}

/// 探测排序纯逻辑（便于单测）：Loader 镜像先行（官方 maven 限流语义保持）→
/// 探测失败排后 → TTFB 升序；无探测样本视同失败（排后，不剔除）。
pub(super) fn sort_routes_with_probes(
    routes: &mut [DownloadRoute],
    probes: &HashMap<String, ProbeResult>,
    class: ResourceClass,
) {
    routes.sort_by_key(|r| {
        let p = probes.get(&r.url).copied().unwrap_or(ProbeResult {
            ok: false,
            ttfb_ms: u64::MAX,
        });
        let loader_mirror_first = u8::from(!(class == ResourceClass::Loader && r.is_mirror));
        (loader_mirror_first, !p.ok, p.ttfb_ms)
    });
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
    let mut routes = resolve_download_routes(urls, source_mode());
    // 下载前域级预探测（智能模式）：候选全部冷启动（无健康度样本）时，对候选域
    // HEAD 测 TTFB 排序——首个文件不再白等黑洞超时（连接 2s + 读 30s → 探测 ≤3s）；
    // 热启动（已有成功样本）走健康度自适应，不重复探测。探测失败不剔除，仅排后
    // （下载期重试/换源兜底）。
    if routes.len() > 1
        && source_mode() == DownloadSourceMode::Auto
        && routes.iter().all(|r| route_health(&r.url).samples == 0)
    {
        let class = infer_resource_class(urls.first().map(String::as_str).unwrap_or(""));
        sort_routes_by_probes(&mut routes, class).await;
    }
    // 收集每源/每轮全部错误（展示时去重），避免最后一个源覆盖前面的失败原因
    let mut errors: Vec<FetchError> = Vec::new();
    for attempt in 0..MAX_ATTEMPTS {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(FetchError::Cancelled);
        }
        for route in &routes {
            let mut builder = Client::builder()
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
            tokio::time::sleep(Duration::from_millis(500 * (1 << attempt))).await;
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
        // 限速：按累计字节/限速推算目标耗时，落后则补睡到目标时刻
        // （每块检查；限速为 0 = 不限）。睡眠期间不响应 cancel——块间
        // 的取消检查在循环头部，限速时取消延迟 ≤ 一个块的时间。
        let limit = SPEED_LIMIT.load(Ordering::Relaxed);
        if limit > 0 {
            let target_secs = downloaded as f64 / limit as f64;
            let elapsed_secs = start.elapsed().as_secs_f64();
            if elapsed_secs < target_secs {
                tokio::time::sleep(Duration::from_secs_f64(target_secs - elapsed_secs)).await;
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
        return Err(FetchError::Network("响应内容为空".into()));
    }

    // 内容校验（逐源循环内）：镜像可能返回陈旧/错误内容，不匹配视同此源失败。
    // 调用方在循环内捕获 Checksum 后换下一源重试，全部源失败才最终报错。
    if let Some(sha1) = expected_sha1 {
        if !sha1.is_empty() {
            // 先 flush 再校验：tokio::fs::File 的 drop 是异步的，立即重新 open 读
            // 可能读到未落盘的上一次内容（多源轮换写同一 dest 时读到旧数据误判 Checksum）
            tokio::io::AsyncWriteExt::flush(&mut file)
                .await
                .map_err(|e| FetchError::Io(format!("写入文件失败\n{e}")))?;
            drop(file);
            if !file_sha1_matches(dest, sha1) {
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

/// 下载层测试共用的本地 HTTP 服务与临时文件工具。
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 就绪握手：accept 循环启动后通过 oneshot 通知调用方，杜绝 spawn 未调度时
    /// 请求先行导致的连接竞态（此前偶发 "error sending request" 即此根因）。
    pub(crate) async fn serve(body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = tx.send(());
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let body = body.clone();
                tokio::spawn(async move {
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
        let _ = rx.await; // 就绪：accept 循环已启动
        format!("http://{addr}/file.jar")
    }

    pub(crate) fn temp_dest(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("{}_{}.jar", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::temp_dest;
    use super::{is_mirror_url, sort_routes_with_probes};
    use crate::config::downloader::ResourceClass;
    use crate::constants;
    use crate::downloader::fetcher::routes::DownloadRoute;
    use crate::downloader::fetcher::fetch_verified;
    use crate::downloader::probe::ProbeResult;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// 镜像源判定：仅 BMCLAPI/MCIM 域名受限，官方源不受限。
    #[test]
    fn is_mirror_url_detects_mirror_domains() {
        assert!(is_mirror_url(
            "https://bmclapi2.bangbang93.com/libraries/x.jar"
        ));
        assert!(is_mirror_url("https://mod.mcimirror.top/data/x.jar"));
        assert!(!is_mirror_url("https://libraries.minecraft.net/x.jar"));
        assert!(!is_mirror_url("https://piston-data.mojang.com/x.jar"));
        assert!(!is_mirror_url("https://resources.download.minecraft.net/x"));
        // 高并发不稳的官方 maven 同样限流
        assert!(is_mirror_url(
            "https://maven.quiltmc.org/repository/release/org/quiltmc/x.jar"
        ));
        assert!(is_mirror_url(
            "https://maven.neoforged.net/releases/net/neoforged/x.jar"
        ));
        assert!(is_mirror_url(
            "https://maven.minecraftforge.net/net/minecraftforge/x.jar"
        ));
    }

    /// 镜像源（URL 含 bmclapi）请求并发必须被限制在 MIRROR_CONCURRENCY 以内：
    /// 20 个并发任务同时下载镜像文件，服务器观测到的同时在处理的连接数 ≤ 8。
    #[tokio::test]
    async fn mirror_requests_capped_at_mirror_concurrency() {
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
                    let cur = a3.fetch_add(1, Ordering::SeqCst) + 1;
                    m3.fetch_max(cur, Ordering::SeqCst);
                    let mut buf = [0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    // 模拟下载耗时，给并发统计留出窗口
                    tokio::time::sleep(Duration::from_millis(80)).await;
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
            max <= constants::MIRROR_CONCURRENCY,
            "镜像并发应 ≤ {}，实际峰值 {}",
            constants::MIRROR_CONCURRENCY,
            max
        );
        assert!(max > 1, "应存在真实并发（>1），实际 {}", max);
    }

    /// 冷启动预探测排序纯逻辑：Loader 镜像永远先行（官方 maven 限流语义）；
    /// 非 Loader 探测快者先行；探测失败/无样本排后（不剔除）。
    #[test]
    fn sort_routes_with_probes_orders_fastest_first() {
        let mk = |url: &str| DownloadRoute {
            url: url.into(),
            is_mirror: url.contains("bmclapi"),
        };
        let ok = |ms: u64| ProbeResult {
            ok: true,
            ttfb_ms: ms,
        };
        let fail = ProbeResult {
            ok: false,
            ttfb_ms: 999,
        };

        // 非 Loader：官方快 → 官方前
        let mut r = vec![
            mk("https://libraries.minecraft.net/a.jar"),
            mk("https://bmclapi2.bangbang93.com/libraries/a.jar"),
        ];
        sort_routes_with_probes(
            &mut r,
            &HashMap::from([
                ("https://libraries.minecraft.net/a.jar".to_string(), ok(50)),
                (
                    "https://bmclapi2.bangbang93.com/libraries/a.jar".to_string(),
                    ok(400),
                ),
            ]),
            ResourceClass::Library,
        );
        assert!(
            r[0].url.contains("minecraft.net"),
            "官方快应先行: {}",
            r[0].url
        );

        // 非 Loader：官方慢/失败 → 镜像前
        let mut r = vec![
            mk("https://libraries.minecraft.net/a.jar"),
            mk("https://bmclapi2.bangbang93.com/libraries/a.jar"),
        ];
        sort_routes_with_probes(
            &mut r,
            &HashMap::from([
                ("https://libraries.minecraft.net/a.jar".to_string(), fail),
                (
                    "https://bmclapi2.bangbang93.com/libraries/a.jar".to_string(),
                    ok(80),
                ),
            ]),
            ResourceClass::Library,
        );
        assert!(
            r[0].url.contains("bmclapi"),
            "官方探测失败应镜像先行: {}",
            r[0].url
        );

        // Loader 类：镜像永远先行（即使官方探测更快）——官方 maven 限流语义
        let mut r = vec![
            mk("https://maven.minecraftforge.net/x.jar"),
            mk("https://bmclapi2.bangbang93.com/maven/x.jar"),
        ];
        sort_routes_with_probes(
            &mut r,
            &HashMap::from([
                ("https://maven.minecraftforge.net/x.jar".to_string(), ok(10)),
                (
                    "https://bmclapi2.bangbang93.com/maven/x.jar".to_string(),
                    ok(500),
                ),
            ]),
            ResourceClass::Loader,
        );
        assert!(
            r[0].url.contains("bmclapi"),
            "Loader 镜像先行语义不可被探测破坏: {}",
            r[0].url
        );

        // 无探测样本视同失败排后（不剔除候选）
        let mut r = vec![
            mk("https://libraries.minecraft.net/b.jar"),
            mk("https://bmclapi2.bangbang93.com/libraries/b.jar"),
        ];
        sort_routes_with_probes(
            &mut r,
            &HashMap::from([(
                "https://bmclapi2.bangbang93.com/libraries/b.jar".to_string(),
                ok(60),
            )]),
            ResourceClass::Library,
        );
        assert!(
            r[0].url.contains("bmclapi"),
            "无样本的官方应排后（镜像探测成功在前）: {}",
            r[0].url
        );
    }
}
