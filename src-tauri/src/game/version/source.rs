//! 下载源网络行为：多源回退元数据拉取 + 列表 TTL 缓存（纯数据模型在 config/downloader/model.rs）。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::config::downloader::{DownloadSource, DownloadSourceMode, DownloaderConfig};
use crate::downloader::endpoints::{BMCLAPI, MIRROR_ROUTES};
use crate::downloader::fetcher::http;
use crate::downloader::meta_cache;

/// 元数据请求缓存行为：`DefaultTtl` 命中即返回否则网络写缓存；`CacheOnly` 只读缓存、
/// 未命中直接报错且绝不发网络（离线模式）；`MustRevalidate` 忽略缓存读强制网络。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheBehaviour {
    #[default]
    DefaultTtl,
    CacheOnly,
    MustRevalidate,
}

impl DownloadSource {
    /// 镜像 URL 重写：仅 manifest_url == BMCLAPI 的源生效，按 MIRROR_ROUTES 重写到镜像前缀。
    pub fn mirror_url(&self, original: &str) -> String {
        if self.manifest_url != BMCLAPI {
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
    /// 文件下载用源顺序（与元数据顺序分离，见 `metadata_sources_in_order`）。
    pub(crate) fn sources_in_order(&self, mode: DownloadSourceMode) -> Vec<&DownloadSource> {
        match mode {
            DownloadSourceMode::OfficialOnly => vec![&self.sources[0]],
            DownloadSourceMode::MirrorOnly => vec![&self.sources[1]],
            DownloadSourceMode::Auto => vec![&self.sources[0], &self.sources[1]],
        }
    }

    /// 元数据（清单/version json/索引/loader 版本列表）专用源顺序：一律官方优先，
    /// 镜像仅作官方失败后的兜底。官方 meta 直连实测 0.2~0.5s，而 BMCLAPI 对这类
    /// KB 级 JSON 同样走 302 调度链（主控→节点→MinIO 预签名），反而慢、且节点质量波动。
    pub(crate) fn metadata_sources_in_order(
        &self,
        mode: DownloadSourceMode,
    ) -> Vec<&DownloadSource> {
        match mode {
            DownloadSourceMode::OfficialOnly => vec![&self.sources[0]],
            DownloadSourceMode::MirrorOnly | DownloadSourceMode::Auto => {
                vec![&self.sources[0], &self.sources[1]]
            }
        }
    }

    /// 文件下载主源（按 game_source_mode）；文件下载失败不跨源回退。
    pub fn file_primary_source(&self) -> Option<&DownloadSource> {
        match self.game_source_mode {
            DownloadSourceMode::OfficialOnly | DownloadSourceMode::Auto => self.sources.first(),
            DownloadSourceMode::MirrorOnly => self.sources.last(),
        }
    }

    /// 文件下载按 game_source_mode 的完整源顺序（含备用源，用于过慢降级）。
    pub fn file_sources_list(&self) -> Vec<&DownloadSource> {
        self.sources_in_order(self.game_source_mode)
    }

    /// 主源判慢时限（秒），超时即降级备用源；取 4s（官方 maven/metadata 实测 2.5-4.7s，2s 会误判降级）。
    const SLOW_SOURCE_TIMEOUT_SECS: u64 = 4;

    /// 依次尝试各源返回首个成功；优先模式下主源超过 SLOW_SOURCE_TIMEOUT_SECS 即降级备用源。
    /// 结果按主源 URL 缓存 TTL 10 分钟（低频变化数据，冷启动后二次访问免网络）。
    pub async fn fetch_with_fallback(
        &self,
        mode: DownloadSourceMode,
        cache: CacheBehaviour,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let key = make_url(self.metadata_sources_in_order(mode)[0]);
        match cache {
            CacheBehaviour::CacheOnly => list_cache_get(&key, true).ok_or_else(|| {
                "离线模式：该元数据无缓存（需联网获取一次后才能离线使用，请联网后重试）".to_string()
            }),
            CacheBehaviour::MustRevalidate => {
                let text = self.fetch_with_fallback_uncached(mode, make_url).await?;
                list_cache_set(key, text.clone());
                Ok(text)
            }
            CacheBehaviour::DefaultTtl => {
                if let Some(cached) = list_cache_get(&key, false) {
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
        let order = self.metadata_sources_in_order(mode);
        let mut last_err = String::new();
        // 无镜像源的备用 URL 与主源相同（如 quilt），跳过重复请求避免同 URL 二次等待。
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
            // 双保险限时：① reqwest 请求级 timeout（连接+TLS+body）；② tokio::time::timeout 独立计时器，
            // 在 TLS 黑洞/代理接管场景 reqwest 超时失效时仍能掐断。主源 4s 降级，备用源 15s。
            let limit = if i == 0 && order.len() > 1 {
                Duration::from_secs(Self::SLOW_SOURCE_TIMEOUT_SECS)
            } else {
                Duration::from_secs(15)
            };
            let req = http().get(&url).timeout(limit);
            eprintln!("[meta] 请求开始: {url}（limit={limit:?}）");
            let fut = async {
                let resp = req.send().await.map_err(common_err_ctx_key!("errors.network", "请求失败"))?;
                if !resp.status().is_success() {
                    bail_key!("errors.network", "请求失败\nHTTP {}", resp.status());
                }
                resp.text().await.map_err(common_err_ctx_key!("errors.network", "读取响应失败"))
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

    /// 依次尝试各源拉取并解析 JSON，返回首个成功（复用文本层缓存，离线语义同 `fetch_with_fallback`）。
    pub async fn fetch_json_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        mode: DownloadSourceMode,
        cache: CacheBehaviour,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<T, String> {
        let text = self.fetch_with_fallback(mode, cache, make_url).await?;
        serde_json::from_str(&text).map_err(common_err_ctx_key!("errors.parse", "解析 JSON 失败"))
    }
}

// ── 列表 TTL 缓存：元数据拉取结果按主源 URL 缓存（内存热度层 + 磁盘持久层）──
// 内存命中用于进程内去重；磁盘层（downloader::meta_cache）承担进程重启后的免网络命中
// 与离线（CacheOnly）兜底。TTL 分域在磁盘层内部判定（快变清单 10min / 稳定数据 24h）。

const LIST_CACHE_TTL_SECS: u64 = 600;
const LIST_CACHE_MAX: usize = 64;

fn list_cache() -> &'static LazyLock<Mutex<HashMap<String, (Instant, String)>>> {
    static C: LazyLock<Mutex<HashMap<String, (Instant, String)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &C
}

/// `allow_stale`：CacheOnly 离线兜底允许陈旧条目（磁盘层返回 body 不校验新鲜度），
/// DefaultTtl 只认新鲜条目。
fn list_cache_get(url: &str, allow_stale: bool) -> Option<String> {
    {
        let m = list_cache().lock();
        if let Some((t, v)) = m.get(url) {
            if t.elapsed() <= Duration::from_secs(LIST_CACHE_TTL_SECS) {
                return Some(v.clone());
            }
        }
    }
    // 内存 miss 或已过期 → 磁盘层兜底
    let max_age = if allow_stale {
        None
    } else {
        Some(meta_cache::ttl_for(url))
    };
    let body = meta_cache::get(url, max_age)?;
    list_cache_set(url.to_string(), body.clone());
    Some(body)
}

fn list_cache_set(url: String, val: String) {
    {
        let mut m = list_cache().lock();
        if m.len() >= LIST_CACHE_MAX {
            m.clear();
        }
        m.insert(url.clone(), (Instant::now(), val.clone()));
    }
    // 内存层独立于磁盘层：入内存的同时落盘（IO 失败静默，下进程失去该项）
    meta_cache::put(&url, &val);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::downloader::DownloadSource;

    #[test]
    fn list_cache_roundtrip() {
        crate::downloader::meta_cache::clear_all();
        list_cache().lock().clear();
        // 独立 key：磁盘层跨测试 run 持久，固定示例 URL 会命中上一轮的落盘条目
        let url = format!(
            "https://example.com/maven-metadata-{}.xml",
            std::process::id()
        );
        assert!(list_cache_get(&url, false).is_none(), "未写入时不应命中");
        list_cache_set(url.clone(), "v1".into());
        assert_eq!(list_cache_get(&url, false).as_deref(), Some("v1"));
        list_cache_set(url.clone(), "v2".into());
        assert_eq!(
            list_cache_get(&url, false).as_deref(),
            Some("v2"),
            "覆盖写应生效"
        );
    }

    #[tokio::test]
    async fn cache_only_reads_cache_and_rejects_miss() {
        let cfg = DownloaderConfig::default();
        let make = |s: &DownloadSource| format!("{}/cacheonly-key.json", s.manifest_url);
        let key = make(cfg.sources_in_order(DownloadSourceMode::Auto)[0]);
        // 未命中：CacheOnly 直接报错，绝不发网络（URL 不可达也只报缓存缺失）
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
        // MustRevalidate 忽略缓存读强制发网络，不返回 stale；测试不依赖网络可用性。
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

    /// 黑洞源（accept 但不响应）必须被主源 4s 超时掐断降级；挂起则说明 reqwest timeout 失效。
    #[tokio::test]
    async fn fallback_primary_blackhole_times_out() {
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
        // 主源 = 黑洞（4s 超时），备用源 = 不可达端口（快速失败）
        let cfg = DownloaderConfig {
            sources: vec![blackhole(port), blackhole(1)],
            ..Default::default()
        };

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

    /// MirrorOnly 下元数据仍官方优先（官方直连，镜像仅兜底）——与文件下载的硬镜像语义分离。
    #[tokio::test]
    async fn mirror_only_metadata_prefers_official() {
        let spawn = |body: &'static str| async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        break;
                    };
                    let mut buf = [0u8; 4096];
                    let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;
                    use tokio::io::AsyncWriteExt;
                    let _ = sock
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            )
                            .as_bytes(),
                        )
                        .await;
                    let _ = sock.shutdown().await;
                }
            });
            port
        };
        let official_port = spawn("official").await;
        let mirror_port = spawn("mirror").await;
        let src = |name: &str, port: u16| DownloadSource {
            name: name.into(),
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
        let cfg = DownloaderConfig {
            sources: vec![src("official", official_port), src("mirror", mirror_port)],
            game_source_mode: DownloadSourceMode::MirrorOnly,
            ..Default::default()
        };
        let text = cfg
            .fetch_with_fallback(
                DownloadSourceMode::MirrorOnly,
                CacheBehaviour::MustRevalidate,
                |s| format!("{}/test.json", s.manifest_url),
            )
            .await
            .expect("元数据应命中官方源");
        assert_eq!(text, "official", "MirrorOnly 元数据应官方优先");
        // 文件下载主源仍保持镜像语义（两套顺序互不串扰）
        assert_eq!(
            cfg.file_primary_source().unwrap().name,
            "mirror",
            "MirrorOnly 文件主源不应被元数据顺序影响"
        );
    }
}
