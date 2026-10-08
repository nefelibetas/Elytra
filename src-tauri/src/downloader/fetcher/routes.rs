//! 自适应路由（#5）：URL 自动分类 + 镜像候选即时生成 + 健康度排序。

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::config::downloader::{
    infer_resource_class, DownloadSourceMode, DownloaderConfig, ResourceClass,
};
use crate::constants::SECT_DOWNLOADER;
use crate::downloader::endpoints::MIRROR_ROUTES;
use crate::persist;

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
    route_health_map()
        .lock()
        .get(url)
        .cloned()
        .unwrap_or_default()
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
    persist::read::<DownloaderConfig>(SECT_DOWNLOADER)
        .map(|c| c.game_source_mode)
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
    let class = infer_resource_class(first);
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
        DownloadSourceMode::OfficialOnly | DownloadSourceMode::MirrorOnly => {
            // 单选：保持调用方源顺序（镜像候选由调用方按模式生成，无健康度竞争）
        }
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::{
        mirror_candidates, record_route_failure, record_route_success, resolve_download_routes,
        route_health,
    };
    use crate::config::downloader::DownloadSourceMode;

    /// 镜像候选生成：mojang/library/loader 域 → BMCLAPI 镜像（单一表在 downloader/model.rs）。
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
        assert_eq!(
            routes[1].url,
            "https://bmclapi2.bangbang93.com/libraries/com/x/y.jar"
        );
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

    /// 单选模式（OfficialOnly/MirrorOnly）保持调用方顺序（无健康度竞争）。
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
        // MirrorOnly：调用方只传镜像 URL → 保持单一镜像
        let mirror = vec!["https://bmclapi2.bangbang93.com/libraries/com/x/y.jar".to_string()];
        let routes2 = resolve_download_routes(&mirror, DownloadSourceMode::MirrorOnly);
        assert_eq!(routes2.len(), 1, "MirrorOnly 单源不追加候选");
        assert!(
            routes2[0].is_mirror,
            "MirrorOnly 保持镜像，实际: {}",
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
        assert!(
            (h.ttfb_ewma_ms - 140.0).abs() < 0.01,
            "EWMA 应收敛到 140，实际 {}",
            h.ttfb_ewma_ms
        );
    }

    /// Auto 模式：冷却中的路由排最末；Loader 镜像冷启动优先。
    #[test]
    fn resolve_routes_auto_sorts_by_health() {
        let official = "https://maven.neoforged.net/releases/x.jar".to_string();
        let mirror = "https://bmclapi2.bangbang93.com/maven/releases/x.jar".to_string();
        let urls = vec![official.clone(), mirror.clone()];
        // 冷启动（无样本）：Loader 镜像优先
        let routes = resolve_download_routes(&urls, DownloadSourceMode::Auto);
        assert!(
            routes[0].is_mirror,
            "Auto + Loader 冷启动镜像优先: {}",
            routes[0].url
        );
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
}
