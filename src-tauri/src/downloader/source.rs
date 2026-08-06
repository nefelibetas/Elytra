//! 下载源网络行为：多源回退的 HTTP 请求（元数据 JSON/文本拉取）。
//! 纯数据（DownloaderConfig 字段）留在 `config/downloader/model.rs`，
//! 网络请求逻辑归此文件——impl 跨文件不改变调用方写法。

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
    /// 2s：国内环境官方源连接/响应普遍慢，缩短判慢让镜像源更快接管。
    const SLOW_SOURCE_TIMEOUT_SECS: u64 = 2;

    /// Try fetching from each source in order; returns first success.
    /// 优先模式：主源超过 2s 未完成（过慢）即降级到备用源，不等待失败。
    pub async fn fetch_with_fallback(
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
                req = req.timeout(std::time::Duration::from_secs(
                    Self::SLOW_SOURCE_TIMEOUT_SECS,
                ));
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

    /// Try fetching JSON from each source; returns first success.
    /// 优先模式：主源超过 2s 未完成（过慢）即降级到备用源，不等待失败。
    pub async fn fetch_json_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<T, String> {
        let order = self.sources_in_order(mode);
        let mut last_err = String::new();
        for (i, source) in order.iter().enumerate() {
            let url = make_url(source);
            let mut req = http().get(&url);
            if i == 0 && order.len() > 1 {
                req = req.timeout(std::time::Duration::from_secs(
                    Self::SLOW_SOURCE_TIMEOUT_SECS,
                ));
            }
            match req.send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        last_err = format!("HTTP {}", resp.status());
                        continue;
                    }
                    match resp.json::<T>().await {
                        Ok(data) => return Ok(data),
                        Err(e) => last_err = format!("解析 JSON 失败: {}", e),
                    }
                }
                Err(e) => last_err = format!("请求失败: {}", e),
            }
        }
        Err(last_err)
    }
}
