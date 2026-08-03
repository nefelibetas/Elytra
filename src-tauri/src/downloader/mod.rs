pub mod assets;
pub mod extract;
pub mod fetcher;
pub mod hash;
pub mod installer;
pub mod library;
pub mod manifest;
pub mod model;
pub mod version;

use std::path::Path;    

use crate::config::downloader::model::DownloaderConfig;

pub fn resolve_version_dir(mc: &Path, version_id: &str) -> (String, std::path::PathBuf) {
    let base = mc.join("versions").join(version_id);
    if !base.exists() {
        return (version_id.to_string(), base);
    }
    // 目录存在但不完整（缺 client jar，如上次下载中断）→ 复用目录重新下载，避免残留残缺版本
    if !base.join(format!("{version_id}.jar")).exists() {
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
    let sources = config.file_sources_list();
    let mc = Path::new(&config.minecraft_dir);
    let (_folder_name, vdir) = resolve_version_dir(mc, version_id);

    let vj = version::download_version_json(version_url, &vdir, &sources)
        .await
        .map_err(|e| vec![e])?;

    let jar = async {
        match vj.downloads.client.as_ref() {
            Some(c) => version::download_client_jar(&c.url, version_id, &vdir, &sources)
                .await
                .map_err(|e| vec![e]),
            None => Ok(()),
        }
    };

    let lib_dir = mc.join("libraries");
    let libs = library::download_libraries(
        &vj.libraries,
        &lib_dir,
        config.concurrency as usize,
        &sources,
        None,
        None,
    );

    let asst = assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        Some(&vj.asset_index.sha1),
        config,
        mc,
        None,
        None,
    );

    tokio::try_join!(jar, libs, asst)?;

    extract::process_natives(&vj.libraries, &lib_dir, &sources, None, None).await?;

    Ok(())
}
