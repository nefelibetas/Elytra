//! 两平台共用部分：社区 API 路由（官方/MCIM 多候选）、多候选 JSON 请求、
//! MC 版本号解析/比较助手。

use crate::config::downloader::model::{DownloadSourceMode, DownloaderConfig};
use crate::constants;
/// 搜索类交互请求超时（默认 client 300s 太长，搜索卡住不该让前端挂 5 分钟）
pub(super) const QUERY_TIMEOUT_SECS: u64 = 10;

/// 社区 API 平台（官方与 MCIM 镜像的路径映射规则不同）。
#[derive(Debug, Clone, Copy)]
pub(crate) enum ApiPlatform {
    Modrinth,
    CurseForge,
}

/// 社区 API 多源候选：按 community_source_mode 在官方与 MCIM 镜像间路由。
/// 镜像映射（docs.mcimirror.top，"可直接替换 URL"）：
///   api.modrinth.com   → mod.mcimirror.top/modrinth
///   api.curseforge.com → mod.mcimirror.top/curseforge（/v1 前缀保留）
/// Auto = 官方优先 + 镜像备用；ChinaOnly 仅镜像；OfficialOnly 仅官方。
pub(crate) fn api_routes(platform: ApiPlatform, path: &str) -> Vec<String> {
    let (official, mirror) = match platform {
        ApiPlatform::Modrinth => (
            format!("https://api.modrinth.com{path}"),
            format!("https://mod.mcimirror.top/modrinth{path}"),
        ),
        ApiPlatform::CurseForge => (
            format!("https://api.curseforge.com/v1{path}"),
            format!("https://mod.mcimirror.top/curseforge/v1{path}"),
        ),
    };
    let cfg: DownloaderConfig =
        crate::persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    match cfg.community_source_mode {
        DownloadSourceMode::OfficialOnly => vec![official],
        DownloadSourceMode::ChinaOnly => vec![mirror],
        DownloadSourceMode::Auto => vec![official, mirror],
    }
}

/// 多候选 JSON GET：逐源尝试；双候选时主源 5s 短超时快速降级镜像；
/// 404 属确定性未命中不换源。`build` 在给定 URL 上追加查询参数/鉴权头等。
pub(crate) async fn get_json_multi<F>(
    urls: &[String],
    mut build: F,
) -> Result<serde_json::Value, String>
where
    F: FnMut(&str) -> reqwest::RequestBuilder,
{
    let mut last_err = String::new();
    for (i, url) in urls.iter().enumerate() {
        let resp = match build(url)
            .timeout(std::time::Duration::from_secs(if i == 0 && urls.len() > 1 {
                // 主源过慢（5s）即降级镜像，对齐 version_query 自适应语义
                5
            } else {
                QUERY_TIMEOUT_SECS
            }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("请求失败\n{}", e);
                continue;
            }
        };
        // 404 属确定性未命中（镜像同样不会有），不换源
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(format!("请求失败\nHTTP {}", resp.status()));
        }
        if !resp.status().is_success() {
            last_err = format!("请求失败\nHTTP {}", resp.status());
            continue;
        }
        return resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("解析 JSON 失败\n{}", e));
    }
    Err(last_err)
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
