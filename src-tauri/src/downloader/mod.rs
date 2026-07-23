pub mod fetcher;
pub mod library;
pub mod manifest;
pub mod model;
pub mod version;

use std::path::Path;

use crate::config::downloader::model::DownloaderConfig;

pub async fn download_version(
    version_id: &str,
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<(), Vec<String>> {
    let minecraft_dir = Path::new(&config.minecraft_dir);
    let version_dir = minecraft_dir.join("versions").join(version_id);

    let version_json = version::download_version_json(version_url, &version_dir)
        .await
        .map_err(|e| vec![e])?;

    if let Some(ref client) = version_json.downloads.client {
        version::download_client_jar(&client.url, version_id, &version_dir)
            .await
            .map_err(|e| vec![e])?;
    }

    let libraries_dir = minecraft_dir.join("libraries");
    library::download_libraries(
        &version_json.libraries,
        &libraries_dir,
        config.concurrency as usize,
    )
    .await?;

    Ok(())
}
