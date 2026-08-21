use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::{Duration, Instant};

use reqwest::Client;
use tokio::sync::Semaphore;

use crate::config::downloader::model::{DownloadSourceMode, DownloaderConfig, ResourceClass};

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
    route_health_map().lock().get(url).cloned().unwrap_or_default()
}

/// 记录成功：清零连续失败与冷却，TTFB EWMA 更新（α=0.25）。
fn record_route_success(url: &str, ttfb_ms: f64) {
    let mut m = route_health_map().lock();
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
    let mut m = route_health_map().lock();
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
        DownloadSourceMode::OfficialOnly | DownloadSourceMode::ChinaOnly => {
            // 单选：保持调用方源顺序（镜像候选由调用方按模式生成，无健康度竞争）
        }
    }
    routes
}

/// 重定向目标是否放行：跟随重定向，但拒绝实测黑洞节点域（17.749333.xyz——
/// BMCLAPI 对象存储节点，国内部分网络 TCP 黑洞，连上不传数据）。
/// 放行 minio.749333.xyz（bmclapi 文件下载主存储：OptiFine 下载 URL 实测
/// 302 → minio，按域名前缀一刀切会把真实下载误杀，安装必失败；
/// 无代理黑洞网络下 minio 不可达由全局 client 的 30s read_timeout 兜底，不永久挂死）。
fn redirect_allowed(host: &str) -> bool {
    !host.contains("749333") || host.starts_with("minio.")
}

fn mirror_safe_redirect() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        let host = attempt.url().host_str().unwrap_or("").to_string();
        if !redirect_allowed(&host) {
            eprintln!("[fetch] 拒绝重定向到黑洞节点: {host}");
            attempt.error("重定向到不可达镜像节点（749333.xyz）")
        } else {
            attempt.follow()
        }
    })
}

/// 解析 Windows 注册表 ProxyServer 值 → (http 代理, https 代理)。
/// 支持两种格式：
///   `127.0.0.1:7890`（全协议统一）
///   `http=127.0.0.1:7890;https=127.0.0.1:7891;ftp=...`（分协议，缺项为 None）
/// 代理连接 URL 统一用 http scheme（Clash 等 mixed 端口为明文 HTTP 代理；
/// 注册表不表达"到代理的连接是否加密"，默认明文）。
fn parse_proxy_server(raw: &str) -> (Option<String>, Option<String>) {
    let raw = raw.trim();
    let mut http = None;
    let mut https = None;
    if raw.contains('=') {
        for seg in raw.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let Some((scheme, addr)) = seg.split_once('=') else {
                continue;
            };
            let addr = addr.trim();
            if addr.is_empty() {
                continue;
            }
            match scheme.trim() {
                "http" => http = Some(format!("http://{addr}")),
                "https" => https = Some(format!("http://{addr}")),
                _ => {}
            }
        }
    } else if !raw.is_empty() {
        let full = format!("http://{raw}");
        http = Some(full.clone());
        https = Some(full);
    }
    (http, https)
}

/// 返回 (http 代理 URL, https 代理 URL)——按平台检测系统代理：
/// - Windows：注册表 Internet Settings（Clash 等系统代理模式）优先，环境变量兜底
/// - Linux：环境变量（https_proxy/http_proxy/all_proxy）优先，GNOME gsettings 兜底
/// - 其他平台：环境变量
fn system_proxy_urls() -> (Option<String>, Option<String>) {
    #[cfg(target_os = "windows")]
    {
        // 1. 注册表（开启"使用代理服务器"时 Clash 等系统代理写这里）
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        if let Ok(settings) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
        ) {
            let enabled: u32 = settings.get_value("ProxyEnable").unwrap_or(0);
            let server: Result<String, _> = settings.get_value("ProxyServer");
            if enabled == 1 {
                if let Ok(server) = server {
                    let (h, s) = parse_proxy_server(&server);
                    if h.is_some() || s.is_some() {
                        return (h, s);
                    }
                }
            }
        }
        // 2. 环境变量兜底
        env_proxy_urls()
    }
    #[cfg(target_os = "linux")]
    {
        // 1. 环境变量（终端/CI/容器场景；桌面端导出的常见方式）
        let env = env_proxy_urls();
        if env.0.is_some() || env.1.is_some() {
            return env;
        }
        // 2. GNOME 桌面系统代理（gsettings org.gnome.system.proxy）
        linux_gsettings_proxy_urls()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        env_proxy_urls()
    }
}

/// 环境变量代理（http_proxy / https_proxy / all_proxy；no_proxy 由 NoProxy 处理）。
fn env_proxy_urls() -> (Option<String>, Option<String>) {
    let https = std::env::var("https_proxy")
        .or_else(|_| std::env::var("HTTPS_PROXY"))
        .ok();
    let http = std::env::var("http_proxy")
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .ok()
        .or_else(|| std::env::var("all_proxy").ok())
        .or_else(|| std::env::var("ALL_PROXY").ok());
    (http, https)
}

/// GNOME 桌面系统代理（gsettings 命令；无 gsettings/非 manual 模式 → None）。
/// 平台无关实现（std::process::Command 跨平台），仅 Linux 分支调用；
/// 不加 cfg 包裹以便 Windows 构建做语法检查。
#[allow(dead_code)] // Windows 构建不调用
fn linux_gsettings_proxy_urls() -> (Option<String>, Option<String>) {
    fn gs(schema: &str, key: &str) -> Option<String> {
        let out = std::process::Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() || s == "''" {
            None
        } else {
            Some(s.trim_matches('\'').to_string())
        }
    }
    let Some(mode) = gs("org.gnome.system.proxy", "mode") else {
        return (None, None);
    };
    if mode != "manual" {
        return (None, None); // none / auto(PAC，不支持)
    }
    let http = gs("org.gnome.system.proxy.http", "host").map(|h| {
        let port = gs("org.gnome.system.proxy.http", "port").unwrap_or_else(|| "8080".into());
        format!("http://{h}:{port}")
    });
    let https = gs("org.gnome.system.proxy.https", "host").map(|h| {
        let port = gs("org.gnome.system.proxy.https", "port").unwrap_or_else(|| "8080".into());
        format!("http://{h}:{port}")
    });
    if http.is_none() && https.is_none() {
        return (None, None);
    }
    (http, https)
}

/// 当前网络代理模式（persist 配置；缺省 Auto）。
fn current_network_mode() -> crate::config::downloader::model::NetworkMode {
    crate::persist::read::<crate::config::downloader::model::DownloaderConfig>(
        crate::constants::SECT_DOWNLOADER,
    )
    .map(|c| c.network_mode)
    .unwrap_or_default()
}

/// 按网络模式给 ClientBuilder 应用代理：
/// - Direct：不应用（强制直连；TUN 模式由网络层接管，无需应用层配置）
/// - Auto / System：检测系统代理并应用（无代理时保持直连）
fn apply_system_proxy(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    use crate::config::downloader::model::NetworkMode;
    match current_network_mode() {
        NetworkMode::Direct => builder,
        NetworkMode::Auto | NetworkMode::System => {
            let (http_proxy, https_proxy) = system_proxy_urls();
            let mut b = builder;
            let no_proxy = reqwest::NoProxy::from_string("localhost,127.0.0.1,[::1]");
            if let Some(u) = http_proxy {
                if let Ok(p) = reqwest::Proxy::http(&u) {
                    eprintln!("[fetch] 应用系统代理 http: {u}");
                    b = b.proxy(p.no_proxy(no_proxy.clone()));
                }
            }
            if let Some(u) = https_proxy {
                if let Ok(p) = reqwest::Proxy::https(&u) {
                    eprintln!("[fetch] 应用系统代理 https: {u}");
                    b = b.proxy(p.no_proxy(no_proxy));
                }
            }
            b
        }
    }
}

fn build_client() -> Client {
    let mut builder = Client::builder()
        .user_agent(UA_DOWNLOAD)
        // connect 黑洞防挂死：部分镜像 302 到不可达对象存储节点（749333.xyz 实测 TCP 黑洞），
        // 全局 client 无连接超时会挂满 300s；2s 连接超时只在新建连接时生效，不影响连接池复用
        .connect_timeout(std::time::Duration::from_secs(2))
        // 读黑洞防挂死：连接建立后 30s 无数据（镜像假死）即掐断。此 client 服务
        // 版本清单/资源索引/元数据等小请求（fetch_with_fallback 降级源只靠总超时，
        // 无 read_timeout 时降级源黑洞会挂满 300s×源数，前端表现为进度永远 0%）
        .read_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(300))
        // 302 到黑洞节点时直接拒绝（reqwest 跟随重定向时无 per-hop 超时控制，
        // 黑名单是唯一在超时前拦住的方式）
        .redirect(mirror_safe_redirect())
        .pool_max_idle_per_host(16);
    builder = apply_system_proxy(builder);
    builder.build().expect("reqwest::Client")
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
    fetch_with(http(), url, dest, progress, None).await
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
    if urls.len() == 1 {
        let url = urls.first().map(String::as_str).unwrap_or("");
        return fetch_with_c(http(), url, dest, progress, None, cancel).await;
    }
    fetch_from_sources(urls, dest, &progress, None, None, cancel).await
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
    if urls.len() == 1 {
        let url = urls.first().map(String::as_str).unwrap_or("");
        return fetch_with_c(http(), url, dest, progress, Some(expected_sha1), cancel).await;
    }
    fetch_from_sources(urls, dest, &progress, None, Some(expected_sha1), cancel).await
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
    const MAX_ATTEMPTS: u32 = 3;
    let mut last_err = None;
    for attempt in 0..MAX_ATTEMPTS {
        match fetch_once(client, url, dest, &progress, expected_sha1, cancel).await {
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
    fetch_from_sources(urls, dest, &progress, Some(Duration::from_secs(60)), None, cancel).await
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

    /// 取消 flag 预置时下载应立即返回 Cancelled 且不留文件
    /// （此前下载循环不检查取消，任务取消要等 read_timeout/换源才生效）。
    #[tokio::test]
    async fn fetch_cancelled_returns_immediately() {
        let body = vec![b'x'; 256 * 1024];
        let url = serve(body).await;
        let dest = temp_dest("fetch_cancelled");
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(true)); // 预置已取消

        let t0 = std::time::Instant::now();
        let res =
            fetch_verified_c(&[url], &dest, None::<fn(u64, u64)>, "", Some(&cancel)).await;
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

    /// 重定向放行决策纯函数：黑洞子域拒绝、minio（bmclapi 下载存储）放行、
    /// 普通域放行。minio 放行是 OptiFine 安装的硬前提（下载 URL 302 → minio）。
    #[test]
    fn redirect_policy_minio_allowed() {
        assert!(!redirect_allowed("17.749333.xyz"), "实测黑洞子域必须拒绝");
        assert!(!redirect_allowed("cdn.749333.xyz"), "未知 749333 子域保持拒绝");
        assert!(redirect_allowed("minio.749333.xyz"), "bmclapi 下载存储必须放行");
        assert!(redirect_allowed("bmclapi2.bangbang93.com"));
        assert!(redirect_allowed("example.com"));
    }

    /// 真实 bmclapi OptiFine 下载（302 → minio.749333.xyz）：验证重定向放行后
    /// OptiFine 下载链路可用（黑名单按域名前缀一刀切时此测试必失败——
    /// minio 放行是 OptiFine 安装的硬前提）。网络测试，默认忽略。
    #[tokio::test]
    #[ignore]
    async fn bmclapi_optifine_real_download() {
        let dest = temp_dest("optifine_real");
        let res = fetch(
            "https://bmclapi2.bangbang93.com/optifine/1.20.1/HD_U/I6",
            &dest,
            None::<fn(u64, u64)>,
        )
        .await;
        assert!(res.is_ok(), "真实下载应成功: {:?}", res);
        let meta = std::fs::metadata(&dest).unwrap();
        assert!(
            meta.len() > 1_000_000,
            "OptiFine jar 应大于 1MB（实测 {} bytes）",
            meta.len()
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// 302 到黑洞节点域（749333.xyz）应被重定向策略立即拒绝（快速失败，
    /// 而不是跟随到 TCP 黑洞挂满超时——国内网络实测卡在 asset index 的根因）。
    #[tokio::test]
    async fn redirect_to_blackhole_domain_rejected() {        // 服务器 accept 3 次（fetch_with 有 3 轮重试），每轮都返回 302 → 黑洞域
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            for _ in 0..3 {
                let Ok((mut sock, _)) = listener.accept().await else { break };
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let head = "HTTP/1.1 302 Found\r\nLocation: https://17.749333.xyz:443/download/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = sock.write_all(head.as_bytes()).await;
            }
        });
        let url = format!("http://{addr}/f");
        let dest = temp_dest("redirect_blackhole");
        let t0 = std::time::Instant::now();
        let res = fetch_verified(&[url], &dest, None::<fn(u64, u64)>, "").await;
        let elapsed = t0.elapsed();
        eprintln!("黑洞重定向总耗时: {:.2}s res={res:?}", elapsed.as_secs_f64());
        assert!(res.is_err(), "黑洞重定向应失败，实际: {:?}", res);
        assert!(
            elapsed.as_secs() < 5,
            "应在超时前快速失败（{}s）",
            elapsed.as_secs_f64()
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// 302 到正常域应正常跟随（重定向策略只拒绝黑洞域，不破坏 BMCLAPI 常规 302）。
    #[tokio::test]
    async fn redirect_to_normal_domain_followed() {
        let good = b"followed-content".to_vec();
        let final_url = serve(good.clone()).await;
        let target = final_url.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let head = format!(
                "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = sock.write_all(head.as_bytes()).await;
        });
        let url = format!("http://{addr}/redirect");
        let dest = temp_dest("redirect_normal");
        let res = fetch_verified(&[url], &dest, None::<fn(u64, u64)>, &sha1_hex(&good)).await;
        assert!(res.is_ok(), "正常 302 应跟随成功: {:?}", res);
        assert!(
            crate::downloader::hash::file_sha1_matches(&dest, &sha1_hex(&good)),
            "落地内容应正确"
        );
        let _ = std::fs::remove_file(&dest);
    }

    /// Windows 注册表 ProxyServer 值解析：统一格式与分协议格式。
    #[test]
    fn parse_proxy_server_variants() {
        // 统一格式（全协议同一代理）
        let (h, s) = parse_proxy_server("127.0.0.1:7897");
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7897"));
        assert_eq!(s.as_deref(), Some("http://127.0.0.1:7897"));
        // 分协议格式（连接代理统一明文 http scheme）
        let (h, s) = parse_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891");
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(s.as_deref(), Some("http://127.0.0.1:7891"));
        // 仅 http（https 缺项）
        let (h, s) = parse_proxy_server("http=127.0.0.1:7890");
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(s, None);
        // 空值/空白
        let (h, s) = parse_proxy_server("");
        assert_eq!(h, None);
        assert_eq!(s, None);
        let (h, s) = parse_proxy_server("  ");
        assert_eq!(h, None);
        assert_eq!(s, None);
    }

    /// 环境变量代理检测（Linux/兜底路径的核心）。
    #[test]
    fn env_proxy_detects_http_and_https() {
        // 临时设置（串行测试；结束后恢复）
        let prev_https = std::env::var("https_proxy").ok();
        let prev_http = std::env::var("http_proxy").ok();
        std::env::set_var("https_proxy", "http://127.0.0.1:7890");
        std::env::set_var("http_proxy", "http://127.0.0.1:7891");
        let (h, s) = env_proxy_urls();
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7891"));
        assert_eq!(s.as_deref(), Some("http://127.0.0.1:7890"));
        // 清理
        match prev_https {
            Some(v) => std::env::set_var("https_proxy", v),
            None => std::env::remove_var("https_proxy"),
        }
        match prev_http {
            Some(v) => std::env::set_var("http_proxy", v),
            None => std::env::remove_var("http_proxy"),
        }
    }

    /// 系统代理应用：网络模式 auto/system 且本机开启系统代理时，
    /// http()（全局 client，已应用代理）应能经代理快速访问官方源
    /// （网络测试；direct 模式或无代理环境跳过）。
    #[tokio::test]
    async fn global_client_uses_system_proxy_when_enabled() {
        use crate::config::downloader::model::NetworkMode;
        if current_network_mode() == NetworkMode::Direct {
            return;
        }
        let (h, s) = system_proxy_urls();
        if h.is_none() && s.is_none() {
            return; // 未检测到系统代理：不依赖代理环境
        }
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            http().get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").send(),
        )
        .await
        .expect("经系统代理访问官方应在 15s 内返回（代理不可达时检查代理配置）");
        let resp = res.expect("请求应成功");
        assert!(resp.status().is_success(), "官方应可达: {}", resp.status());
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
        let routes = resolve_download_routes(&urls, DownloadSourceMode::Auto);
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
        let routes2 = resolve_download_routes(&urls2, DownloadSourceMode::Auto);
        assert_eq!(routes2.len(), 2);
    }

    /// 单选模式（OfficialOnly/ChinaOnly）保持调用方顺序（无健康度竞争）。
    #[test]
    fn resolve_routes_single_mode_keeps_order() {
        // OfficialOnly：调用方传官方 URL → 保持（自动追加镜像候选，顺序官方在前）
        let urls = vec!["https://libraries.minecraft.net/com/x/y.jar".to_string()];
        let routes = resolve_download_routes(&urls, DownloadSourceMode::OfficialOnly);
        assert!(
            !routes[0].is_mirror,
            "OfficialOnly 官方先行，实际: {}",
            routes[0].url
        );
        // ChinaOnly：调用方只传镜像 URL → 保持单一镜像
        let mirror = vec![
            "https://bmclapi2.bangbang93.com/libraries/com/x/y.jar".to_string(),
        ];
        let routes2 = resolve_download_routes(&mirror, DownloadSourceMode::ChinaOnly);
        assert_eq!(routes2.len(), 1, "ChinaOnly 单源不追加候选");
        assert!(
            routes2[0].is_mirror,
            "ChinaOnly 保持镜像，实际: {}",
            routes2[0].url
        );
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
