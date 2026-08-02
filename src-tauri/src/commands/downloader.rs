use std::path::Path;

use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::manifest;
use crate::downloader::model::VersionManifest;
use crate::downloader::resolve_version_dir;
use crate::persist;

#[tauri::command]
pub async fn get_version_list() -> Result<VersionManifest, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    manifest::fetch_version_list(&config).await
}

#[tauri::command]
pub async fn download_version(version_id: String, version_url: String) -> Result<(), String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    crate::downloader::download_version(&version_id, &version_url, &config)
        .await
        .map_err(|errors| errors.join("\n"))
}

/// 修复缺失的游戏文件（client.jar / 支持库 / 资源 / natives）。
/// 基于本地 version json，只补缺失部分；完成后可重新启动。
#[tauri::command]
pub async fn repair_version(version_id: String) -> Result<String, String> {
    use std::path::Path;
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let sources = config.file_sources_list();
    let mc = Path::new(&config.minecraft_dir);
    let vdir = mc.join("versions").join(&version_id);
    let json_path = vdir.join(format!("{version_id}.json"));
    let json_text = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("读取版本文件失败：{e}"))?;
    let vj: crate::downloader::model::VersionJson =
        serde_json::from_str(&json_text).map_err(|e| format!("解析版本文件失败：{e}"))?;

    // 1. client.jar（缺失才下载，带 sha1 校验）
    let jar_path = vdir.join(format!("{version_id}.jar"));
    if !jar_path.exists() {
        if let Some(c) = &vj.downloads.client {
            crate::downloader::version::download_client_jar(&c.url, &version_id, &vdir, &sources)
                .await
                .map_err(|e| format!("下载客户端失败：{e}"))?;
            if !c.sha1.is_empty()
                && !crate::downloader::hash::file_sha1_matches(&jar_path, &c.sha1)
            {
                let _ = std::fs::remove_file(&jar_path);
                return Err("客户端文件校验失败（sha1 不匹配）".to_string());
            }
        }
    }

    // 2-4. 支持库 / 资源 / natives（均自带跳过已存在文件，幂等）
    let lib_dir = mc.join("libraries");
    crate::downloader::library::download_libraries(
        &vj.libraries,
        &lib_dir,
        config.concurrency as usize,
        &sources,
        None,
        None,
    )
    .await
    .map_err(|e| e.join("\n"))?;
    crate::downloader::assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        Some(&vj.asset_index.sha1),
        &config,
        mc,
        None,
        None,
    )
    .await
    .map_err(|e| e.join("\n"))?;
    crate::downloader::extract::process_natives(&vj.libraries, &lib_dir, &sources, None, None)
        .await
        .map_err(|e| e.join("\n"))?;

    Ok("修复完成".to_string())
}

#[tauri::command]
pub fn get_downloader_config() -> DownloaderConfig {
    persist::read("downloader").unwrap_or_default()
}

#[tauri::command]
pub fn save_downloader_config(config: DownloaderConfig) -> Result<(), String> {
    persist::save("downloader", &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_installed_versions() -> Result<Vec<String>, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let versions_dir = Path::new(&config.minecraft_dir).join("versions");
    if !versions_dir.exists() {
        return Ok(vec![]);
    }
    let mut items: Vec<String> = std::fs::read_dir(&versions_dir)
        .map_err(|e| format!("读取版本目录失败: {}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                Some(entry.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    items.sort();
    Ok(items)
}

#[tauri::command]
pub fn resolve_version_name(base_name: String) -> Result<String, String> {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let mc = Path::new(&config.minecraft_dir);
    let (name, _) = resolve_version_dir(mc, &base_name);
    Ok(name)
}
