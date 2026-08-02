use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::model::VersionManifest;

pub async fn fetch_version_list(config: &DownloaderConfig) -> Result<VersionManifest, String> {
    config
        .fetch_json_with_fallback::<VersionManifest>(|source| {
            format!("{}/mc/game/version_manifest_v2.json", source.manifest_url)
        })
        .await
}
