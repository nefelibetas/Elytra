use std::path::Path;
use std::path::PathBuf;

use elytra_launcher_lib::downloader::library::download_libraries;
use elytra_launcher_lib::downloader::manifest;
use elytra_launcher_lib::downloader::version::download_version_json;

#[tokio::test]
async fn download_snapshot_version() {
    let manifest = manifest::fetch_version_list("https://piston-meta.mojang.com")
        .await
        .unwrap();
    let snapshot_id = &manifest.latest.snapshot;

    let version_meta = manifest
        .versions
        .iter()
        .find(|v| &v.id == snapshot_id)
        .expect("snapshot not found");
    let version_dir = format!("{}", snapshot_id);
    let dest_dir = Path::new(r"D:\download_test\version\").join(version_dir);
    let version_json = download_version_json(&version_meta.url, &dest_dir)
        .await
        .unwrap();

    let lib_dir = PathBuf::from(r"D:\download_test\libraries");
    download_libraries(&version_json.libraries, &lib_dir, 8)
        .await
        .unwrap();

    println!(
        "downloaded {} with {} libraries",
        snapshot_id,
        version_json.libraries.len()
    );
}
