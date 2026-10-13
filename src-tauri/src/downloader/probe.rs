//! 源预探测：HEAD 测 TTFB（选源）与流式测吞吐（大包选源），结果按 URL TTL 缓存；Auto 模式下载前并发探测，快者优先，失败由调用方决定剔除或仅降序。

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use parking_lot::Mutex;

use crate::downloader::fetcher::http;

/// 源探测结果：可达性 + 首字节延迟（毫秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeResult {
    pub(crate) ok: bool,
    pub(crate) ttfb_ms: u64,
}

/// 速度探测结果：可达性 + 有效吞吐（字节/秒，不可达为 0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpeedProbeResult {
    pub(crate) ok: bool,
    pub(crate) speed_bps: u64,
}

/// 源预探测超时（秒）：黑洞/假死源 3s 掐断（失败 = 剔除/降序依据）。
const PROBE_TIMEOUT_SECS: u64 = 3;
/// 速度探测超时（秒）：要跑满 2 MB 样本，比 TTFB 探测放宽。
const SPEED_PROBE_TIMEOUT_SECS: u64 = 8;
/// 速度探测样本大小：2 MB。
const SPEED_PROBE_BYTES: u64 = 2 * 1024 * 1024;
/// 探测结果 TTL（秒）：会话内两类探测不重复。
const PROBE_CACHE_TTL_SECS: u64 = 600;
const PROBE_CACHE_MAX: usize = 64;

/// 取缓存（TTL 内命中即返回，过期项顺带清理）。
fn cache_get<T: Copy>(cache: &Mutex<HashMap<String, (Instant, T)>>, key: &str) -> Option<T> {
    let mut m = cache.lock();
    let (t, res) = m.get(key)?;
    if t.elapsed() < Duration::from_secs(PROBE_CACHE_TTL_SECS) {
        Some(*res)
    } else {
        m.remove(key);
        None
    }
}

/// 写缓存并淘汰最旧项（上限 `PROBE_CACHE_MAX`）。
fn cache_set<T: Copy>(cache: &Mutex<HashMap<String, (Instant, T)>>, key: String, res: T) {
    let mut m = cache.lock();
    m.insert(key, (Instant::now(), res));
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

fn probe_cache() -> &'static Mutex<HashMap<String, (Instant, ProbeResult)>> {
    static C: LazyLock<Mutex<HashMap<String, (Instant, ProbeResult)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &C
}

fn speed_cache() -> &'static Mutex<HashMap<String, (Instant, SpeedProbeResult)>> {
    static C: LazyLock<Mutex<HashMap<String, (Instant, SpeedProbeResult)>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &C
}

/// HEAD 探测单个 URL：测 TTFB + 可达性。3s 双保险超时（reqwest 请求级 + tokio 计时器）；结果按 URL 缓存。
pub(crate) async fn probe_url(url: &str) -> ProbeResult {
    if let Some(cached) = cache_get(probe_cache(), url) {
        return cached;
    }
    let res = probe_url_uncached(url).await;
    cache_set(probe_cache(), url.to_string(), res);
    res
}

/// 域级探测：缓存键取域根（scheme://host/），同域文件共享缓存；用真实文件路径探测保证 2xx 判定有效。
pub(crate) async fn probe_domain(url: &str) -> ProbeResult {
    let key = domain_root(url);
    if let Some(cached) = cache_get(probe_cache(), &key) {
        return cached;
    }
    let res = probe_url_uncached(url).await;
    cache_set(probe_cache(), key, res);
    res
}

/// 流式探测单个 URL 的下载吞吐（只读前 2 MB），结果按 URL 缓存；与 `probe_url` 分工：后者测延迟（小文件选源），本函数测带宽（JDK runtime 等大包）。
pub(crate) async fn probe_speed(url: &str) -> SpeedProbeResult {
    if let Some(cached) = cache_get(speed_cache(), url) {
        return cached;
    }
    let res = probe_speed_uncached(url).await;
    cache_set(speed_cache(), url.to_string(), res);
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

/// 探测执行体（无缓存）：3s 双保险超时，返回 TTFB/可达性。
async fn probe_url_uncached(url: &str) -> ProbeResult {
    let t0 = Instant::now();
    let outcome = async {
        let resp = http()
            .head(url)
            .timeout(Duration::from_secs(PROBE_TIMEOUT_SECS))
            .send()
            .await
            .map_err(common_err_ctx!("请求失败"))?;
        if !resp.status().is_success() {
            bail!("请求失败\nHTTP {}", resp.status());
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

/// 速度探测执行体（无缓存）：流式读取前 `SPEED_PROBE_BYTES` 字节计算吞吐。
async fn probe_speed_uncached(url: &str) -> SpeedProbeResult {
    let client = http();
    let timeout = Duration::from_secs(SPEED_PROBE_TIMEOUT_SECS);
    let result = tokio::time::timeout(timeout, async {
        let resp = match client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return None,
        };
        if let Some(len) = resp.content_length() {
            if len > 0 && len < SPEED_PROBE_BYTES / 2 {
                return None;
            }
        }
        let mut reader = resp.bytes_stream();
        let mut bytes_read = 0u64;
        let start = Instant::now();
        while let Some(chunk) = reader.next().await {
            match chunk {
                Ok(chunk) if bytes_read < SPEED_PROBE_BYTES => bytes_read += chunk.len() as u64,
                _ => break,
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        let speed_bps = if elapsed == 0.0 {
            u64::MAX
        } else {
            (bytes_read as f64 / elapsed) as u64
        };
        Some(SpeedProbeResult {
            ok: true,
            speed_bps,
        })
    })
    .await;
    result.ok().flatten().unwrap_or(SpeedProbeResult {
        ok: false,
        speed_bps: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_cache_roundtrip() {
        let url = "https://example.com/probe-cache-test";
        probe_cache().lock().remove(url);
        let res = ProbeResult {
            ok: true,
            ttfb_ms: 42,
        };
        cache_set(probe_cache(), url.to_string(), res);
        assert_eq!(cache_get(probe_cache(), url), Some(res));
        assert_eq!(cache_get(probe_cache(), url).unwrap().ttfb_ms, 42);
        probe_cache().lock().remove(url);
    }

    #[test]
    fn speed_cache_is_separate_from_probe_cache() {
        let url = "https://example.com/speed-cache-test";
        speed_cache().lock().remove(url);
        probe_cache().lock().remove(url);
        let speed = SpeedProbeResult {
            ok: true,
            speed_bps: 1_048_576,
        };
        cache_set(speed_cache(), url.to_string(), speed);
        assert_eq!(cache_get(speed_cache(), url), Some(speed));
        assert_eq!(cache_get(probe_cache(), url), None, "两类缓存的键空间独立");
        speed_cache().lock().remove(url);
    }
}
