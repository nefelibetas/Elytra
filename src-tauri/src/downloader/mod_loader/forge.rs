// ── Installer 式安装（Forge / NeoForge）：官方 installer.jar 下载 + ZIP 解析 ──

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher::{self, http};

use super::{LoaderEvent, LoaderHook, expand_inheritance};

#[allow(clippy::too_many_arguments)]
pub async fn install_forge(
    mc_version: &str,
    forge_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &Arc<AtomicU64>,
    lib_bytes: &Arc<AtomicU64>,
    lib_total: &Arc<AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-forge-{}", mc_version, forge_version);
    let vdir = mc.join("versions").join(&dir_name);
    let url = source.url(
        &source.forge_installer_url,
        &[("mc", mc_version), ("ver", forge_version)],
    );
    download_and_extract(
        &url,
        source,
        config,
        &vdir,
        &dir_name,
        parent_text,
        bytes,
        lib_bytes,
        lib_total,
        hook,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn install_neoforge(
    mc_version: &str,
    neo_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &Arc<AtomicU64>,
    lib_bytes: &Arc<AtomicU64>,
    lib_total: &Arc<AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-neoforge-{}", mc_version, neo_version);
    let vdir = mc.join("versions").join(&dir_name);
    let url = source.url(&source.neoforge_installer_url, &[("ver", neo_version)]);
    download_and_extract(
        &url,
        source,
        config,
        &vdir,
        &dir_name,
        parent_text,
        bytes,
        lib_bytes,
        lib_total,
        hook,
    )
    .await
}

/// 10 参数均为必需数据（URL/源/配置/目标/字节计数/阶段钩子），私有实现细节，不抽象。
#[allow(clippy::too_many_arguments)]
async fn download_and_extract(
    installer_url: &str,
    source: &DownloadSource,
    config: &DownloaderConfig,
    version_dir: &Path,
    dir_name: &str,
    parent_text: &str,
    bytes: &Arc<AtomicU64>,
    lib_bytes: &Arc<AtomicU64>,
    lib_total: &Arc<AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let mc_dir = config.minecraft_dir_abs();
    let tmp = mc_dir.join(".elytra").join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let installer_name = installer_url.rsplit('/').next().unwrap_or("installer.jar");
    let installer_path = tmp.join(installer_name);
    // 字节级进度：增量累加进外部计数器（收尾心跳显示速度）；同时写入 LoaderLibs 步骤计数
    let file_done = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let progress = {
        let fd = file_done.clone();
        let b = bytes.clone();
        let lb = lib_bytes.clone();
        let lt = lib_total.clone();
        move |downloaded: u64, total: u64| {
            let prev = fd.swap(downloaded, std::sync::atomic::Ordering::Relaxed);
            let delta = downloaded.saturating_sub(prev);
            b.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
            lb.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
            if total > 0 {
                lt.store(total, std::sync::atomic::Ordering::Relaxed);
            }
        }
    };
    fetcher::fetch(installer_url, &installer_path, Some(progress))
        .await
        .map_err(|e| format!("下载安装程序失败: {}", e))?;

    let file =
        std::fs::File::open(&installer_path).map_err(|e| format!("打开安装程序失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取安装程序失败: {}", e))?;

    std::fs::create_dir_all(version_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let jar_path = version_dir.join(format!("{}.jar", dir_name));
    let json_path = version_dir.join(format!("{}.json", dir_name));

    // Parse install_profile.json if present
    let profile: Option<serde_json::Value> = (0..archive.len()).find_map(|i| {
        let mut entry = archive.by_index(i).ok()?;
        if !entry.name().ends_with("install_profile.json") {
            return None;
        }
        let mut buf = String::new();
        entry.read_to_string(&mut buf).ok()?;
        serde_json::from_str(&buf).ok()
    });

    // --- version.json ---
    let found_json =
        resolve_version_json(&mut archive, &profile, &json_path, dir_name, parent_text).await?;

    // --- jar ---
    let found_jar = resolve_jar(&mut archive, &profile, source, &jar_path, dir_name).await?;

    let _ = std::fs::remove_file(&installer_path);
    if !found_json {
        return Err("未找到 version.json".into());
    }
    if let Some(h) = hook {
        h(LoaderEvent::ConfigDone);
    }
    if !found_jar {
        return Err("未找到 client.jar".into());
    }
    if let Some(h) = hook {
        h(LoaderEvent::LibsDone);
    }
    Ok(())
}

/// Resolve version.json from ZIP install_profile.json metadata or by scanning.
/// 提取后展开 inheritsFrom（Forge/NeoForge 的 JSON 只含自身库 + 父指针，须合并原版父 JSON）。
async fn resolve_version_json(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &Option<serde_json::Value>,
    json_path: &Path,
    dir_name: &str,
    parent_text: &str,
) -> Result<bool, String> {
    // 提取 → 展开继承 → 序列化 → 写盘；三个策略共用
    let write_extracted = |v: serde_json::Value| -> Result<(), String> {
        let merged = expand_inheritance(parent_text, v, dir_name)?;
        let out =
            serde_json::to_string_pretty(&merged).map_err(|e| format!("序列化失败: {}", e))?;
        std::fs::write(json_path, &out).map_err(|e| format!("写入 version.json 失败: {}", e))
    };

    // Strategy 1: install_profile has "json" field → extract from ZIP
    if let Some(p) = profile {
        if let Some(json_path_in_zip) = p["json"].as_str() {
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| format!("读取安装程序条目失败: {}", e))?;
                if entry.name() == json_path_in_zip.trim_start_matches('/') {
                    let mut buf = String::new();
                    entry
                        .read_to_string(&mut buf)
                        .map_err(|e| format!("读取 JSON 失败: {}", e))?;
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&buf) {
                        write_extracted(v)?;
                        return Ok(true);
                    }
                }
            }
        }

        // Strategy 2: install_profile has "versionInfo" → serialize that
        if let Some(vi) = p["versionInfo"].as_object() {
            let v = serde_json::Value::Object(vi.clone());
            write_extracted(v)?;
            return Ok(true);
        }
    }

    // Strategy 3: scan for any entry ending in version.json
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取安装程序条目失败: {}", e))?;
        let name = entry.name().to_string();
        if name.ends_with("version.json") {
            let mut buf = String::new();
            entry
                .read_to_string(&mut buf)
                .map_err(|e| format!("读取 JSON 失败: {}", e))?;
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&buf) {
                write_extracted(v)?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// 把 ZIP 条目解压到目标文件。
fn extract_entry_to(entry: &mut zip::read::ZipFile, dest: &Path) -> Result<(), String> {
    let mut out = std::fs::File::create(dest).map_err(|e| format!("创建文件失败: {}", e))?;
    std::io::copy(entry, &mut out).map_err(|e| format!("解压失败: {}", e))?;
    Ok(())
}

/// Resolve client jar from ZIP, maven download, or fallback to original manifest.
async fn resolve_jar(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &Option<serde_json::Value>,
    source: &DownloadSource,
    jar_path: &Path,
    dir_name: &str,
) -> Result<bool, String> {
    // Strategy 1: install_profile → install.filePath → extract from ZIP
    if let Some(p) = profile {
        if let Some(fp) = p.pointer("/install/filePath").and_then(|v| v.as_str()) {
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| format!("读取安装程序条目失败: {}", e))?;
                if entry.name() == fp {
                    extract_entry_to(&mut entry, jar_path)?;
                    return Ok(true);
                }
            }
        }
    }

    // Strategy 2: install_profile → path (maven path) → download from maven
    if let Some(p) = profile {
        if let Some(maven_path) = p["path"].as_str() {
            let jar_url = format!("{}/{}", source.maven_url.trim_end_matches('/'), maven_path);
            return Ok(fetcher::fetch(&jar_url, jar_path, None::<fn(u64, u64)>)
                .await
                .is_ok());
        }
    }

    // Strategy 3: scan ZIP for any .jar entry (exclude installer/sources/javadoc)
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取安装程序条目失败: {}", e))?;
        let name = entry.name().to_string();
        if name.ends_with(".jar")
            && !name.contains("installer")
            && !name.contains("sources")
            && !name.contains("javadoc")
        {
            extract_entry_to(&mut entry, jar_path)?;
            return Ok(true);
        }
    }

    // Strategy 4: download client.jar from original version manifest
    let manifest_url = format!("{}/mc/game/version_manifest_v2.json", source.manifest_url);
    if let Ok(resp) = http().get(&manifest_url).send().await {
        if let Ok(text) = resp.text().await {
            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(versions) = manifest["versions"].as_array() {
                    // ponytail: mc_ver 由目录名连字符首段猜测，版本号含连字符时会错；应传 mc_version
                    let mc_ver = dir_name.split('-').next().unwrap_or(dir_name);
                    let entry = versions.iter().find(|v| v["id"].as_str() == Some(mc_ver));
                    if let Some(entry) = entry {
                        if let Some(url) = entry["url"].as_str() {
                            let url = source.mirror_url(url);
                            if let Ok(resp) = http().get(&url).send().await {
                                if let Ok(vj) = resp.text().await {
                                    if let Ok(vj_val) =
                                        serde_json::from_str::<serde_json::Value>(&vj)
                                    {
                                        let dl_url = vj_val
                                            .pointer("/downloads/client/url")
                                            .and_then(|u| u.as_str())
                                            .map(|u| source.mirror_url(u));
                                        if let Some(dl_url) = dl_url {
                                            return Ok(fetcher::fetch(
                                                &dl_url,
                                                jar_path,
                                                None::<fn(u64, u64)>,
                                            )
                                            .await
                                            .is_ok());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(false)
}
