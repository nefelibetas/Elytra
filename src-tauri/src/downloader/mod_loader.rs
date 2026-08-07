use std::io::Read;
use std::path::{Path, PathBuf};

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher::{self, http};

fn maven_path(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return format!("{}.jar", name.replace(':', "_"));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    format!(
        "{}/{}/{}/{}-{}.jar",
        group, artifact, version, artifact, version
    )
}

/// 纯内存合并：原版 JSON 文本 + loader profile → 合并后的版本 JSON。
/// 追加 jvm/game 参数、替换 mainClass、maven 库转 downloads.artifact、删 inheritsFrom。
fn merge_loader_profile(
    parent_text: &str,
    profile: &serde_json::Value,
    dir_name: &str,
    maven_url: &str,
) -> Result<serde_json::Value, String> {
    let mut merged: serde_json::Value =
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败: {}", e))?;

    for key in ["jvm", "game"] {
        let loader_args = profile["arguments"][key].as_array();
        if let Some(loader_args) = loader_args {
            if !loader_args.is_empty() {
                if let Some(args) = merged["arguments"][key].as_array_mut() {
                    args.extend(loader_args.iter().cloned());
                } else {
                    merged["arguments"][key] = serde_json::Value::Array(loader_args.clone());
                }
            }
        }
    }

    merged["id"] = serde_json::Value::String(dir_name.to_string());
    if let Some(mc) = profile["mainClass"].as_str() {
        merged["mainClass"] = serde_json::Value::String(mc.to_string());
    }

    if let Some(loader_libs) = profile["libraries"].as_array() {
        if let Some(libs) = merged["libraries"].as_array_mut() {
            for lib in loader_libs {
                let name = match lib["name"].as_str() {
                    Some(n) if !n.is_empty() => n,
                    _ => continue,
                };
                let mp = maven_path(name);
                let base_url = lib["url"].as_str().unwrap_or(maven_url);
                let full_url = format!("{}/{}", base_url.trim_end_matches('/'), mp);
                let sha1 = lib["sha1"].as_str().unwrap_or("");
                let size = lib["size"].as_u64().unwrap_or(0);

                let mut new_lib = lib.clone();
                if let Some(obj) = new_lib.as_object_mut() {
                    obj.remove("md5");
                    obj.remove("sha256");
                    obj.remove("sha512");
                    obj.remove("url");
                    obj.remove("mainJar");
                }
                new_lib["downloads"] = serde_json::json!({
                    "artifact": {
                        "path": mp,
                        "url": full_url,
                        "sha1": sha1,
                        "size": size,
                    }
                });
                libs.push(new_lib);
            }
        }
    }

    if let Some(obj) = merged.as_object_mut() {
        obj.remove("inheritsFrom");
    }
    Ok(merged)
}

/// 展开 inheritsFrom 继承（Forge/NeoForge 的 version.json 只有自身库 + 父指针）：
/// 以父 JSON 为基 → 追加子库/参数 → 子关键字段覆盖（缺则保留父）→ 删 inheritsFrom。
/// 子 JSON 无 inheritsFrom 时视为完整独立 JSON，直接返回（仅改 id）。
fn expand_inheritance(
    parent_text: &str,
    mut child: serde_json::Value,
    dir_name: &str,
) -> Result<serde_json::Value, String> {
    child["id"] = serde_json::Value::String(dir_name.to_string());
    if child.get("inheritsFrom").is_none() {
        return Ok(child);
    }

    let mut merged: serde_json::Value =
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败: {}", e))?;

    // libraries：父 + 子 拼接（父在前）
    let child_libs = child["libraries"].take();
    if let Some(libs) = child_libs.as_array() {
        if let Some(parent_libs) = merged["libraries"].as_array_mut() {
            parent_libs.extend(libs.iter().cloned());
        } else {
            merged["libraries"] = child_libs.clone();
        }
    }

    // arguments：父 + 子 拼接（jvm/game）
    for key in ["jvm", "game"] {
        let child_args = child["arguments"][key].take();
        if let Some(args) = child_args.as_array() {
            if let Some(parent_args) = merged["arguments"][key].as_array_mut() {
                parent_args.extend(args.iter().cloned());
            } else {
                merged["arguments"][key] = child_args.clone();
            }
        }
    }

    // 子覆盖（子无该字段则保留父的）：mainClass/minecraftArguments/assetIndex/downloads 等
    for key in [
        "mainClass",
        "minecraftArguments",
        "assets",
        "assetIndex",
        "downloads",
        "type",
        "javaVersion",
        "complianceLevel",
        "minimumLauncherVersion",
    ] {
        if let Some(v) = child.get(key) {
            merged[key] = v.clone();
        }
    }

    merged["id"] = serde_json::Value::String(dir_name.to_string());
    if let Some(obj) = merged.as_object_mut() {
        obj.remove("inheritsFrom");
    }
    Ok(merged)
}

pub struct InstallInfo {
    pub version_dir_name: String,
    pub version_json_path: PathBuf,
    pub jar_path: PathBuf,
}

/// ModLoader 安装的阶段事件（runner 钩子：推进步骤状态/置完成标志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderEvent {
    /// 合并 JSON 已写入
    ConfigDone,
    /// 本体（fabric libs / installer→jar）已完成
    LibsDone,
}

/// 阶段钩子：安装过程中在对应事件点同步调用（非 async，直接闭包）。
/// Send + Sync：钩子闭包会穿过 async task 边界（跨线程）。
pub type LoaderHook<'a> = &'a (dyn Fn(LoaderEvent) + Send + Sync);

/// 8 参数均为必需数据（版本/模板/配置/字节计数/阶段钩子），私有实现细节，不抽象。
#[allow(clippy::too_many_arguments)]
async fn install_loader_via_profile(
    mc_version: &str,
    loader_version: &str,
    dir_suffix: &str,
    parent_text: &str,
    profile_url_tpl: &str,
    config: &DownloaderConfig,
    bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-{}-{}", mc_version, dir_suffix, loader_version);
    let vdir = mc.join("versions").join(&dir_name);

    let profile_url = source.url(
        profile_url_tpl,
        &[("mc", mc_version), ("loaderVer", loader_version)],
    );
    let resp = http()
        .get(&profile_url)
        .send()
        .await
        .map_err(|e| format!("获取 profile 失败: {}", e))?;
    let profile: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 profile 失败: {}", e))?;

    let merged = merge_loader_profile(parent_text, &profile, &dir_name, &source.maven_url)?;

    std::fs::create_dir_all(&vdir).map_err(|e| format!("创建目录失败: {}", e))?;
    let json_path = vdir.join(format!("{}.json", dir_name));
    let json_str =
        serde_json::to_string_pretty(&merged).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&json_path, &json_str).map_err(|e| format!("写入 JSON 失败: {}", e))?;
    if let Some(h) = hook {
        h(LoaderEvent::ConfigDone);
    }

    let lib_dir = mc.join("libraries");
    let libraries: Vec<crate::downloader::model::Library> =
        serde_json::from_value(merged["libraries"].clone()).unwrap_or_default();
    let sources: Vec<&DownloadSource> = config.file_sources_list();
    // 文件计数保持静默（任务步骤不细分），字节计入外部计数器（收尾心跳显示速度/进度）
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    crate::downloader::library::download_libraries(
        &libraries,
        &lib_dir,
        config.concurrency as usize,
        &sources,
        counter,
        bytes.clone(),
    )
    .await
    .map_err(|e| format!("下载库失败:\n{}", e.join("\n")))?;
    if let Some(h) = hook {
        h(LoaderEvent::LibsDone);
    }

    let parent_jar = mc
        .join("versions")
        .join(mc_version)
        .join(format!("{}.jar", mc_version));
    let dir_jar = vdir.join(format!("{}.jar", dir_name));
    if parent_jar.exists() && !dir_jar.exists() {
        let _ = std::fs::copy(&parent_jar, &dir_jar);
    }

    Ok(InstallInfo {
        version_dir_name: dir_name,
        version_json_path: json_path,
        jar_path: dir_jar,
    })
}

pub async fn install_fabric(
    mc_version: &str,
    loader_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    install_loader_via_profile(
        mc_version,
        loader_version,
        "fabric",
        parent_text,
        &source.fabric_profile_url,
        config,
        bytes,
        hook,
    )
    .await
}

pub async fn install_quilt(
    mc_version: &str,
    loader_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    install_loader_via_profile(
        mc_version,
        loader_version,
        "quilt",
        parent_text,
        &source.quilt_profile_url,
        config,
        bytes,
        hook,
    )
    .await
}

pub async fn install_forge(
    mc_version: &str,
    forge_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-forge-{}", mc_version, forge_version);
    let vdir = mc.join("versions").join(&dir_name);
    let url = source.url(
        &source.forge_installer_url,
        &[("mc", mc_version), ("ver", forge_version)],
    );
    download_and_extract(&url, source, config, &vdir, &dir_name, parent_text, bytes, hook).await
}

pub async fn install_neoforge(
    mc_version: &str,
    neo_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-neoforge-{}", mc_version, neo_version);
    let vdir = mc.join("versions").join(&dir_name);
    let url = source.url(&source.neoforge_installer_url, &[("ver", neo_version)]);
    download_and_extract(&url, source, config, &vdir, &dir_name, parent_text, bytes, hook).await
}

/// 8 参数均为必需数据（URL/源/配置/目标/字节计数/阶段钩子），私有实现细节，不抽象。
#[allow(clippy::too_many_arguments)]
async fn download_and_extract(
    installer_url: &str,
    source: &DownloadSource,
    config: &DownloaderConfig,
    version_dir: &Path,
    dir_name: &str,
    parent_text: &str,
    bytes: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    hook: Option<LoaderHook<'_>>,
) -> Result<InstallInfo, String> {
    let mc_dir = config.minecraft_dir_abs();
    let tmp = mc_dir.join(".elytra").join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let installer_name = installer_url.rsplit('/').next().unwrap_or("installer.jar");
    let installer_path = tmp.join(installer_name);
    // 字节级进度：增量累加进外部计数器（收尾心跳显示速度）
    let file_done = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let progress = {
        let fd = file_done.clone();
        let b = bytes.clone();
        move |downloaded: u64, _total: u64| {
            let prev = fd.swap(downloaded, std::sync::atomic::Ordering::Relaxed);
            b.fetch_add(downloaded.saturating_sub(prev), std::sync::atomic::Ordering::Relaxed);
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
    Ok(InstallInfo {
        version_dir_name: dir_name.to_string(),
        version_json_path: json_path,
        jar_path,
    })
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

pub async fn install_optifine(
    mc_version: &str,
    patch: &str,
    of_type: &str,
    config: &DownloaderConfig,
) -> Result<InstallInfo, String> {
    let config = config.clone();
    let source = config
        .file_primary_source()
        .ok_or("no active source")?
        .clone();
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-optifine-{}", mc_version, patch);
    let vdir = mc.join("versions").join(&dir_name);

    let filename = format!("OptiFine_{}_{}_{}.jar", mc_version, of_type, patch);
    let jar_url = source.url(
        &source.optifine_download_url,
        &[("mc", mc_version), ("type", of_type), ("patch", patch)],
    );

    std::fs::create_dir_all(&vdir).map_err(|e| format!("创建目录失败: {}", e))?;
    let jar_path = vdir.join(&filename);
    fetcher::fetch(&jar_url, &jar_path, None::<fn(u64, u64)>)
        .await
        .map_err(|e| format!("下载 OptiFine 失败: {}", e))?;

    let json_path = vdir.join(format!("{}.json", dir_name));
    let minimal = serde_json::json!({
        "id": dir_name,
        "mainClass": "net.minecraft.launchwrapper.Launch",
        "libraries": [{
            "name": format!("optifine:OptiFine:{}", patch),
            "downloads": {
                "artifact": {
                    "path": format!("{}/{}", dir_name, filename),
                    "url": format!("file:///{}", jar_path.display().to_string().replace("\\", "/")),
                    "sha1": "", "size": 0
                }
            }
        }],
        "inheritsFrom": mc_version,
        "type": "release"
    });
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&minimal).map_err(|e| format!("序列化失败: {}", e))?,
    )
    .map_err(|e| format!("写入 version.json 失败: {}", e))?;

    Ok(InstallInfo {
        version_dir_name: dir_name,
        version_json_path: json_path,
        jar_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: &str = r#"{
        "id": "26.2",
        "mainClass": "net.minecraft.client.main.Main",
        "arguments": { "jvm": ["-Xmx2G"] },
        "libraries": [],
        "downloads": { "client": { "url": "https://piston-data.mojang.com/client.jar", "sha1": "abc", "size": 100 } },
        "inheritsFrom": "x",
        "type": "release"
    }"#;

    #[test]
    fn maven_path_full_coordinate() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.15.11"),
            "net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );
    }

    #[test]
    fn maven_path_short_coordinate() {
        assert_eq!(maven_path("a:b"), "a_b.jar");
    }

    #[test]
    fn merge_loader_profile_combines_all_fields() {
        let profile = serde_json::json!({
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": { "jvm": ["-Dfabric.side=client"] },
            "libraries": [
                {
                    "name": "net.fabricmc:fabric-loader:0.15.11",
                    "url": "https://maven.fabricmc.net/",
                    "sha1": "d34d",
                    "size": 5000
                },
                {
                    "name": "net.fabricmc:tiny-mappings-parser:0.3.0",
                    "sha1": "", "size": 0
                }
            ]
        });
        let merged = merge_loader_profile(
            PARENT,
            &profile,
            "26.2-fabric-0.15.11",
            "https://libraries.minecraft.net",
        )
        .expect("merge 应成功");

        assert_eq!(merged["id"], "26.2-fabric-0.15.11");
        assert_eq!(
            merged["mainClass"],
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        // jvm 参数追加：原版 + loader 的
        let jvm = merged["arguments"]["jvm"].as_array().expect("jvm 为数组");
        assert_eq!(jvm.len(), 2);
        assert_eq!(jvm[1], "-Dfabric.side=client");
        // inheritsFrom 被删除
        assert!(merged.get("inheritsFrom").is_none());
        // 原版 downloads 保留（jar 下载信息不丢）
        assert!(merged["downloads"]["client"].is_object());
        // maven 库转换
        let libs = merged["libraries"].as_array().expect("libraries 为数组");
        assert_eq!(libs.len(), 2);
        let artifact = &libs[0]["downloads"]["artifact"];
        assert_eq!(
            artifact["path"],
            "net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar"
        );
        assert_eq!(artifact["url"], "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.15.11/fabric-loader-0.15.11.jar");
        assert_eq!(artifact["sha1"], "d34d");
        assert_eq!(artifact["size"], 5000);
        // 无 url 的库回落到 maven_url；md5/url 等字段被清除
        assert!(libs[0].get("url").is_none());
        assert!(libs[0].get("md5").is_none());
        assert_eq!(
            libs[1]["downloads"]["artifact"]["url"],
            "https://libraries.minecraft.net/net/fabricmc/tiny-mappings-parser/0.3.0/tiny-mappings-parser-0.3.0.jar"
        );
    }

    #[test]
    fn merge_loader_profile_invalid_parent_returns_err() {
        let profile = serde_json::json!({"mainClass": "x"});
        let r = merge_loader_profile("not json", &profile, "id", "maven");
        assert!(r.is_err(), "原版 JSON 损坏时应返回 Err");
    }

    /// Forge 风格子 JSON：仅自身库 + inheritsFrom 父指针。
    fn forge_child() -> serde_json::Value {
        serde_json::json!({
            "id": "1.20.1-forge-47.1.0",
            "inheritsFrom": "1.20.1",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "minecraftArguments": "--username ${auth_player_name}",
            "arguments": {
                "jvm": ["-Dforge.logging.console.level=debug"],
                "game": ["--fml.forgeVersion", "47.1.0"]
            },
            "libraries": [{ "name": "net.minecraftforge:forge:47.1.0" }]
        })
    }

    #[test]
    fn expand_inheritance_merges_forge_json() {
        let merged =
            expand_inheritance(PARENT, forge_child(), "1.20.1-forge-47.1.0").expect("展开应成功");
        assert_eq!(merged["id"], "1.20.1-forge-47.1.0");
        assert!(merged.get("inheritsFrom").is_none());
        // mainClass/minecraftArguments 子覆盖
        assert_eq!(
            merged["mainClass"],
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        assert_eq!(
            merged["minecraftArguments"],
            "--username ${auth_player_name}"
        );
        // libraries：父（空）+ 子 1 个
        let libs = merged["libraries"].as_array().expect("libraries 为数组");
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0]["name"], "net.minecraftforge:forge:47.1.0");
        // arguments：jvm 父1+子1；game 父无 → 子覆盖
        let jvm = merged["arguments"]["jvm"].as_array().expect("jvm 为数组");
        assert_eq!(jvm.len(), 2);
        assert_eq!(jvm[0], "-Xmx2G");
        assert_eq!(jvm[1], "-Dforge.logging.console.level=debug");
        let game = merged["arguments"]["game"].as_array().expect("game 为数组");
        assert_eq!(game.len(), 2);
        // 父字段继承：downloads/type
        assert!(merged["downloads"]["client"].is_object());
        assert_eq!(merged["type"], "release");
    }

    #[test]
    fn expand_inheritance_appends_game_args_to_parent() {
        let parent = r#"{
            "id": "1.20.1",
            "mainClass": "net.minecraft.client.main.Main",
            "arguments": { "jvm": ["-Xmx2G"], "game": ["--version", "${version_name}"] },
            "libraries": [],
            "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
            "type": "release"
        }"#;
        let merged =
            expand_inheritance(parent, forge_child(), "1.20.1-forge-47.1.0").expect("展开应成功");
        let game = merged["arguments"]["game"].as_array().expect("game 为数组");
        assert_eq!(game.len(), 4);
        assert_eq!(game[0], "--version");
        assert_eq!(game[2], "--fml.forgeVersion");
    }

    #[test]
    fn expand_inheritance_without_parent_returns_child() {
        let standalone = serde_json::json!({
            "id": "x",
            "mainClass": "net.minecraft.client.main.Main",
            "libraries": []
        });
        let merged = expand_inheritance("unused", standalone.clone(), "26.2-fabric-0.15.11")
            .expect("无 inheritsFrom 时应直接返回");
        assert_eq!(merged["id"], "26.2-fabric-0.15.11");
        assert_eq!(merged["mainClass"], "net.minecraft.client.main.Main");
    }

    #[test]
    fn expand_inheritance_invalid_parent_returns_err() {
        let r = expand_inheritance("not json", forge_child(), "id");
        assert!(r.is_err(), "父 JSON 损坏时应返回 Err");
    }
}
