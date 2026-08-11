//! 下载源网络行为：多源回退的 HTTP 请求（元数据 JSON/XML 拉取）+ 列表 TTL 缓存。
//! 纯数据（DownloaderConfig 字段）留在 `config/downloader/model.rs`，
//! 网络请求逻辑归此文件——impl 跨文件不改变调用方写法。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::downloader::model::{DownloadSource, DownloadSourceMode, DownloaderConfig};
use crate::downloader::fetcher::http;

impl DownloaderConfig {
    /// 按模式返回依次尝试的源列表。
    pub(crate) fn sources_in_order(&self, mode: DownloadSourceMode) -> Vec<&DownloadSource> {
        match mode {
            DownloadSourceMode::OfficialOnly => vec![&self.sources[0]],
            DownloadSourceMode::ChinaOnly => vec![&self.sources[1]],
            DownloadSourceMode::PreferOfficial => vec![&self.sources[0], &self.sources[1]],
            DownloadSourceMode::PreferChina => vec![&self.sources[1], &self.sources[0]],
        }
    }

    /// 优先模式下主源判慢时限（秒）：超时即降级到备用源。
    /// 4s：官方 maven/metadata 实测 2.5-4.7s 能完成，2s 阈值频繁误判降级到更差的镜像路径；
    /// 国内环境官方源连接/响应普遍慢，过长则镜像源接管慢。
    const SLOW_SOURCE_TIMEOUT_SECS: u64 = 4;

    /// Try fetching from each source in order; returns first success.
    /// 优先模式：主源超过 SLOW_SOURCE_TIMEOUT_SECS 未完成（过慢）即降级到备用源，不等待失败。
    /// 结果按主源 URL 缓存 TTL 10 分钟（maven-metadata / loader 列表等低频变化数据，
    /// 冷启动后二次访问免网络等待）。
    pub async fn fetch_with_fallback(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let key = make_url(self.sources_in_order(mode)[0]);
        if let Some(cached) = list_cache_get(&key) {
            return Ok(cached);
        }
        let text = self.fetch_with_fallback_uncached(mode, make_url).await?;
        list_cache_set(key, text.clone());
        Ok(text)
    }

    async fn fetch_with_fallback_uncached(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let order = self.sources_in_order(mode);
        let mut last_err = String::new();
        for (i, source) in order.iter().enumerate() {
            let url = make_url(source);
            let mut req = http().get(&url);
            if i == 0 && order.len() > 1 {
                req = req.timeout(Duration::from_secs(Self::SLOW_SOURCE_TIMEOUT_SECS));
            }
            match req.send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        last_err = format!("HTTP {}", resp.status());
                        continue;
                    }
                    match resp.text().await {
                        Ok(text) => return Ok(text),
                        Err(e) => last_err = format!("读取响应失败: {}", e),
                    }
                }
                Err(e) => last_err = format!("请求失败: {}", e),
            }
        }
        Err(last_err)
    }

    /// Try fetching JSON from each source; returns first success（经文本层缓存复用）。
    pub async fn fetch_json_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<T, String> {
        let text = self.fetch_with_fallback(mode, make_url).await?;
        serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))
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
    let m = list_cache().lock().unwrap();
    let (t, v) = m.get(url)?;
    if t.elapsed() > Duration::from_secs(LIST_CACHE_TTL_SECS) {
        return None;
    }
    Some(v.clone())
}

fn list_cache_set(url: String, val: String) {
    let mut m = list_cache().lock().unwrap();
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
}
