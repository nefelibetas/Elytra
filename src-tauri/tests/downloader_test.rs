use elytra_lib::config::downloader::model::DownloaderConfig;
use elytra_lib::downloader::manifest;

fn test_config() -> DownloaderConfig {
    let mut cfg = DownloaderConfig::default();
    cfg.file_source_mode = elytra_lib::config::downloader::model::DownloadSourceMode::PreferOfficial;
    cfg.minecraft_dir = r"D:\download_test".into();
    cfg
}

/// 真实下载最新快照版本到 `D:\download_test`（数百 MB）。
/// 默认跳过：日常验证用 `cargo test --lib`；需要时显式 `cargo test --test downloader_test -- --ignored`。
#[tokio::test]
#[ignore = "真实下载数百 MB，默认不跑（AGENTS.md 测试红线）"]
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

/// 版本清单拉取（单发小请求，需网络；离线环境会失败）。
#[tokio::test]
async fn fetch_manifest_works() {
    let config = test_config();
    let result = manifest::fetch_version_list(&config).await;
    assert!(result.is_ok());
    let m = result.unwrap();
    assert!(!m.versions.is_empty());
    assert!(m.versions.iter().any(|v| v.r#type == "release"));
}
