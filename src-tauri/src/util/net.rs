//! 社区资源下载多源候选构造（官方 CDN / 镜像）。

use crate::config::downloader::{DownloadSourceMode, DownloaderConfig};
use crate::downloader;
use crate::persist;

/// 社区文件下载候选：Modrinth 官方 CDN → MCIM 换域名镜像。按 community_source_mode：OfficialOnly
/// 仅官方 / MirrorOnly 仅镜像 / Auto 探测两源 TTFB（TTL 10min）快者优先、慢者兜底；非 modrinth CDN 仅官方一源。
pub async fn community_download_urls(url: &str) -> Vec<String> {
    let official = url.to_string();
    let mirrored = official.replace(
        downloader::endpoints::CommunityEndpoints::MODRINTH_CDN_BASE,
        downloader::endpoints::CommunityEndpoints::MODRINTH_CDN_MIRROR_BASE,
    );
    if mirrored == official {
        return vec![official];
    }
    let cfg: DownloaderConfig = persist::downloader_config();
    match cfg.community_source_mode {
        DownloadSourceMode::OfficialOnly => vec![official],
        DownloadSourceMode::MirrorOnly => vec![mirrored],
        DownloadSourceMode::Auto => {
            let o = downloader::probe::probe_url(&official).await;
            let m = downloader::probe::probe_url(&mirrored).await;
            if prefer_mirror(m.ok, m.ttfb_ms, o.ttfb_ms) {
                vec![mirrored, official]
            } else {
                vec![official, mirrored]
            }
        }
    }
}

/// 镜像优先决策（纯逻辑）：镜像探测可用且快于官方 → 镜像优先；失败/更慢 → 官方（现状默认）。
fn prefer_mirror(mirror_ok: bool, mirror_ttfb_ms: u64, official_ttfb_ms: u64) -> bool {
    mirror_ok && mirror_ttfb_ms < official_ttfb_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 社区文件源排序决策：镜像快于官方 → 镜像优先；镜像失败/更慢 → 官方优先。
    #[test]
    fn prefer_mirror_ttfb_decision() {
        // 镜像更快 → 镜像优先
        assert!(prefer_mirror(true, 80, 400));
        // 镜像探测失败 → 官方（镜像候选保留作兜底，仅排序不剔除）
        assert!(!prefer_mirror(false, 0, 400));
        // 镜像更慢 → 官方
        assert!(!prefer_mirror(true, 500, 100));
    }
}
