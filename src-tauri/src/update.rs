//! 启动器自身更新检查：只读 GitHub Releases 的 latest，返回版本与下载页；不下载、不执行。

use crate::constants::APP_VERSION;

const REPO: &str = "nefelibetas/ElytraLauncher";

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateInfo {
    pub current_version: &'static str,
    pub latest_version: String,
    pub update_available: bool,
    pub url: String,
    pub notes: String,
    pub published_at: Option<String>,
}

/// 查询最新正式 Release。无已发布版本时返回 `None`；限流/网络/解析错误返回可本地化错误。
pub async fn check() -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let response = crate::downloader::fetcher::http()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(common_err_ctx_key!("errors.update.failed", "检查更新失败"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if matches!(
        response.status(),
        reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::TOO_MANY_REQUESTS
    ) {
        bail_key!(
            "errors.update.rateLimited",
            "检查更新失败\nGitHub API 限流，请稍后重试"
        );
    }
    let response = response
        .error_for_status()
        .map_err(common_err_ctx_key!("errors.update.failed", "检查更新失败"))?;
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(common_err_ctx_key!("errors.update.failed", "检查更新失败"))?;

    // latest 端点通常已过滤，防御性跳过草稿/预发布。
    if json.get("draft").and_then(|v| v.as_bool()).unwrap_or(false)
        || json
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    {
        return Ok(None);
    }
    let Some(tag) = json.get("tag_name").and_then(|v| v.as_str()) else {
        bail_key!("errors.update.failed", "检查更新失败\n发布信息缺少版本号");
    };
    let Some(page_url) = json.get("html_url").and_then(|v| v.as_str()) else {
        bail_key!("errors.update.failed", "检查更新失败\n发布信息缺少下载地址");
    };

    let latest_version = normalize_version(tag);
    Ok(Some(UpdateInfo {
        current_version: APP_VERSION,
        update_available: is_newer(&latest_version, APP_VERSION),
        latest_version,
        url: page_url.to_string(),
        notes: json
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        published_at: json
            .get("published_at")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }))
}

fn normalize_version(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

/// 仅比较 `major.minor.patch`；预发布/构建后缀不参与（latest 端点本就只返回正式版）。
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let core = normalize_version(raw);
    let core = core.split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order_is_numeric() {
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(is_newer("1.2.0", "1.1.99"));
        assert!(!is_newer("1.0.2", "1.0.2"));
        assert!(!is_newer("1.0.1", "1.0.2"));
        assert!(!is_newer("nightly", "1.0.2"));
    }

    #[test]
    fn version_tags_accept_v_prefix_and_suffix() {
        assert_eq!(parse_version("v1.0.2"), Some((1, 0, 2)));
        assert_eq!(parse_version("1.0.2-beta.1"), Some((1, 0, 2)));
        assert_eq!(parse_version("1.0"), None);
        assert_eq!(parse_version("1.0.2.3"), None);
    }
}
