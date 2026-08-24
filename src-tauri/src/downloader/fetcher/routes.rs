//! 自适应路由（#5）：URL 自动分类 + 镜像候选即时生成 + 健康度排序。

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::config::downloader::model::{DownloadSourceMode, DownloaderConfig, ResourceClass};
use crate::constants::SECT_DOWNLOADER;
use crate::downloader::model::MIRROR_ROUTES;

/// 真镜像源判定（路由排序/镜像候选用，不含"官方 maven 限流"语义）：
/// 仅 BMCLAPI / MCIM 域。is_mirror_url 承担限流职责（含官方不稳 maven），
/// 路由的"镜像优先/追加候选"必须只看这里，避免官方 neoforged maven 被误判为镜像。
fn is_mirror_domain(url: &str) -> bool {
    url.contains("bmclapi") || url.contains("mcimirror")
}

/// 单条下载路由。
#[derive(Debug, Clone)]
pub(super) struct DownloadRoute {
    pub(super) url: String,
    /// 是否镜像源（并发限流判定与排序权重）。
    pub(super) is_mirror: bool,
}

/// 为官方 URL 生成镜像候选（无命中返回空）。
pub(super) fn mirror_candidates(original: &str) -> Vec<String> {
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
pub(super) struct RouteHealth {
    /// 连续失败次数（成功后清零）。
    pub(super) failures: u32,
    /// 连续失败 >=3 时的冷却截止（冷却期内路由排到最末）。
    pub(super) cooldown_until: Option<Instant>,
    /// 成功样本数（无样本 = 冷启动，排序权重最低）。
    pub(super) samples: u32,
    /// TTFB EWMA（毫秒，α=0.25，计划对齐 Axolotl）。
    pub(super) ttfb_ewma_ms: f64,
}

impl RouteHealth {
    pub(super) fn in_cooldown(&self) -> bool {
        self.cooldown_until.is_some_and(|t| t > Instant::now())
    }
}

fn route_health_map() -> &'static Mutex<std::collections::HashMap<String, RouteHealth>> {
    static M: LazyLock<Mutex<std::collections::HashMap<String, RouteHealth>>> =
        LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
    &M
}

pub(super) fn route_health(url: &str) -> RouteHealth {
    route_health_map().lock().get(url).cloned().unwrap_or_default()
}

/// 记录成功：清零连续失败与冷却，TTFB EWMA 更新（α=0.25）。
pub(super) fn record_route_success(url: &str, ttfb_ms: f64) {
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
pub(super) fn record_route_failure(url: &str) {
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
pub(super) fn source_mode() -> DownloadSourceMode {
    crate::persist::read::<DownloaderConfig>(SECT_DOWNLOADER)
        .map(|c| c.file_source_mode)
        .unwrap_or_default()
}

/// 生成路由序列（fetch_from_sources 入口）：
/// 原始 URL 列表（去重保序）→ 追加自动镜像候选（列表未含镜像时）→ 排序：
/// - Loader 类资源镜像前置（mirror_first_loader：Loader 资源永远镜像先行，官方保底）；
/// - Auto 模式按健康度：Loader 镜像优先 → 冷却中 → 连续失败多 → 无样本 → TTFB 升序。
pub(super) fn resolve_download_routes(
    urls: &[String],
    mode: DownloadSourceMode,
) -> Vec<DownloadRoute> {
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
