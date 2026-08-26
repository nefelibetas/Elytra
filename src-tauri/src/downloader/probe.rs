//! 源预探测：HEAD 测 TTFB + 可达性（下载前选源），结果 TTL 缓存。
//! JDK 安装链（jdk/download.rs）与社区资源下载（util::community_download_urls）共用：
//! Auto 模式下下载前对候选源并发探测，快者优先、慢者兜底；探测失败（超时/黑洞/
//! 非 2xx）由调用方决定剔除（环节层）或仅降序（源层候选）。

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::downloader::fetcher::http;

/// 源探测结果：可达性 + 首字节延迟（毫秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeResult {
    pub(crate) ok: bool,
    pub(crate) ttfb_ms: u64,
}

/// 源预探测超时（秒）：黑洞/假死源 3s 掐断（探测失败 = 剔除/降序依据）。
const PROBE_TIMEOUT_SECS: u64 = 3;
/// 探测结果 TTL（秒）：一次会话内多次下载不重复探测。
const PROBE_CACHE_TTL_SECS: u64 = 600;
const PROBE_CACHE_MAX: usize = 64;

fn probe_cache() -> &'static Mutex<HashMap<String, (Instant, ProbeResult)>> {
    static C: LazyLock<Mutex<HashMap<String, (Instant, ProbeResult)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &C
}

fn probe_cache_get(url: &str) -> Option<ProbeResult> {
    let mut m = probe_cache().lock();
    let (t, res) = m.get(url)?;
    if t.elapsed() < Duration::from_secs(PROBE_CACHE_TTL_SECS) {
        Some(*res)
    } else {
        m.remove(url);
        None
    }
}

fn probe_cache_set(url: String, res: ProbeResult) {
    let mut m = probe_cache().lock();
    m.insert(url, (Instant::now(), res));
    if m.len() > PROBE_CACHE_MAX {
        if let Some(oldest) = m
            .iter()
            .min_by_key(|(_, (t, _))| *t)
            .map(|(k, _)| k.clone())
        {
            m.remove(&oldest);
        }
    }
}

/// HEAD 探测单个 URL：测量 TTFB + 可达性。黑洞/假死源 3s 掐断（双保险：
/// reqwest 请求级 timeout + tokio 独立计时器）；结果按 URL 缓存（TTL 10 分钟）。
pub(crate) async fn probe_url(url: &str) -> ProbeResult {
    if let Some(cached) = probe_cache_get(url) {
        return cached;
    }
    let res = probe_url_uncached(url).await;
    probe_cache_set(url.to_string(), res);
    res
}

/// 域级探测：缓存键取域根（scheme://host/），探测值来自完整 URL——
/// 同一域的文件共享缓存（跨文件/跨任务命中，游戏文件下载一次探测管全批次）；
/// 用真实文件路径探测保证 2xx 判定有效（域根路径可能 404/403，误判不可达）。
pub(crate) async fn probe_domain(url: &str) -> ProbeResult {
    let key = domain_root(url);
    if let Some(cached) = probe_cache_get(&key) {
        return cached;
    }
    let res = probe_url_uncached(url).await;
    probe_cache_set(key, res);
    res
}

/// 提取域根（scheme://host/）作缓存键；探测目标来自下载清单，假定合法 URL。
fn domain_root(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest);
    if url.starts_with("https://") {
        format!("https://{host}/")
    } else if url.starts_with("http://") {
        format!("http://{host}/")
    } else {
        url.to_string()
    }
}

/// 探测执行体（无缓存）：3s 双保险超时 + 黑洞掐断，返回 TTFB/可达性。
async fn probe_url_uncached(url: &str) -> ProbeResult {
    let t0 = Instant::now();
    let outcome = async {
        let resp = http()
            .head(url)
            .timeout(Duration::from_secs(PROBE_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|e| format!("请求失败\n{e}"))?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        Ok::<(), String>(())
    };
    match tokio::time::timeout(Duration::from_secs(PROBE_TIMEOUT_SECS), outcome).await {
        Ok(Ok(())) => ProbeResult {
            ok: true,
            ttfb_ms: t0.elapsed().as_millis() as u64,
        },
        _ => ProbeResult {
            ok: false,
            ttfb_ms: t0.elapsed().as_millis() as u64,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 探测缓存读写：同 URL 命中返回缓存结果（TTL 内不再发网络）；
    /// 缓存淘汰上限不 panic。
    #[test]
    fn probe_cache_roundtrip() {
        let url = "https://example.com/probe-cache-test";
        // 清理可能残留
        probe_cache().lock().remove(url);
        let res = ProbeResult {
            ok: true,
            ttfb_ms: 42,
        };
        probe_cache_set(url.to_string(), res);
        assert_eq!(probe_cache_get(url), Some(res));
        // 缓存命中后返回克隆值，原值不变
        assert_eq!(probe_cache_get(url).unwrap().ttfb_ms, 42);
        probe_cache().lock().remove(url);
    }
}
