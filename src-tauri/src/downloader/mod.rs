pub mod assets;
pub mod fetcher;
pub mod hash;
pub mod library;
pub mod manifest;
pub mod model;
pub mod version;

use std::path::Path;

use crate::config::downloader::model::DownloaderConfig;

fn resolve_version_dir(mc: &Path, version_id: &str) -> (String, std::path::PathBuf) {
    let base = mc.join("versions").join(version_id);
    if !base.exists() {
        return (version_id.to_string(), base);
    }
    for n in 1.. {
        let name = format!("{}_{}", version_id, n);
        let dir = mc.join("versions").join(&name);
        if !dir.exists() {
            return (name, dir);
        }
    }
    unreachable!()
}

pub async fn download_version(
    version_id: &str,
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<(), Vec<String>> {
    let mc = Path::new(&config.minecraft_dir);
    let (_folder_name, vdir) = resolve_version_dir(mc, version_id);

    let vj = version::download_version_json(version_url, &vdir)
        .await
        .map_err(|e| vec![e])?;

    let jar = async {
        match vj.downloads.client.as_ref() {
            Some(c) => version::download_client_jar(&c.url, version_id, &vdir)
                .await
                .map_err(|e| vec![e]),
            None => Ok(()),
        }
    };

    let lib_dir = mc.join("libraries");
    let libs = library::download_libraries(&vj.libraries, &lib_dir, config.concurrency as usize);

    let asst = assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        Some(&vj.asset_index.sha1),
        config,
        mc,
    );

    tokio::try_join!(jar, libs, asst)?;
    Ok(())
}
