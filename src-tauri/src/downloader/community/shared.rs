//! 两平台共用部分：JSON 交互请求通道与 MC 版本号解析/比较助手。

use crate::downloader::fetcher;

/// 搜索类交互请求超时（默认 client 300s 太长，搜索卡住不该让前端挂 5 分钟）
pub(super) const QUERY_TIMEOUT_SECS: u64 = 10;

/// JSON 交互请求（10s 超时）：直接 GET 解析，不做文件下载层的重试/换源（搜索失败即返回）。
/// pub(crate)：本地 Mod 识别（mod_updates）复用同一查询通道，避免两份副本漂移。
pub(crate) async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let resp = fetcher::http()
        .get(url)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("请求失败\n{}", e))?;
    if !resp.status().is_success() {
        return Err(format!("请求失败\nHTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败\n{}", e))
}

/// 解析 MC 版本号为数值段（"1.21.1" → [1,21,1]）；含非数字段（snapshot 如 "24w14a"）返回 None。
fn parse_game_version(v: &str) -> Option<Vec<u64>> {
    v.split('.').map(|seg| seg.parse::<u64>().ok()).collect()
}

/// 数值段逐段比较；段数不足按 0 补足（"1.20" 与 "1.20.0" 相等）；任一侧无法解析返回 None。
pub(super) fn compare_game_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let pa = parse_game_version(a)?;
    let pb = parse_game_version(b)?;
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return Some(x.cmp(&y));
        }
    }
    Some(std::cmp::Ordering::Equal)
}

/// 版本列表 → 范围字符串：仅一个有效版本显示单个，多个显示 "最低 ~ 最高"；无有效版本返回 None。
pub(super) fn version_range(versions: &[String]) -> Option<String> {
    let valid: Vec<&String> = versions
        .iter()
        .filter(|v| parse_game_version(v).is_some())
        .collect();
    if valid.is_empty() {
        return None;
    }
    // valid 已通过 parse_game_version 过滤，比较必然可解析；仍用 unwrap_or 兜底防 panic
    let min = valid
        .iter()
        .copied()
        .min_by(|a, b| compare_game_versions(a, b).unwrap_or(std::cmp::Ordering::Equal))?;
    let max = valid
        .iter()
        .copied()
        .max_by(|a, b| compare_game_versions(a, b).unwrap_or(std::cmp::Ordering::Equal))?;
    if min == max {
        Some(min.clone())
    } else {
        Some(format!("{} ~ {}", min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 版本比较：数值段逐段比较，段数不足补 0，非法版本（snapshot）返回 None。
    #[test]
    fn compare_game_versions_numeric_semantics() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_game_versions("1.21.1", "1.20.1"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_game_versions("1.9.4", "1.21.1"),
            Some(Ordering::Less),
            "按数值而非字符串比较"
        );
        assert_eq!(
            compare_game_versions("1.20", "1.20.0"),
            Some(Ordering::Equal),
            "段数不足补 0"
        );
        assert_eq!(
            compare_game_versions("24w14a", "1.21.1"),
            None,
            "snapshot 无法解析"
        );
        assert_eq!(
            compare_game_versions("1.21.1", "1.21.1"),
            Some(Ordering::Equal)
        );
    }
}
