use elytra_lib::config::downloader::model::DownloaderConfig;
use elytra_lib::downloader::manifest;

fn test_config() -> DownloaderConfig {
    DownloaderConfig {
        sources: vec![elytra_lib::config::downloader::model::DownloadSource {
            name: "test".into(),
            manifest_url: "https://piston-meta.mojang.com".into(),
            resource_url: "https://resources.download.minecraft.net".into(),
        }],
        active_source: "0".into(),
        concurrency: 8,
        minecraft_dir: r"D:\download_test".into(),
    }
}

#[tokio::test]
async fn download_snapshot_full() {
    let config = test_config();
    let m = manifest::fetch_version_list(&config).await.unwrap();
    let sid = &m.latest.snapshot;
    let meta = m.versions.iter().find(|v| &v.id == sid).unwrap();

    elytra_lib::downloader::download_version(sid, &meta.url, &config)
        .await
        .unwrap();

    println!("downloaded {} complete", sid);
}

#[tokio::test]
async fn fetch_manifest_works() {
    let config = test_config();
    let result = manifest::fetch_version_list(&config).await;
    assert!(result.is_ok());
    let m = result.unwrap();
    assert!(!m.versions.is_empty());
    assert!(m.versions.iter().any(|v| v.r#type == "release"));
}
