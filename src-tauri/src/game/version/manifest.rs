use crate::config::downloader::DownloaderConfig;
use crate::game::version::model::VersionManifest;
use crate::game::version::source::CacheBehaviour;

pub async fn fetch_version_list(config: &DownloaderConfig) -> Result<VersionManifest, String> {
    config
        .fetch_json_with_fallback::<VersionManifest>(
            config.game_source_mode,
            CacheBehaviour::DefaultTtl,
            |source| format!("{}/mc/game/version_manifest_v2.json", source.manifest_url),
        )
        .await
}
