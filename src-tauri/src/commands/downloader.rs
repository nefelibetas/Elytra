use crate::config::downloader::model::DownloaderConfig;
use crate::constants;
use crate::downloader::manifest;
use crate::downloader::model::VersionManifest;
use crate::downloader::resolve_version_dir;
use crate::persist;
use crate::downloader::fetcher;

#[tauri::command]
pub async fn get_version_list() -> Result<VersionManifest, String> {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    manifest::fetch_version_list(&config).await
}

#[tauri::command]
pub fn get_downloader_config() -> DownloaderConfig {
    persist::read(constants::SECT_DOWNLOADER).unwrap_or_default()
}

#[tauri::command]
pub fn save_downloader_config(config: DownloaderConfig) -> Result<(), String> {
    let old: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    persist::save(constants::SECT_DOWNLOADER, &config).map_err(|e| format!("保存配置失败\n{e}"))?;
    // 网络代理模式（Auto/System/Direct）变化 → 重建全局 http client：
    // client 的代理配置在构建时固定，LazyLock 单例不重建则切换不生效
    // （探测与下载全链路共用 http()，行为必须跟随用户设置）。
    // 全局限速同步（0 = 不限）
    fetcher::set_speed_limit(config.max_speed);
    if old.network_mode != config.network_mode {
        fetcher::reset_http_client();
    }
    Ok(())
}

/// 已安装版本信息（get_installed_versions 返回项）。
#[derive(serde::Serialize)]
pub struct InstalledVersionInfo {
    pub name: String,
    /// 从版本 json 推断的 loader（fabric/forge/neoforge/optifine/vanilla）；
    /// json 读取失败（非版本目录）时为空串
    pub loader: String,
}

#[tauri::command]
pub fn get_installed_versions() -> Result<Vec<InstalledVersionInfo>, String> {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let versions_dir = config.minecraft_dir_abs().join("versions");
    if !versions_dir.exists() {
        return Ok(vec![]);
    }
    let mut items: Vec<InstalledVersionInfo> = std::fs::read_dir(&versions_dir)
        .map_err(|e| format!("读取版本目录失败\n{}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // 从版本 json 推断 loader（PCL2 实例目录名可能无 loader 标记）
            let loader = crate::launcher::args::find_version_json(&entry.path(), &name)
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| crate::util::infer_loader_from_json(&t))
                .unwrap_or_default();
            Some(InstalledVersionInfo { name, loader })
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

#[tauri::command]
pub fn resolve_version_name(base_name: String) -> Result<String, String> {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let mc = config.minecraft_dir_abs();
    let (name, _) = resolve_version_dir(&mc, &base_name);
    Ok(name)
}
