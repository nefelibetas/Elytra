use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::Client;
use tokio::sync::Semaphore;

use crate::config::downloader::model::{DownloadSourceMode, DownloaderConfig, ResourceClass};

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

/// 该 URL 是否走限流（镜像源 + 高并发不稳定的官方 maven，统一限 MIRROR_CONCURRENCY）。
/// 镜像源域名变化时在此追加；maven.neoforged.net / maven.quiltmc.org / maven.minecraftforge.net
/// 实测高并发下连接失败（服务器限流），无镜像兜底的源（quilt）尤其依赖此保护。
fn is_mirror_url(url: &str) -> bool {
    url.contains("bmclapi")
        || url.contains("mcimirror")
        || url.contains("maven.neoforged.net")
        || url.contains("maven.quiltmc.org")
        || url.contains("maven.minecraftforge.net")
}

/// 真镜像源判定（路由排序/镜像候选用，不含"官方 maven 限流"语义）：
/// 仅 BMCLAPI / MCIM 域。is_mirror_url 承担限流职责（含官方不稳 maven），
/// 路由的"镜像优先/追加候选"必须只看这里，避免官方 neoforged maven 被误判为镜像。
fn is_mirror_domain(url: &str) -> bool {
    url.contains("bmclapi") || url.contains("mcimirror")
}

// ── 自适应路由（#5）：URL 自动分类 + 镜像候选即时生成 + 健康度排序 ──

/// 单条下载路由。
#[derive(Debug, Clone)]
pub struct DownloadRoute {
    pub url: String,
    /// 是否镜像源（并发限流判定与排序权重）。
    pub is_mirror: bool,
}

/// 显式镜像路由表：任意官方 URL 即时生成 BMCLAPI 镜像候选
/// （官方 URL 始终保留在候选列表中保底），取代"主源是 BMCLAPI 才重写"的源模板限制。
/// 与 model.rs 的 REWRITE_RULES 同构（镜像 URL 形态保持一致），并补充 fabric-meta。
const MIRROR_ROUTES: &[(&str, &str)] = &[
    (
        "resources.download.minecraft.net",
        "https://bmclapi2.bangbang93.com/assets",
    ),
    (
        "libraries.minecraft.net",
        "https://bmclapi2.bangbang93.com/libraries",
    ),
    ("piston-meta.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("piston-data.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("launcher.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("launchermeta.mojang.com", "https://bmclapi2.bangbang93.com"),
    // ModLoader 官方 maven → BMCLAPI /maven（完整镜像，302 实测可用）
    (
        "maven.neoforged.net",
        "https://bmclapi2.bangbang93.com/maven",
    ),
    (
        "maven.minecraftforge.net",
        "https://bmclapi2.bangbang93.com/maven",
    ),
    ("maven.fabricmc.net", "https://bmclapi2.bangbang93.com/maven"),
    // fabric 版本清单镜像（quilt 无镜像，保持直连官方）
    (
        "meta.fabricmc.net",
        "https://bmclapi2.bangbang93.com/fabric-meta",
    ),
];

/// 为官方 URL 生成镜像候选（无命中返回空）。
fn mirror_candidates(original: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (domain, replacement) in MIRROR_ROUTES {
        if original.contains(domain) {
            let path = original.split(domain).nth(1).unwrap_or("");
            out.push(format!("{replacement}{path}"));
        }
    }
    out
}

/// 单条路由的健康状态（全进程共享，按 URL 记录）。
#[derive(Debug, Clone, Default)]
struct RouteHealth {
    /// 连续失败次数（成功后清零）。
    failures: u32,
    /// 连续失败 >=3 时的冷却截止（冷却期内路由排到最末）。
    cooldown_until: Option<Instant>,
    /// 成功样本数（无样本 = 冷启动，排序权重最低）。
    samples: u32,
    /// TTFB EWMA（毫秒，α=0.25，计划对齐 Axolotl）。
    ttfb_ewma_ms: f64,
}

impl RouteHealth {
    fn in_cooldown(&self) -> bool {
        self.cooldown_until.is_some_and(|t| t > Instant::now())
    }
}

fn route_health_map() -> &'static Mutex<std::collections::HashMap<String, RouteHealth>> {
    static M: OnceLock<Mutex<std::collections::HashMap<String, RouteHealth>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn route_health(url: &str) -> RouteHealth {
    route_health_map().lock().unwrap().get(url).cloned().unwrap_or_default()
}

/// 记录成功：清零连续失败与冷却，TTFB EWMA 更新（α=0.25）。
fn record_route_success(url: &str, ttfb_ms: f64) {
    let mut m = route_health_map().lock().unwrap();
    let h = m.entry(url.to_string()).or_default();
    h.failures = 0;
    h.cooldown_until = None;
    h.samples = h.samples.saturating_add(1);
    h.ttfb_ewma_ms = if h.samples <= 1 {
        ttfb_ms
    } else {
        h.ttfb_ewma_ms * 0.75 + ttfb_ms * 0.25
    };
}

/// 记录失败：连续失败数 +1；>=3 进入 30s 冷却（冷却中的源排在路由最末）。
fn record_route_failure(url: &str) {
    const COOLDOWN_SECS: u64 = 30;
    const FAILURE_THRESHOLD: u32 = 3;
    let mut m = route_health_map().lock().unwrap();
    let h = m.entry(url.to_string()).or_default();
    h.failures = h.failures.saturating_add(1);
    if h.failures >= FAILURE_THRESHOLD {
        h.cooldown_until = Some(Instant::now() + Duration::from_secs(COOLDOWN_SECS));
    }
}

/// 当前文件下载源模式（Auto 时启用健康度路由；路由层兜底读全局配置）。
fn source_mode() -> DownloadSourceMode {
    crate::persist::read::<DownloaderConfig>(SECT_DOWNLOADER)
        .map(|c| c.file_source_mode)
        .unwrap_or_default()
}

/// 生成路由序列（fetch_from_sources 入口）：
/// 原始 URL 列表（去重保序）→ 追加自动镜像候选（列表未含镜像时）→ 排序：
/// - Loader 类资源镜像前置（mirror_first_loader：Loader 资源永远镜像先行，官方保底）；
/// - Auto 模式按健康度：Loader 镜像优先 → 冷却中 → 连续失败多 → 无样本 → TTFB 升序。
fn resolve_download_routes(urls: &[String], mode: DownloadSourceMode) -> Vec<DownloadRoute> {
    let mut seen: Vec<String> = Vec::new();
    for u in urls {
        if !u.is_empty() && !seen.contains(u) {
            seen.push(u.clone());
        }
    }
    let first = seen.first().map(String::as_str).unwrap_or("");
    let class = crate::config::downloader::model::infer_resource_class(first);
    // 原始列表已含镜像（调用方手工构造的双源）则不重复追加
    if !seen.iter().any(|u| is_mirror_domain(u)) {
        for cand in mirror_candidates(first) {
            if !seen.contains(&cand) {
                seen.push(cand);
            }
        }
    }
    let mut routes: Vec<DownloadRoute> = seen
        .into_iter()
        .map(|url| DownloadRoute {
            is_mirror: is_mirror_domain(&url),
            url,
        })
        .collect();
    match mode {
        DownloadSourceMode::Auto => {
            // 健康度排序（稳定：同键保持原始顺序 = 调用方源顺序）
            // 键值越小越靠前：非冷却 → Loader 镜像优先 → 失败少 → 有样本 → TTFB 小。
            // 冷却优先级高于镜像优先：镜像被冷却时排最末（计划："冷却中 → 失败多 → 无样本"）。
            routes.sort_by_key(|r| {
                let h = route_health(&r.url);
                let loader_mirror_first =
                    u8::from(!(class == ResourceClass::Loader && r.is_mirror));
                let cooldown = u8::from(h.in_cooldown());
                let has_sample = u8::from(h.samples == 0);
                (
                    cooldown,
                    loader_mirror_first,
                    h.failures,
                    has_sample,
                    h.ttfb_ewma_ms as u64,
                )
            });
        }
        DownloadSourceMode::PreferChina
        | DownloadSourceMode::PreferOfficial
        | DownloadSourceMode::OfficialOnly
        | DownloadSourceMode::ChinaOnly => {
            // 非 Auto：仅 mirror_first_loader 规则生效（其余保持调用方顺序）
            if class == ResourceClass::Loader && mode == DownloadSourceMode::PreferChina {
                routes.sort_by_key(|r| !r.is_mirror); // 镜像 key=false → 排前，官方保底排后
            }
        }
    }
    routes
}

fn build_client() -> Client {
    Client::builder()
        .user_agent(UA_DOWNLOAD)
        // connect 黑洞防挂死：部分镜像 302 到不可达对象存储节点（749333.xyz 实测 TCP 黑洞），
        // 全局 client 无连接超时会挂满 300s；2s 连接超时只在新建连接时生效，不影响连接池复用
        .connect_timeout(std::time::Duration::from_secs(2))
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
            // 存储的字符串自带「中文首行\n技术细节」结构（见各处构造点），直接原样输出
            FetchError::Network(e) => write!(f, "{e}"),
            FetchError::Io(e) => write!(f, "{e}"),
            FetchError::InvalidUrl => write!(f, "下载地址无效"),
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
            Ok(out) => return Ok(out.dl),
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
/// 单次请求结果（含 TTFB 供健康度记录）。
struct FetchOnceOutput {
    dl: DownloadResult,
    /// 首字节耗时（毫秒，发送请求到响应头到达）。
    ttfb_ms: f64,
}

async fn fetch_from_sources<F: Fn(u64, u64)>(
    urls: &[String],
    dest: &Path,
    progress: &Option<F>,
    total_timeout: Option<Duration>,
    expected_sha1: Option<&str>,
) -> Result<DownloadResult, FetchError> {
    let max_speed = max_speed_bps();
    const MAX_ATTEMPTS: u32 = 3;
    // 自适应路由：去重 + 自动镜像候选（官方保底）+ mirror_first_loader/健康度排序。
    // 调用方传入的源顺序在非 Auto 模式下保持不变（向后兼容）。
    let routes = resolve_download_routes(urls, source_mode());
    // 收集每源/每轮全部错误（展示时去重），避免最后一个源覆盖前面的失败原因
    let mut errors: Vec<FetchError> = Vec::new();
    for attempt in 0..MAX_ATTEMPTS {
        for route in &routes {
            let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(2));
            if let Some(t) = total_timeout {
                builder = builder.timeout(t);
            }
            let client = match builder.build() {
                Ok(c) => c,
                Err(e) => {
                    errors.push(FetchError::Network(format!("{}: {e}", route.url)));
                    continue;
                }
            };
            match fetch_once(&client, &route.url, dest, progress, max_speed, expected_sha1).await
            {
                Ok(out) => {
                    record_route_success(&route.url, out.ttfb_ms);
                    return Ok(out.dl);
                }
                Err(e) => {
                    // 失败清理残留（下一源/下一轮会重新创建）
                    let _ = std::fs::remove_file(dest);
                    record_route_failure(&route.url);
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
    max_speed: u64,
    expected_sha1: Option<&str>,
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
        let chunk = chunk.map_err(|e| FetchError::Network(format!("请求失败\n{e}")))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| FetchError::Io(format!("写入文件失败\n{e}")))?;
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
        // 高并发不稳的官方 maven 同样限流
        assert!(is_mirror_url("https://maven.quiltmc.org/repository/release/org/quiltmc/x.jar"));
        assert!(is_mirror_url("https://maven.neoforged.net/releases/net/neoforged/x.jar"));
        assert!(is_mirror_url("https://maven.minecraftforge.net/net/minecraftforge/x.jar"));
    }

    /// 镜像候选生成：mojang/library/loader 域 → BMCLAPI 镜像（与 model.rs REWRITE_RULES 同构）。
    #[test]
    fn mirror_candidates_generate_bmclapi_urls() {
        assert_eq!(
            mirror_candidates("https://libraries.minecraft.net/com/x/y.jar"),
            vec!["https://bmclapi2.bangbang93.com/libraries/com/x/y.jar"]
        );
        assert_eq!(
            mirror_candidates("https://resources.download.minecraft.net/ab/hash"),
            vec!["https://bmclapi2.bangbang93.com/assets/ab/hash"]
        );
        assert_eq!(
            mirror_candidates("https://maven.neoforged.net/releases/net/neoforged/x/1.0/x-1.0.jar"),
            vec!["https://bmclapi2.bangbang93.com/maven/releases/net/neoforged/x/1.0/x-1.0.jar"]
        );
        assert_eq!(
            mirror_candidates("https://meta.fabricmc.net/v2/versions/loader/1.20.4"),
            vec!["https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/1.20.4"]
        );
        // 未知域不生成
        assert!(mirror_candidates("https://example.com/x.jar").is_empty());
    }

    /// 路由解析：去重保序 + 未含镜像时自动追加候选。
    #[test]
    fn resolve_routes_dedupes_and_appends_mirror_candidate() {
        let urls = vec![
            "https://libraries.minecraft.net/com/x/y.jar".to_string(),
            "https://libraries.minecraft.net/com/x/y.jar".to_string(), // 重复
        ];
        let routes = resolve_download_routes(&urls, DownloadSourceMode::PreferOfficial);
        assert_eq!(routes.len(), 2, "去重后原始 1 + 镜像候选 1");
        assert_eq!(routes[0].url, "https://libraries.minecraft.net/com/x/y.jar");
        assert_eq!(routes[1].url, "https://bmclapi2.bangbang93.com/libraries/com/x/y.jar");
        assert!(!routes[0].is_mirror);
        assert!(routes[1].is_mirror);
        // 调用方已含镜像（手工双源）→ 不再重复追加
        let urls2 = vec![
            "https://maven.fabricmc.net/a.jar".to_string(),
            "https://bmclapi2.bangbang93.com/maven/a.jar".to_string(),
        ];
        let routes2 = resolve_download_routes(&urls2, DownloadSourceMode::PreferOfficial);
        assert_eq!(routes2.len(), 2);
    }

    /// mirror_first_loader：PreferChina 下 Loader 类镜像前置（官方保底排后）。
    #[test]
    fn resolve_routes_loader_mirror_first_in_china_mode() {
        let urls = vec![
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.21/forge-1.21-installer.jar"
                .to_string(),
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.21/forge-1.21-installer.jar"
                .to_string(),
        ];
        let routes = resolve_download_routes(&urls, DownloadSourceMode::PreferChina);
        assert!(
            routes[0].is_mirror,
            "Loader 资源镜像应前置，实际: {}",
            routes[0].url
        );
        assert!(!routes[1].is_mirror, "官方保底排后");
        // PreferOfficial 下保持调用方顺序（官方先行）
        let routes2 = resolve_download_routes(&urls, DownloadSourceMode::PreferOfficial);
        assert!(!routes2[0].is_mirror);
    }

    /// 健康度：连续失败 >=3 进入冷却（Auto 排序中冷却路由排最末）。
    #[test]
    fn route_health_cooldown_after_consecutive_failures() {
        let url = "https://example.com/bad-route.jar";
        let mut h = route_health(url);
        assert_eq!(h.failures, 0);
        assert!(!h.in_cooldown());
        record_route_failure(url);
        record_route_failure(url);
        record_route_failure(url);
        h = route_health(url);
        assert_eq!(h.failures, 3);
        assert!(h.in_cooldown(), "连续 3 次失败应进入 30s 冷却");
        // 成功重置失败数并清除冷却
        record_route_success(url, 120.0);
        h = route_health(url);
        assert_eq!(h.failures, 0);
        assert!(!h.in_cooldown());
        assert_eq!(h.samples, 1);
        // TTFB EWMA 更新（α=0.25）
        record_route_success(url, 200.0);
        h = route_health(url);
        assert!((h.ttfb_ewma_ms - 140.0).abs() < 0.01, "EWMA 应收敛到 140，实际 {}", h.ttfb_ewma_ms);
    }

    /// Auto 模式：冷却中的路由排最末；Loader 镜像冷启动优先。
    #[test]
    fn resolve_routes_auto_sorts_by_health() {
        let official = "https://maven.neoforged.net/releases/x.jar".to_string();
        let mirror = "https://bmclapi2.bangbang93.com/maven/releases/x.jar".to_string();
        let urls = vec![official.clone(), mirror.clone()];
        // 冷启动（无样本）：Loader 镜像优先
        let routes = resolve_download_routes(&urls, DownloadSourceMode::Auto);
        assert!(routes[0].is_mirror, "Auto + Loader 冷启动镜像优先: {}", routes[0].url);
        // 镜像连续失败进入冷却 → 官方排前
        record_route_failure(&mirror);
        record_route_failure(&mirror);
        record_route_failure(&mirror);
        let routes2 = resolve_download_routes(&urls, DownloadSourceMode::Auto);
        assert!(
            !routes2[0].is_mirror,
            "冷却中的镜像应排最末，实际: {}",
            routes2[0].url
        );
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
