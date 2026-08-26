//! 下载源网络行为：多源回退的 HTTP 请求（元数据 JSON/XML 拉取）+ 列表 TTL 缓存。
//! 纯数据（DownloaderConfig 字段）留在 `config/downloader/model.rs`，
//! 网络请求逻辑归此文件——impl 跨文件不改变调用方写法。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::downloader::model::{DownloadSource, DownloadSourceMode, DownloaderConfig};
use crate::downloader::fetcher::http;
use crate::downloader::model::MIRROR_ROUTES;

/// 元数据请求的缓存/网络行为（离线模式 strict cache-only 的语义载体）：
/// - `DefaultTtl`：缓存优先（TTL 内命中直接返回），未命中发网络并写缓存——默认行为；
/// - `CacheOnly`：只读缓存——命中返回，未命中直接报错，**绝不发网络请求**（离线模式）；
/// - `MustRevalidate`：忽略缓存读，强制网络并写缓存（在线场景显式刷新）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheBehaviour {
    #[default]
    DefaultTtl,
    CacheOnly,
    MustRevalidate,
}

impl DownloadSource {
    /// 镜像 URL 重写：仅中国镜像源（manifest_url 含 bmclapi）生效，
    /// 把官方域 URL 按 MIRROR_ROUTES（单一表，见 downloader/model.rs）重写到 BMCLAPI 镜像前缀。
    pub fn mirror_url(&self, original: &str) -> String {
        if !self.manifest_url.contains("bmclapi") {
            return original.to_string();
        }
        for (domain, replacement) in MIRROR_ROUTES {
            if original.contains(domain) {
                let path = original.split(domain).nth(1).unwrap_or("");
                return format!("{}{}", replacement, path);
            }
        }
        original.to_string()
    }
}

impl DownloaderConfig {
    /// 按模式返回依次尝试的源列表。
    /// Auto（自适应路由）：源顺序仅作 URL 模板构造兜底，实际路由由 fetcher 层
    /// 按资源类 + 健康度决定（官方 → 镜像候选即时生成），此处官方优先（4s 降级）。
    pub(crate) fn sources_in_order(&self, mode: DownloadSourceMode) -> Vec<&DownloadSource> {
        match mode {
            DownloadSourceMode::OfficialOnly => vec![&self.sources[0]],
            DownloadSourceMode::ChinaOnly => vec![&self.sources[1]],
            DownloadSourceMode::Auto => vec![&self.sources[0], &self.sources[1]],
        }
    }

    /// 文件下载主源（按 file_source_mode 选择），文件下载失败不会跨源回退。
    pub fn file_primary_source(&self) -> Option<&DownloadSource> {
        match self.file_source_mode {
            DownloadSourceMode::OfficialOnly | DownloadSourceMode::Auto => self.sources.first(),
            DownloadSourceMode::ChinaOnly => self.sources.last(),
        }
    }

    /// 文件下载按 file_source_mode 的完整源顺序（含备用源，用于过慢降级）。
    pub fn file_sources_list(&self) -> Vec<&DownloadSource> {
        self.sources_in_order(self.file_source_mode)
    }

    /// 优先模式下主源判慢时限（秒）：超时即降级到备用源。
    /// 4s：官方 maven/metadata 实测 2.5-4.7s 能完成，2s 阈值频繁误判降级到更差的镜像路径；
    /// 国内环境官方源连接/响应普遍慢，过长则镜像源接管慢。
    const SLOW_SOURCE_TIMEOUT_SECS: u64 = 4;

    /// Try fetching from each source in order; returns first success.
    /// 优先模式：主源超过 SLOW_SOURCE_TIMEOUT_SECS 未完成（过慢）即降级到备用源，不等待失败。
    /// 结果按主源 URL 缓存 TTL 10 分钟（maven-metadata / loader 列表等低频变化数据，
    /// 冷启动后二次访问免网络等待）。
    /// `cache` 为 `CacheOnly` 时只读缓存、绝不发网络（离线模式语义）。
    pub async fn fetch_with_fallback(
        &self,
        mode: DownloadSourceMode,
        cache: CacheBehaviour,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let key = make_url(self.sources_in_order(mode)[0]);
        match cache {
            CacheBehaviour::CacheOnly => list_cache_get(&key).ok_or_else(|| {
                "离线模式：该元数据无缓存（需联网获取一次后才能离线使用，请联网后重试）".to_string()
            }),
            CacheBehaviour::MustRevalidate => {
                let text = self.fetch_with_fallback_uncached(mode, make_url).await?;
                list_cache_set(key, text.clone());
                Ok(text)
            }
            CacheBehaviour::DefaultTtl => {
                if let Some(cached) = list_cache_get(&key) {
                    return Ok(cached);
                }
                let text = self.fetch_with_fallback_uncached(mode, make_url).await?;
                list_cache_set(key, text.clone());
                Ok(text)
            }
        }
    }

    async fn fetch_with_fallback_uncached(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let order = self.sources_in_order(mode);
        let mut last_err = String::new();
        // 无镜像源的备用 URL 与主源相同（如 quilt 无 BMCLAPI 镜像）：
        // 跳过重复请求，避免"主源 4s 超时 → 备用源同 URL 再等 15s"的白等。
        let mut tried: Vec<String> = Vec::new();
        for (i, source) in order.iter().enumerate() {
            let url = make_url(source);
            if tried.iter().any(|u| u == &url) {
                eprintln!(
                    "[meta] 源{}({i}) 跳过: {url}（与已尝试源同 URL）",
                    source.name
                );
                continue;
            }
            tried.push(url.clone());
            // 每个源显式限时（双保险）：
            //  ① reqwest RequestBuilder::timeout——请求级超时（连接+TLS+body）
            //  ② tokio::time::timeout 包整个请求——独立计时器，reqwest 超时在
            //     TLS 黑洞/代理接管场景若失效仍能掐断。
            // 主源 4s 过慢降级；备用源 15s（元数据都是小文件，黑洞/假死立即掐断换下一源）。
            let limit = if i == 0 && order.len() > 1 {
                Duration::from_secs(Self::SLOW_SOURCE_TIMEOUT_SECS)
            } else {
                Duration::from_secs(15)
            };
            let req = http().get(&url).timeout(limit);
            eprintln!("[meta] 请求开始: {url}（limit={limit:?}）");
            let fut = async {
                let resp = req.send().await.map_err(|e| format!("请求失败\n{e}"))?;
                if !resp.status().is_success() {
                    return Err(format!("HTTP {}", resp.status()));
                }
                resp.text().await.map_err(|e| format!("读取响应失败\n{e}"))
            };
            match tokio::time::timeout(limit, fut).await {
                Ok(Ok(text)) => return Ok(text),
                Ok(Err(e)) => {
                    last_err = e.clone();
                    eprintln!("[meta] 源{}({i}) 失败: {url} → {e}", source.name);
                }
                Err(_) => {
                    last_err = format!("请求超时（{limit:?}）");
                    eprintln!("[meta] 源{}({i}) 超时: {url}（{limit:?}）", source.name);
                }
            }
        }
        Err(last_err)
    }

    /// Try fetching JSON from each source; returns first success（经文本层缓存复用）。
    /// `cache` 为 `CacheOnly` 时只读缓存、绝不发网络（离线模式语义）。
    pub async fn fetch_json_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        mode: DownloadSourceMode,
        cache: CacheBehaviour,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<T, String> {
        let text = self.fetch_with_fallback(mode, cache, make_url).await?;
        serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败\n{}", e))
    }
}

// ── 列表 TTL 缓存：元数据拉取结果按主源 URL 缓存（纯内存，TTL 自动过期）──

const LIST_CACHE_TTL_SECS: u64 = 600;
const LIST_CACHE_MAX: usize = 64;

fn list_cache() -> &'static Mutex<HashMap<String, (Instant, String)>> {
    static C: std::sync::OnceLock<Mutex<HashMap<String, (Instant, String)>>> =
        std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn list_cache_get(url: &str) -> Option<String> {
    let m = list_cache().lock();
    let (t, v) = m.get(url)?;
    if t.elapsed() > Duration::from_secs(LIST_CACHE_TTL_SECS) {
        return None;
    }
    Some(v.clone())
}

fn list_cache_set(url: String, val: String) {
    let mut m = list_cache().lock();
    if m.len() >= LIST_CACHE_MAX {
        m.clear();
    }
    m.insert(url, (Instant::now(), val));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_cache_roundtrip() {
        let url = "https://example.com/maven-metadata.xml";
        assert!(list_cache_get(url).is_none(), "未写入时不应命中");
        list_cache_set(url.into(), "v1".into());
        assert_eq!(list_cache_get(url).as_deref(), Some("v1"));
        list_cache_set(url.into(), "v2".into());
        assert_eq!(list_cache_get(url).as_deref(), Some("v2"), "覆盖写应生效");
    }

    #[tokio::test]
    async fn cache_only_reads_cache_and_rejects_miss() {
        let cfg = DownloaderConfig::default();
        let make = |s: &DownloadSource| format!("{}/cacheonly-key.json", s.manifest_url);
        let key = make(cfg.sources_in_order(DownloadSourceMode::Auto)[0]);
        // 未命中：CacheOnly 直接报错（绝不发网络请求——即使 URL 不可达也只报缓存缺失）
        let err = cfg
            .fetch_with_fallback(DownloadSourceMode::Auto, CacheBehaviour::CacheOnly, make)
            .await
            .unwrap_err();
        assert!(
            err.contains("离线模式"),
            "CacheOnly 未命中应报离线错误: {err}"
        );
        // 命中：返回缓存内容，不发网络
        list_cache_set(key, "cached-v1".into());
        let ok = cfg
            .fetch_with_fallback(DownloadSourceMode::Auto, CacheBehaviour::CacheOnly, make)
            .await
            .unwrap();
        assert_eq!(ok, "cached-v1");
    }

    #[tokio::test]
    async fn must_revalidate_ignores_cache() {
        let cfg = DownloaderConfig::default();
        let make = |s: &DownloadSource| format!("{}/mustrevalidate-key.json", s.manifest_url);
        let key = make(cfg.sources_in_order(DownloadSourceMode::Auto)[0]);
        list_cache_set(key, "stale".into());
        // MustRevalidate 忽略缓存读，强制发网络（该路径不存在 → HTTP 404 / 网络错误），
        // 不会返回缓存中的 stale 内容；离线环境同样报网络错误，测试不依赖网络可用性。
        let err = cfg
            .fetch_with_fallback(
                DownloadSourceMode::Auto,
                CacheBehaviour::MustRevalidate,
                make,
            )
            .await
            .unwrap_err();
        assert!(!err.is_empty());
    }

    /// 黑洞源（accept 连接但不响应）必须被主源 4s 超时掐断并降级——
    /// 国内网络实测卡死场景（asset index 经 TUN 黑洞），若此测试挂起
    /// 说明 reqwest 的 per-request timeout 在该场景失效（需换请求层方案）。
    #[tokio::test]
    async fn fallback_primary_blackhole_times_out() {
        use crate::config::downloader::model::DownloadSource;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                // 黑洞：accept 后保持连接但不读写任何数据（模拟 TUN 假死源）
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                std::mem::forget(sock); // 泄漏 fd 保持连接挂起
            }
        });
        let blackhole = |port: u16| DownloadSource {
            name: "blackhole".into(),
            manifest_url: format!("http://127.0.0.1:{port}"),
            resource_url: format!("http://127.0.0.1:{port}/assets"),
            maven_url: format!("http://127.0.0.1:{port}/maven"),
            libraries_url: format!("http://127.0.0.1:{port}/libraries"),
            forge_list_url: "".into(),
            fabric_list_url: "".into(),
            neoforge_list_url: "".into(),
            quilt_list_url: "".into(),
            optifine_list_url: "".into(),
            forge_installer_url: "".into(),
            neoforge_installer_url: "".into(),
            fabric_loader_url: "".into(),
            fabric_profile_url: "".into(),
            quilt_profile_url: "".into(),
            optifine_download_url: "".into(),
        };
        let mut cfg = DownloaderConfig::default();
        // 主源 = 黑洞（4s 超时），备用源 = 不可达端口（快速失败）
        cfg.sources = vec![blackhole(port), blackhole(1)];

        let t0 = Instant::now();
        let res = cfg
            .fetch_with_fallback(
                DownloadSourceMode::Auto,
                CacheBehaviour::MustRevalidate,
                |s| format!("{}/test.json", s.manifest_url),
            )
            .await;
        let elapsed = t0.elapsed();
        assert!(res.is_err(), "全黑洞应失败: {:?}", res);
        assert!(
            elapsed.as_secs() < 8,
            "主源 4s 超时应触发并降级，实际 {}s（超时机制失效）",
            elapsed.as_secs_f64()
        );
    }
}
