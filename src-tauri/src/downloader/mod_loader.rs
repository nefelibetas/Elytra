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

pub struct InstallInfo {
    pub version_dir_name: String,
    pub version_json_path: PathBuf,
    pub jar_path: PathBuf,
}

async fn install_loader_via_profile(
    mc_version: &str,
    loader_version: &str,
    dir_suffix: &str,
    parent_text: &str,
    profile_url_tpl: &str,
    config: &DownloaderConfig,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-{}-{}", mc_version, dir_suffix, loader_version);
    let vdir = mc.join("versions").join(&dir_name);

    // 原版父 JSON 由调用方保证（run_task 已拉取），仅做内存合并；
    // 不创建/覆盖 versions/{mc}/——Fabric 版本是完整独立 JSON（inheritsFrom 已删），
    // 原版目录只应在用户单独下载原版时由下载流程创建
    let mut merged: serde_json::Value =
        serde_json::from_str(parent_text).map_err(|e| format!("解析原版版本 JSON 失败: {}", e))?;

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

    if let Some(fabric_jvm) = profile["arguments"]["jvm"].as_array() {
        if !fabric_jvm.is_empty() {
            if let Some(jvm) = merged["arguments"]["jvm"].as_array_mut() {
                for arg in fabric_jvm {
                    jvm.push(arg.clone());
                }
            } else {
                merged["arguments"]["jvm"] = serde_json::Value::Array(fabric_jvm.clone());
            }
        }
    }

    merged["id"] = serde_json::Value::String(dir_name.clone());
    if let Some(mc) = profile["mainClass"].as_str() {
        merged["mainClass"] = serde_json::Value::String(mc.to_string());
    }

    if let Some(fabric_libs) = profile["libraries"].as_array() {
        if let Some(libs) = merged["libraries"].as_array_mut() {
            for lib in fabric_libs {
                let name = match lib["name"].as_str() {
                    Some(n) if !n.is_empty() => n,
                    _ => continue,
                };
                let mp = maven_path(name);
                let base_url = lib["url"].as_str().unwrap_or(&source.maven_url);
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

    std::fs::create_dir_all(&vdir).map_err(|e| format!("创建目录失败: {}", e))?;
    let json_path = vdir.join(format!("{}.json", dir_name));
    let json_str =
        serde_json::to_string_pretty(&merged).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&json_path, &json_str).map_err(|e| format!("写入 JSON 失败: {}", e))?;

    let lib_dir = mc.join("libraries");
    let libraries: Vec<crate::downloader::model::Library> =
        serde_json::from_value(merged["libraries"].clone()).unwrap_or_default();
    let sources: Vec<&DownloadSource> = config.file_sources_list();
    // 静默下载：一次性计数器（无进度展示场景）
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let bytes = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    crate::downloader::library::download_libraries(
        &libraries,
        &lib_dir,
        config.concurrency as usize,
        &sources,
        counter,
        bytes,
    )
    .await
    .map_err(|e| format!("下载库失败:\n{}", e.join("\n")))?;

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
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    install_loader_via_profile(
        mc_version,
        loader_version,
        "fabric",
        parent_text,
        &source.fabric_profile_url,
        config,
    )
    .await
}

pub async fn install_forge(
    mc_version: &str,
    forge_version: &str,
    config: &DownloaderConfig,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-forge-{}", mc_version, forge_version);
    let vdir = mc.join("versions").join(&dir_name);
    let url = source.url(
        &source.forge_installer_url,
        &[("mc", mc_version), ("ver", forge_version)],
    );
    download_and_extract(&url, source, config, &vdir, &dir_name).await
}

pub async fn install_neoforge(
    mc_version: &str,
    neo_version: &str,
    config: &DownloaderConfig,
) -> Result<InstallInfo, String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-neoforge-{}", mc_version, neo_version);
    let vdir = mc.join("versions").join(&dir_name);
    let url = source.url(&source.neoforge_installer_url, &[("ver", neo_version)]);
    download_and_extract(&url, source, config, &vdir, &dir_name).await
}

async fn download_and_extract(
    installer_url: &str,
    source: &DownloadSource,
    config: &DownloaderConfig,
    version_dir: &Path,
    dir_name: &str,
) -> Result<InstallInfo, String> {
    let mc_dir = config.minecraft_dir_abs();
    let tmp = mc_dir.join(".elytra").join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {}", e))?;
    let installer_name = installer_url.split('/').last().unwrap_or("installer.jar");
    let installer_path = tmp.join(installer_name);
    fetcher::fetch(installer_url, &installer_path, None::<fn(u64, u64)>)
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
        resolve_version_json(&mut archive, &profile, version_dir, &json_path, dir_name).await;

    // --- jar ---
    let found_jar = resolve_jar(
        &mut archive,
        &profile,
        source,
        config,
        version_dir,
        &jar_path,
        &tmp,
        dir_name,
    )
    .await;

    let _ = std::fs::remove_file(&installer_path);
    if !found_json {
        return Err("未找到 version.json".into());
    }
    if !found_jar {
        return Err("未找到 client.jar".into());
    }
    Ok(InstallInfo {
        version_dir_name: dir_name.to_string(),
        version_json_path: json_path,
        jar_path,
    })
}

/// Resolve version.json from ZIP install_profile.json metadata or by scanning.
async fn resolve_version_json(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &Option<serde_json::Value>,
    _version_dir: &Path,
    json_path: &Path,
    dir_name: &str,
) -> bool {
    // Strategy 1: install_profile has "json" field → extract from ZIP
    if let Some(p) = profile {
        if let Some(json_path_in_zip) = p["json"].as_str() {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).ok().expect("read entry");
                if entry.name() == json_path_in_zip.trim_start_matches('/') {
                    let mut buf = String::new();
                    entry.read_to_string(&mut buf).ok().expect("read json");
                    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&buf) {
                        v["id"] = serde_json::Value::String(dir_name.to_string());
                        let out = serde_json::to_string_pretty(&v).ok().expect("serialize");
                        std::fs::write(&json_path, &out).ok().expect("write json");
                        return true;
                    }
                }
            }
        }

        // Strategy 2: install_profile has "versionInfo" → serialize that
        if let Some(vi) = p["versionInfo"].as_object() {
            let mut v = serde_json::Value::Object(vi.clone());
            v["id"] = serde_json::Value::String(dir_name.to_string());
            let out = serde_json::to_string_pretty(&v).ok().expect("serialize");
            if std::fs::write(&json_path, &out).is_ok() {
                return true;
            }
        }
    }

    // Strategy 3: scan for any entry ending in version.json
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok().expect("read entry");
        let name = entry.name().to_string();
        if name.ends_with("version.json") {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).ok().expect("read json");
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&buf) {
                v["id"] = serde_json::Value::String(dir_name.to_string());
                let out = serde_json::to_string_pretty(&v).ok().expect("serialize");
                std::fs::write(&json_path, &out).ok().expect("write json");
                return true;
            }
        }
    }

    false
}

/// Resolve client jar from ZIP, maven download, or fallback to original manifest.
async fn resolve_jar(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &Option<serde_json::Value>,
    source: &DownloadSource,
    _config: &DownloaderConfig,
    _version_dir: &Path,
    jar_path: &Path,
    _tmp: &Path,
    dir_name: &str,
) -> bool {
    // Strategy 1: install_profile → install.filePath → extract from ZIP
    if let Some(p) = profile {
        if let Some(fp) = p.pointer("/install/filePath").and_then(|v| v.as_str()) {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).ok().expect("read entry");
                if entry.name() == fp {
                    let mut out = std::fs::File::create(&jar_path).ok().expect("create jar");
                    std::io::copy(&mut entry, &mut out)
                        .ok()
                        .expect("extract jar");
                    return true;
                }
            }
        }
    }

    // Strategy 2: install_profile → path (maven path) → download from maven
    if let Some(p) = profile {
        if let Some(maven_path) = p["path"].as_str() {
            let jar_url = format!("{}/{}", source.maven_url.trim_end_matches('/'), maven_path);
            return fetcher::fetch(&jar_url, &jar_path, None::<fn(u64, u64)>)
                .await
                .is_ok();
        }
    }

    // Strategy 3: scan ZIP for any .jar entry (exclude installer/sources/javadoc)
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok().expect("read entry");
        let name = entry.name().to_string();
        if name.ends_with(".jar")
            && !name.contains("installer")
            && !name.contains("sources")
            && !name.contains("javadoc")
        {
            let mut out = std::fs::File::create(&jar_path).ok().expect("create jar");
            std::io::copy(&mut entry, &mut out)
                .ok()
                .expect("extract jar");
            return true;
        }
    }

    // Strategy 4: download client.jar from original version manifest
    // ponytail: only tries first manifest page, no pagination.
    let manifest_url = format!("{}/mc/game/version_manifest_v2.json", source.manifest_url);
    if let Ok(resp) = http().get(&manifest_url).send().await {
        if let Ok(text) = resp.text().await {
            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(versions) = manifest["versions"].as_array() {
                    // Extract original MC version from dir_name (e.g., "1.20.1-forge-47.1.0" -> "1.20.1")
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
                                            return fetcher::fetch(
                                                &dl_url,
                                                &jar_path,
                                                None::<fn(u64, u64)>,
                                            )
                                            .await
                                            .is_ok();
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

    false
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
