// ── Installer 式安装（Forge / NeoForge 共用流程）：官方 installer.jar 下载 + ZIP 解析；NeoForge 专属逻辑在 neoforge.rs ──

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::neoforge::{download_neoforge_universal, extract_neoforge_client};
use super::processors;
use super::{expand_inheritance, InstallCtx, LoaderEvent, LoaderHook, VersionTarget};
use crate::config::downloader::{DownloadSource, DownloaderConfig};
use crate::downloader::endpoints;
use crate::downloader::fetcher::{self, http};
use crate::game::resource::{extract, library};
use crate::game::version::model;
use crate::jdk;
use crate::util::java_bin_path;

pub async fn install_forge(
    mc_version: &str,
    forge_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let InstallCtx { config, .. } = ctx;
    // 多源回退：官方 maven 对国内网络不稳，按完整源顺序各构造 URL，fetch_fallback 依次尝试
    let installer_urls: Vec<String> = config
        .file_sources_list()
        .iter()
        .map(|s| {
            s.url(
                &s.forge_installer_url,
                &[("mc", mc_version), ("ver", forge_version)],
            )
        })
        .collect();
    download_and_extract(&installer_urls, target, parent_text, ctx).await
}

/// Forge 与 NeoForge 共用：version.json/jar 解析后按分支处理各自专属构件。
pub(super) async fn download_and_extract(
    installer_urls: &[String],
    target: VersionTarget<'_>,
    parent_text: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let VersionTarget {
        dir_name,
        vdir: version_dir,
    } = target;
    let InstallCtx {
        config,
        pc,
        cancel,
        hook,
        ..
    } = ctx;
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc_dir = config.minecraft_dir_abs();
    let (mut archive, installer_path) =
        fetch_installer(installer_urls, &mc_dir, pc, cancel).await?;

    std::fs::create_dir_all(version_dir)
        .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    let jar_path = version_dir.join(format!("{}.jar", dir_name));
    let json_path = version_dir.join(format!("{}.json", dir_name));

    // Parse install_profile.json if present
    let profile = read_install_profile(&mut archive);

    // --- version.json ---
    let found_json =
        resolve_version_json(&mut archive, &profile, &json_path, dir_name, parent_text).await?;

    // --- jar ---
    let jar_missing = resolve_jar(
        &mut archive,
        &profile,
        source,
        &jar_path,
        dir_name,
        &json_path,
    )
    .await?;

    // installer 须保留到安装结束：processors 引擎需要 {INSTALLER}
    if !found_json {
        return Err(keyed_msg!(
            "errors.loader.versionJsonMissing",
            "未找到 version.json"
        ));
    }
    if let Some(h) = hook {
        h(LoaderEvent::ConfigDone);
    }
    if let Some(reason) = jar_missing {
        bail_key!(
            "errors.loader.clientJarMissing",
            "未找到 client.jar\n{reason}"
        );
    }

    // ── 下载展开后 JSON 的全部库（vanilla 库已存在自动跳过，实际拉取 ModLoader 增量）──
    // Forge/NeoForge 26.x 本体（universal/fancymodloader 等）以 library 形式进 classpath，安装阶段必须一并下载
    let lib_dir = mc_dir.join("libraries");
    let mut libraries = read_libraries(&json_path)?;
    // 客户端 patched jar 由 installer 声明的 processors 生成（1.12- 无 processors → 跳过，由 universal 提供）
    let profile_ref = profile.clone().unwrap_or(serde_json::Value::Null);
    // spec-1 legacy 安装器（如 Forge 1.12.2）的构件内嵌在 zip 的 maven/ 下、version.json URL 为空，须在下载支持库前解出
    install_embedded_maven(&mut archive, &profile_ref, &lib_dir)?;
    // spec-0 老 installer：install.filePath（zip 根）按 install.path 坐标落位
    install_profile_file(&mut archive, &profile_ref, &lib_dir)?;
    // install_profile 声明的构件也必须落盘（含 version.json 里没有声明的运行时模块 jar，见 profile_libraries）
    extend_with_profile_libraries(&mut libraries, &profile_ref);
    let files = LoaderFiles {
        dir_name,
        mc_dir: &mc_dir,
        lib_dir: &lib_dir,
        config,
        cancel,
    };
    forge_patch_client(&mut archive, &installer_path, &profile_ref, &files).await?;
    // NeoForge：universal 不在 version.json，须下载到 libraries 但不进 classpath（进了会因"找到
    // NeoForge 类而无 MC 类"误报 "The patched Minecraft jar is missing"）；同时跑 installertools
    // PROCESS_MINECRAFT_JAR 生成 minecraft-client-patched（新式安装优先用它，缺 client-srg.jar 会报 corrupted）
    if let Some(uni) = find_neoforge_universal(&profile) {
        install_neoforge_universal(
            uni,
            &mut archive,
            &installer_path,
            &libraries,
            &files,
            pc,
            hook,
        )
        .await?;
        let _ = std::fs::remove_file(&installer_path);
        verify_profile_artifacts(&profile_ref, &lib_dir)?;
        return Ok(());
    }
    // 其余库走统一下载（多源回退 + sha1 校验；URL 为空的库在此被跳过）
    download_loader_libraries(&libraries, &lib_dir, dir_name, config, pc, cancel).await?;
    verify_profile_artifacts(&profile_ref, &lib_dir)?;

    let _ = std::fs::remove_file(&installer_path);
    if let Some(h) = hook {
        h(LoaderEvent::LibsDone);
    }
    Ok(())
}

/// 安装器解析出的共享文件位置与下载句柄（NeoForge 分支与统一下载阶段共用）。
#[derive(Clone, Copy)]
struct LoaderFiles<'a> {
    dir_name: &'a str,
    mc_dir: &'a Path,
    lib_dir: &'a Path,
    config: &'a DownloaderConfig,
    cancel: &'a Arc<AtomicBool>,
}

/// 下载官方 installer.jar 到临时目录并打开为 ZIP。
async fn fetch_installer(
    installer_urls: &[String],
    mc_dir: &Path,
    pc: &crate::downloader::progress::ProgressCounters,
    cancel: &Arc<AtomicBool>,
) -> Result<(zip::ZipArchive<std::fs::File>, PathBuf), String> {
    let tmp = mc_dir.join(".elytra").join("tmp");
    std::fs::create_dir_all(&tmp).map_err(common_err_ctx_key!(
        "errors.file.createDir",
        "创建临时目录失败"
    ))?;
    let installer_name = installer_urls
        .first()
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or("installer.jar");
    let installer_path = tmp.join(installer_name);
    // 字节级进度：step_progress 工厂（增量累加进全局与 LoaderLibs 步骤计数，total 为分母）
    fetcher::fetch_fallback_c(
        installer_urls,
        &installer_path,
        Some(pc.step_progress()),
        Some(cancel),
    )
    .await
    .map_err(common_err_ctx_key!(
        "errors.download.failed",
        "下载安装程序失败"
    ))?;

    let file = std::fs::File::open(&installer_path)
        .map_err(common_err_ctx_key!("errors.file.read", "打开安装程序失败"))?;
    let archive = zip::ZipArchive::new(file)
        .map_err(common_err_ctx_key!("errors.file.read", "读取安装程序失败"))?;
    Ok((archive, installer_path))
}

/// 解析安装器内的 install_profile.json（缺失或解析失败返回 None）。
fn read_install_profile(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<serde_json::Value> {
    (0..archive.len()).find_map(|i| {
        let mut entry = archive.by_index(i).ok()?;
        if !entry.name().ends_with("install_profile.json") {
            return None;
        }
        let mut buf = String::new();
        entry.read_to_string(&mut buf).ok()?;
        serde_json::from_str(&buf).ok()
    })
}

/// 读取展开后的 version.json 并取出 libraries 列表（无该字段时为空列表）。
fn read_libraries(json_path: &Path) -> Result<Vec<model::Library>, String> {
    let json_text = std::fs::read_to_string(json_path).map_err(common_err_ctx_key!(
        "errors.file.read",
        "读取 version.json 失败"
    ))?;
    let merged: serde_json::Value = serde_json::from_str(&json_text).map_err(
        common_err_ctx_key!("errors.version.parse", "解析 version.json 失败"),
    )?;
    Ok(serde_json::from_value(merged["libraries"].clone()).unwrap_or_default())
}

/// install_profile 里可网络下载的构件（URL 为空的内嵌构件由 `install_embedded_maven` / `install_profile_file` 落盘，不走批量下载）。
/// Forge 1.17+ 运行时模块 jar（`fmlcore` / `javafmllanguage` / `lowcodelanguage` / `mclanguage`）只在 install_profile 声明，
/// version.json 只声明 fmlloader/fmlearlydisplay；缺一即 `Invalid paths argument, contained no existing paths` 秒退（退出码 1）。
pub(super) fn profile_libraries(profile: &serde_json::Value) -> Vec<model::Library> {
    profile["libraries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|lib| {
            let artifact = &lib["downloads"]["artifact"];
            let path = artifact["path"].as_str()?.to_string();
            let url = artifact["url"].as_str().unwrap_or("").to_string();
            if path.is_empty() || url.is_empty() {
                return None;
            }
            Some(model::Library {
                name: lib["name"].as_str().unwrap_or(&path).to_string(),
                downloads: Some(model::LibraryDownloads {
                    artifact: Some(model::Artifact {
                        path,
                        url,
                        sha1: artifact["sha1"].as_str().unwrap_or("").to_string(),
                        size: artifact["size"].as_u64().unwrap_or(0),
                    }),
                    classifiers: None,
                }),
                rules: None,
                natives: None,
                extract: None,
            })
        })
        .collect()
}

/// 把 install_profile 的可下载构件并入待下清单，按 `downloads.artifact.path` 去重（保留先到者：
/// version.json 已声明的条目带着正确的分类器/规则）。
fn extend_with_profile_libraries(libraries: &mut Vec<model::Library>, profile: &serde_json::Value) {
    for lib in profile_libraries(profile) {
        let path = lib
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .map(|a| a.path.clone());
        let known = libraries.iter().any(|l| {
            l.downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .map(|a| a.path.as_str())
                == path.as_deref()
        });
        if !known {
            libraries.push(lib);
        }
    }
}

/// 安装收尾核对：install_profile 声明的**每一个**构件都必须落在 `libraries/`（含 version.json 未声明、只由 profile 声明的运行时模块 jar，以及 URL 为空、由 installer 内嵌流程落盘的条目）。
/// 下载/解包环节的静默漏项（sha1 缺失、内嵌条目没解出来、镜像 404 被当作"可选"）在此变成**安装期错误**；否则表现为"安装成功、启动秒退"。
fn verify_profile_artifacts(profile: &serde_json::Value, lib_dir: &Path) -> Result<(), String> {
    let mut missing: Vec<String> = Vec::new();
    for lib in profile["libraries"].as_array().into_iter().flatten() {
        let artifact = &lib["downloads"]["artifact"];
        let Some(path) = artifact["path"].as_str() else {
            continue;
        };
        let dest = lib_dir.join(path);
        let sha1 = artifact["sha1"].as_str().unwrap_or("");
        let ok = if sha1.is_empty() {
            dest.is_file()
        } else {
            crate::downloader::hash::file_sha1_matches(&dest, sha1)
        };
        if !ok {
            missing.push(path.to_string());
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let missing_count = missing.len();
    let mut msg = keyed_msg!(
        "errors.loader.librariesMissing?count={missing_count}",
        "ModLoader 支持库缺失 {missing_count} 个（安装器 install_profile 已声明，缺文件会导致游戏启动失败）"
    );
    for p in missing.iter().take(20) {
        msg.push_str(&format!("\n  {p}"));
    }
    if missing.len() > 20 {
        msg.push_str(&format!("\n  … 其余 {} 个", missing.len() - 20));
    }
    Err(msg)
}

/// 在 install_profile.libraries 中查找 NeoForge universal 条目。
fn find_neoforge_universal(profile: &Option<serde_json::Value>) -> Option<&serde_json::Value> {
    profile.as_ref()?["libraries"].as_array()?.iter().find(|l| {
        l["name"]
            .as_str()
            .is_some_and(|n| n.starts_with("net.neoforged:neoforge:") && n.ends_with(":universal"))
    })
}

/// 下载展开后 JSON 的全部库（多源回退 + sha1 校验；URL 为空的库在此被跳过）。
async fn download_loader_libraries(
    libraries: &[model::Library],
    lib_dir: &Path,
    dir_name: &str,
    config: &DownloaderConfig,
    pc: &crate::downloader::progress::ProgressCounters,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let sources: Vec<&DownloadSource> = config.file_sources_list();
    library::download_libraries(
        libraries,
        lib_dir,
        config.concurrency as usize,
        &sources,
        None, // loader 安装内库下载不走复用（run_task 主流程才探测复用根）
        pc,
        Some(cancel),
    )
    .await
    .map_err(|e| {
        keyed_msg!(
            "errors.download.failed",
            "下载 {} 支持库失败:\n{}",
            dir_name,
            e.join("\n")
        )
    })?;
    Ok(())
}

/// NeoForge 分支：下载 universal 并运行 installertools 生成 patched client，随后下载其余支持库。
async fn install_neoforge_universal(
    uni: &serde_json::Value,
    archive: &mut zip::ZipArchive<std::fs::File>,
    installer_path: &Path,
    libraries: &[model::Library],
    files: &LoaderFiles<'_>,
    pc: &crate::downloader::progress::ProgressCounters,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let LoaderFiles {
        dir_name,
        mc_dir,
        lib_dir,
        config,
        cancel,
    } = *files;
    let neo_version = uni["name"]
        .as_str()
        .and_then(|n| n.split(':').nth(2))
        .unwrap_or("")
        .to_string();
    if neo_version.is_empty() {
        return Err(keyed_msg!(
            "errors.loader.universalCoordNoVersion",
            "NeoForge universal 坐标缺少版本号"
        ));
    }
    download_neoforge_universal(uni, lib_dir, config, cancel).await?;
    // 运行 installertools 生成 patched（前置：等待 vanilla 下载完成）
    extract_neoforge_client(
        archive,
        installer_path,
        lib_dir,
        mc_dir,
        dir_name,
        InstallCtx {
            config,
            pc,
            cancel,
            hook,
        },
    )
    .await?;
    // 其余库走统一下载（universal 已单独下载，JSON 无其条目 → 不进 classpath）
    download_loader_libraries(libraries, lib_dir, dir_name, config, pc, cancel).await?;
    if let Some(h) = hook {
        h(LoaderEvent::LibsDone);
    }
    Ok(())
}
/// 安装器内嵌的 maven 构件：spec-1 legacy（如 Forge 1.12.2）把构件放在 zip 的 `maven/<group>/<artifact>/<ver>/<file>` 下、
/// version.json 里对应库 URL 为空。按 profile 声明的 sha1（有则校验）解到 `libraries/<相对路径>`，已就绪则跳过。
fn install_embedded_maven(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &serde_json::Value,
    lib_dir: &Path,
) -> Result<(), String> {
    let mut entries: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(common_err_ctx_key!(
            "errors.file.read",
            "读取安装程序条目失败"
        ))?;
        if entry.is_dir() {
            continue;
        }
        if let Some(rel) = entry.name().strip_prefix("maven/") {
            if !rel.is_empty() {
                entries.push(rel.to_string());
            }
        }
    }
    for rel in entries {
        let dest = lib_dir.join(&rel);
        let sha1 = declared_sha1(profile, &rel);
        if !sha1.is_empty() && crate::downloader::hash::file_sha1_matches(&dest, &sha1) {
            continue;
        }
        if sha1.is_empty() && dest.is_file() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
        }
        let mut member = archive
            .by_name(&format!("maven/{rel}"))
            .map_err(common_err_ctx_key!(
                "errors.file.read",
                "读取安装程序条目失败"
            ))?;
        let mut out = std::fs::File::create(&dest)
            .map_err(common_err_ctx_key!("errors.file.write", "创建文件失败"))?;
        std::io::copy(&mut member, &mut out).map_err(common_err_ctx_key!(
            "errors.file.extract",
            "解出安装器内嵌构件失败"
        ))?;
    }
    Ok(())
}

/// spec-0 老 installer（Forge 1.7.10 / 1.8.9）：`install.path` 为 maven 坐标、`install.filePath` 为 zip 根文件名。
/// 这些版本 maven 上无同名普通构件（只有 -universal/-userdev/-installer）而 versionInfo 按普通坐标声明 → 直下 404；
/// 照官方把 zip 内 filePath 复制成该坐标（无 sha1 可校验，存在即跳过）。
fn install_profile_file(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &serde_json::Value,
    lib_dir: &Path,
) -> Result<(), String> {
    let install = &profile["install"];
    let (Some(coord), Some(file)) = (install["path"].as_str(), install["filePath"].as_str()) else {
        return Ok(());
    };
    let mut parts = coord.split(':');
    let (Some(group), Some(artifact), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Ok(());
    };
    let classifier = parts.next().unwrap_or("");
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jar");
    let name = if classifier.is_empty() {
        format!("{artifact}-{version}.{ext}")
    } else {
        format!("{artifact}-{version}-{classifier}.{ext}")
    };
    let dest = lib_dir
        .join(group.replace('.', "/"))
        .join(artifact)
        .join(version)
        .join(name);
    if dest.is_file() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    }
    let mut member = archive.by_name(file).map_err(common_err_ctx_key!(
        "errors.loader.installerEntryMissing",
        "安装程序内缺少 {} ",
        file
    ))?;
    let mut out = std::fs::File::create(&dest)
        .map_err(common_err_ctx_key!("errors.file.write", "创建文件失败"))?;
    std::io::copy(&mut member, &mut out).map_err(common_err_ctx_key!(
        "errors.file.extract",
        "解出安装器内嵌文件失败"
    ))?;
    Ok(())
}

/// 从 install_profile.libraries 里按 maven 相对路径取声明的 sha1（无声明返回空串）。
fn declared_sha1(profile: &serde_json::Value, maven_rel_path: &str) -> String {
    profile["libraries"]
        .as_array()
        .and_then(|libs| {
            libs.iter().find_map(|l| {
                let p = l["downloads"]["artifact"]["path"].as_str()?;
                (p == maven_rel_path).then(|| {
                    l["downloads"]["artifact"]["sha1"]
                        .as_str()
                        .unwrap_or("")
                        .to_string()
                })
            })
        })
        .unwrap_or_default()
}

/// Forge：按 installer 声明的 `install_profile.processors` 生成 patched client（与 NeoForge 同源）；
/// 旧式 installer（1.12-）无 processors → 客户端由 universal jar 提供。
fn forge_patch_client<'a>(
    archive: &'a mut zip::ZipArchive<std::fs::File>,
    installer_path: &'a Path,
    profile_json: &'a serde_json::Value,
    files: &'a LoaderFiles<'a>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
    let LoaderFiles {
        dir_name,
        mc_dir,
        lib_dir,
        config,
        cancel,
    } = *files;
    Box::pin(async move {
        let _ = archive; // 条目解包由 processors 引擎按 profile 自行处理
        let profile = processors::parse_profile_value(profile_json)?;
        if profile.processors.is_empty() {
            return Ok(());
        }
        let patched = processors::output_path(&profile, "PATCHED", lib_dir, mc_dir)?;
        if processors::intact_archive(&patched) {
            return Ok(());
        }
        let vanilla_jar = wait_for_vanilla_client(mc_dir, dir_name, cancel).await?;
        let work_dir = mc_dir.join(".elytra").join("tmp").join("loader");
        let pctx = processors::ProcessorCtx {
            installer: installer_path,
            mc_dir,
            vanilla_jar: &vanilla_jar,
            lib_dir,
            work_dir: &work_dir,
            config,
            cancel,
        };
        processors::run_client_processors(&profile, &pctx).await?;
        if !processors::intact_archive(&patched) {
            bail_key!(
                "errors.install.loader",
                "Forge 安装链执行完毕但未生成客户端\n{}",
                patched.display()
            );
        }
        Ok(())
    })
}

/// 等待 runner 的 ClientJar 步骤下载完成（与 install 并行）：连续 3 次（间隔 500ms）大小不变视为完成；上限 120s，取消立即返回。
pub(super) async fn wait_for_vanilla_client(
    mc_dir: &Path,
    dir_name: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<PathBuf, String> {
    let vanilla_jar = mc_dir
        .join("versions")
        .join(dir_name)
        .join(format!("{dir_name}.jar"));
    let mut last_size = 0u64;
    let mut stable = 0u32;
    for _ in 0..240 {
        if cancel.load(Ordering::SeqCst) {
            return Err(keyed_msg!("errors.task.cancelled", "任务已取消"));
        }
        let size = std::fs::metadata(&vanilla_jar)
            .map(|m| m.len())
            .unwrap_or(0);
        if size > 1_000_000 {
            if size == last_size {
                stable += 1;
                if stable >= 3 {
                    return Ok(vanilla_jar);
                }
            } else {
                stable = 0;
            }
            last_size = size;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(keyed_msg!(
        "errors.loader.vanillaClientTimeout",
        "等待原版客户端下载超时\n120s 内未检测到完整的原版客户端，无法生成 ModLoader client"
    ))
}

/// Auto 模式选 JDK 的纯决策：`required > 0` 走启动侧按需选择（精确/最低满足/最高），否则回退缓存首项。
fn pick_auto_jdk(jdks: &[jdk::model::JdkInfo], required: u32) -> Option<jdk::model::JdkInfo> {
    let picked = if required > 0 {
        crate::launcher::env::auto_pick_jdk(jdks, required)
    } else {
        None
    };
    picked.or_else(|| jdks.first().cloned())
}

/// 运行补丁工具所需的 Java 可执行文件（与游戏启动同一套 JDK 配置）。`required` 为目标版本声明的 Java 主版本；
/// Auto 模式据此按需选择（无脑取缓存首项可能落在 Java 8 上 → `UnsupportedClassVersionError`）。
pub(super) fn resolve_java(required: u32) -> Result<PathBuf, String> {
    let options = jdk::read();
    let cache = jdk::read_cache();
    match options.selection {
        jdk::model::JdkSelection::Manual { path } => Ok(java_bin_path(Path::new(&path))),
        jdk::model::JdkSelection::Auto => {
            if let Some(jdk) = pick_auto_jdk(&cache.jdks, required) {
                return Ok(java_bin_path(Path::new(&jdk.path)));
            }
            // 兜底：装了 JDK 但还没让启动器扫描过（JAVA_HOME 指向的目录直接可用）
            if let Ok(home) = std::env::var("JAVA_HOME") {
                let java = java_bin_path(Path::new(&home));
                if java.is_file() {
                    return Ok(java);
                }
            }
            Err(keyed_msg!(
                "errors.loader.javaMissing",
                "未检测到 Java 环境\n无法运行 ModLoader 安装工具（需要 JDK，请在设置中配置 Java 或设置 JAVA_HOME）"
            ))
        }
    }
}

/// 提取 version.json 后展开 inheritsFrom（JSON 只含自身库 + 父指针，须合并原版父 JSON）；三策略：install_profile.json → versionInfo → 扫描条目。
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
        let out = serde_json::to_string_pretty(&merged).map_err(common_err_ctx_key!(
            "errors.loader.serializeVersionJson",
            "序列化失败"
        ))?;
        std::fs::write(json_path, &out).map_err(common_err_ctx_key!(
            "errors.file.write",
            "写入 version.json 失败"
        ))
    };

    // Strategy 1: install_profile has "json" field → extract from ZIP
    if let Some(p) = profile {
        if let Some(json_path_in_zip) = p["json"].as_str() {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(common_err_ctx_key!(
                    "errors.file.read",
                    "读取安装程序条目失败"
                ))?;
                if entry.name() == json_path_in_zip.trim_start_matches('/') {
                    let mut buf = String::new();
                    entry
                        .read_to_string(&mut buf)
                        .map_err(common_err_ctx_key!("errors.file.read", "读取 JSON 失败"))?;
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
        let mut entry = archive.by_index(i).map_err(common_err_ctx_key!(
            "errors.file.read",
            "读取安装程序条目失败"
        ))?;
        let name = entry.name().to_string();
        if name.ends_with("version.json") {
            let mut buf = String::new();
            entry
                .read_to_string(&mut buf)
                .map_err(common_err_ctx_key!("errors.file.read", "读取 JSON 失败"))?;
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&buf) {
                write_extracted(v)?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// 解析 client jar：ZIP 内条目 / maven 下载 / 兜底官方清单。`Ok(Some(原因))` = 各策略未命中（原因上交便于定位真实故障点）。
async fn resolve_jar(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &Option<serde_json::Value>,
    source: &DownloadSource,
    jar_path: &Path,
    dir_name: &str,
    json_path: &Path,
) -> Result<Option<String>, String> {
    // 新版 Forge（26.x+）：install_profile 无 install 字段，ZIP 里只有 shim/universal（属 library，不进版本目录）；client.jar 由 version.json 的 downloads.client（继承 vanilla）提供
    if let Some(p) = profile {
        if p.get("install").is_none() {
            if let Ok(json) = std::fs::read_to_string(json_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                    if v.pointer("/downloads/client").is_some() {
                        return Ok(None);
                    }
                }
            }
        }
    }

    // Strategy 1: install_profile → install.filePath → extract from ZIP
    if let Some(p) = profile {
        if let Some(fp) = p.pointer("/install/filePath").and_then(|v| v.as_str()) {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i).map_err(common_err_ctx_key!(
                    "errors.file.read",
                    "读取安装程序条目失败"
                ))?;
                if entry.name() == fp {
                    extract::extract_entry_to(&mut entry, jar_path)?;
                    return Ok(None);
                }
            }
        }
    }

    // Strategy 2: install_profile → path (maven 坐标或路径) → download from maven
    if let Some(p) = profile {
        if let Some(maven_path) = p["path"].as_str() {
            // maven 坐标 "group:artifact:version[:classifier]" → 路径格式
            let path = if maven_path.contains(':') {
                let parts: Vec<&str> = maven_path.split(':').collect();
                if parts.len() >= 3 {
                    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
                    let dir = format!(
                        "{}/{}/{}/{}",
                        group.replace('.', "/"),
                        artifact,
                        version,
                        artifact
                    );
                    match parts.get(3) {
                        Some(c) => format!("{}-{}-{}.jar", dir, version, c),
                        None => format!("{}-{}.jar", dir, version),
                    }
                } else {
                    maven_path.to_string()
                }
            } else {
                maven_path.to_string()
            };
            // Forge 构件在 maven.minecraftforge.net（source.maven_url 是 Mojang 库源，不适用）；镜像经 mirror_url 重写到 BMCLAPI /maven/
            let jar_url = source.mirror_url(&format!("{}/{}", endpoints::FORGE_MAVEN, path));
            // maven 直下失败原因随 Ok(Some(..)) 上交（命中本策略后不再尝试后续策略）
            return match fetcher::fetch(&jar_url, jar_path, None::<fn(u64, u64)>).await {
                Ok(_) => Ok(None),
                Err(e) => Ok(Some(keyed_msg!(
                    "errors.download.failed",
                    "maven 下载 client.jar 失败\n{e}"
                ))),
            };
        }
    }

    // Strategy 3: scan ZIP for any .jar entry (exclude installer/sources/javadoc)
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(common_err_ctx_key!(
            "errors.file.read",
            "读取安装程序条目失败"
        ))?;
        let name = entry.name().to_string();
        if name.ends_with(".jar")
            && !name.contains("installer")
            && !name.contains("sources")
            && !name.contains("javadoc")
        {
            extract::extract_entry_to(&mut entry, jar_path)?;
            return Ok(None);
        }
    }
    // Strategy 4: 兜底走官方清单下载（见 resolve_jar_from_manifest）
    match resolve_jar_from_manifest(source, dir_name, jar_path).await {
        Ok(()) => Ok(None),
        Err(e) => Ok(Some(e)),
    }
}

/// Strategy 4 的具体执行：官方版本清单 → 版本 JSON → downloads.client.url → 下载。
/// 任一环节失败即返回具体原因；成功写入 jar_path 后返回 Ok(())。
async fn resolve_jar_from_manifest(
    source: &DownloadSource,
    dir_name: &str,
    jar_path: &Path,
) -> Result<(), String> {
    let manifest_url = format!("{}/mc/game/version_manifest_v2.json", source.manifest_url);
    let text = http()
        .get(&manifest_url)
        .send()
        .await
        .map_err(common_err_ctx_key!(
            "errors.download.fetch",
            "拉取版本清单失败"
        ))?
        .text()
        .await
        .map_err(common_err_ctx_key!(
            "errors.version.read",
            "读取版本清单失败"
        ))?;
    let manifest: serde_json::Value = serde_json::from_str(&text).map_err(common_err_ctx_key!(
        "errors.version.parse",
        "解析版本清单失败"
    ))?;
    let versions = manifest["versions"].as_array().ok_or_else(|| {
        keyed_msg!(
            "errors.loader.manifestNoVersions",
            "版本清单缺 versions 数组"
        )
    })?;
    // ponytail: mc_ver 由目录名连字符首段猜测，版本号含连字符时会错；应传 mc_version
    let mc_ver = dir_name.split('-').next().unwrap_or(dir_name);
    let entry = versions
        .iter()
        .find(|v| v["id"].as_str() == Some(mc_ver))
        .ok_or_else(|| {
            keyed_msg!(
                "errors.loader.manifestVersionMissing?mc_ver={mc_ver}",
                "版本清单未收录 {mc_ver}"
            )
        })?;
    let url = entry["url"]
        .as_str()
        .ok_or_else(|| keyed_msg!("errors.loader.manifestEntryNoUrl", "版本条目缺 url"))?;
    let url = source.mirror_url(url);
    let vj = http()
        .get(&url)
        .send()
        .await
        .map_err(common_err_ctx_key!(
            "errors.download.fetch",
            "拉取版本 JSON 失败"
        ))?
        .text()
        .await
        .map_err(common_err_ctx_key!(
            "errors.version.read",
            "读取版本 JSON 失败"
        ))?;
    let vj_val: serde_json::Value = serde_json::from_str(&vj).map_err(common_err_ctx_key!(
        "errors.version.parse",
        "解析版本 JSON 失败"
    ))?;
    let dl_url = vj_val
        .pointer("/downloads/client/url")
        .and_then(|u| u.as_str())
        .map(|u| source.mirror_url(u))
        .ok_or_else(|| {
            keyed_msg!(
                "errors.loader.versionJsonNoClientUrl",
                "版本 JSON 缺 downloads.client.url"
            )
        })?;
    fetcher::fetch(&dl_url, jar_path, None::<fn(u64, u64)>)
        .await
        .map_err(common_err_ctx_key!(
            "errors.download.failed",
            "下载 client.jar 失败"
        ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// legacy 安装器内嵌构件：按 `maven/` 相对路径解到 `libraries/`，并按声明 sha1 幂等。
    #[test]
    fn embedded_maven_extracted_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("libraries");
        std::fs::create_dir_all(&lib_dir).unwrap();
        let rel = "net/minecraftforge/forge/1.12.2-14.23.5.2864/forge-1.12.2-14.23.5.2864.jar";
        let content = b"legacy-universal-jar-bytes";
        let sha1 = crate::downloader::hash::sha1_hex(content);

        // 假 installer：内嵌 maven/<rel> + profile 声明其 sha1
        let installer = dir.path().join("forge-installer.jar");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&installer).unwrap());
            z.start_file(
                format!("maven/{rel}"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            std::io::Write::write_all(&mut z, content).unwrap();
            z.finish().unwrap();
        }
        let profile: serde_json::Value = serde_json::from_str(&format!(
            r#"{{ "libraries": [ {{ "name": "net.minecraftforge:forge:1.12.2-14.23.5.2864",
                 "downloads": {{ "artifact": {{ "path": "{rel}", "sha1": "{sha1}" }} }} }} ] }}"#
        ))
        .unwrap();

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&installer).unwrap()).unwrap();
        install_embedded_maven(&mut archive, &profile, &lib_dir).unwrap();
        let dest = lib_dir.join(rel);
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            content,
            "应解出声明路径下的构件"
        );

        // 幂等：删掉后内容被改动（sha1 不匹配）→ 重新解出正确内容
        std::fs::write(&dest, b"corrupted").unwrap();
        install_embedded_maven(&mut archive, &profile, &lib_dir).unwrap();
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            content,
            "sha1 不匹配（损坏）时应重新解出"
        );
    }

    /// spec-0 `install` 段：zip 根的 `filePath` 按 `path` 坐标落位。
    #[test]
    fn install_section_file_placed_by_coordinate() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("libraries");
        let installer = dir.path().join("forge-installer.jar");
        let bytes = b"universal-jar-bytes";
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&installer).unwrap());
            z.start_file(
                "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            std::io::Write::write_all(&mut z, bytes).unwrap();
            z.finish().unwrap();
        }
        let profile: serde_json::Value = serde_json::from_str(
            r#"{ "install": { "path": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
                 "filePath": "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar" } }"#,
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&installer).unwrap()).unwrap();
        install_profile_file(&mut archive, &profile, &lib_dir).unwrap();
        let dest = lib_dir
            .join("net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10")
            .join("forge-1.7.10-10.13.4.1614-1.7.10.jar");
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            bytes,
            "应按坐标落位为普通构件名"
        );

        // 无 install 段（新版）→ 空转
        let mut a2 = zip::ZipArchive::new(std::fs::File::open(&installer).unwrap()).unwrap();
        install_profile_file(&mut a2, &serde_json::json!({}), &lib_dir).unwrap();
    }

    /// 真实 installer 验证（真实网络，默认 `#[ignore]`）：spec-0 老代（MC 1.7.10）maven 上无普通构件，
    /// 靠 `install.filePath` 提供，须落位成普通坐标。运行：`cargo test --lib -- --ignored forge_1710_install_section --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn forge_1710_install_section_from_real_installer() {
        use crate::config::downloader::DownloaderConfig;
        use crate::downloader::fetcher;

        const MC_VERSION: &str = "1.7.10";
        const FORGE_VERSION: &str = "10.13.4.1614-1.7.10";

        let cfg = DownloaderConfig::default();
        let dir = std::env::temp_dir().join("elytra_e2e_forge_1710");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let installer = dir.join("forge-installer.jar");
        let source = cfg.file_primary_source().expect("无可用下载源");
        let url = source.url(
            &source.forge_installer_url,
            &[("mc", MC_VERSION), ("ver", FORGE_VERSION)],
        );
        fetcher::fetch_fallback(&[url], &installer, None::<fn(u64, u64)>)
            .await
            .expect("下载 Forge 1.7.10 installer 失败（URL 模板对老代是否可用）");

        let profile_json =
            processors::read_profile_from_installer(&installer).expect("读 install_profile 失败");
        let profile: serde_json::Value =
            serde_json::from_str(&profile_json).expect("解析 install_profile 失败");
        assert!(
            profile["versionInfo"].is_object(),
            "1.7.10 是 spec-0：版本 json 在 versionInfo 里"
        );
        assert_eq!(
            profile["install"]["filePath"].as_str(),
            Some("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar")
        );

        let lib_dir = dir.join("libraries");
        std::fs::create_dir_all(&lib_dir).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&installer).unwrap()).unwrap();
        install_profile_file(&mut archive, &profile, &lib_dir).unwrap();

        let dest = lib_dir
            .join("net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10")
            .join("forge-1.7.10-10.13.4.1614-1.7.10.jar");
        let size = dest
            .metadata()
            .unwrap_or_else(|e| panic!("未落位 {}：{e}", dest.display()))
            .len();
        assert!(size > 1_000_000, "落位的构件过小：{size}");
        // 与 zip 根的同名 universal 字节一致
        let mut z = zip::ZipArchive::new(std::fs::File::open(&installer).unwrap()).unwrap();
        let mut src = Vec::new();
        std::io::Read::read_to_end(
            &mut z
                .by_name("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar")
                .unwrap(),
            &mut src,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            src,
            "落位内容应与 zip 内文件一致"
        );
        eprintln!(
            "✅ 1.7.10 spec-0 install 段落位：{}（{size} 字节）",
            dest.display()
        );
    }

    /// 真实 installer 验证（真实网络，默认 `#[ignore]`）：Forge 1.12.2 版本 json 里 `net.minecraftforge:forge`
    /// 库 URL 为空，构件内嵌在 installer 的 maven/ 下。运行：`cargo test --lib -- --ignored forge_1122_embedded_maven --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn forge_1122_embedded_maven_from_real_installer() {
        use crate::config::downloader::DownloaderConfig;
        use crate::downloader::fetcher;

        const MC_VERSION: &str = "1.12.2";
        const FORGE_VERSION: &str = "14.23.5.2864";

        let cfg = DownloaderConfig::default();
        let dir = std::env::temp_dir().join("elytra_e2e_forge_legacy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let installer = dir.join("forge-installer.jar");
        let source = cfg.file_primary_source().expect("无可用下载源");
        let url = source.url(
            &source.forge_installer_url,
            &[("mc", MC_VERSION), ("ver", FORGE_VERSION)],
        );
        fetcher::fetch_fallback(&[url], &installer, None::<fn(u64, u64)>)
            .await
            .expect("下载 Forge 1.12.2 installer 失败");

        let profile_json =
            processors::read_profile_from_installer(&installer).expect("读 install_profile 失败");
        let profile: serde_json::Value =
            serde_json::from_str(&profile_json).expect("解析 install_profile 失败");
        assert!(
            processors::parse_profile_value(&profile)
                .expect("解析 profile 失败")
                .processors
                .is_empty(),
            "1.12.2 是 spec-1 legacy：应无 processors（引擎据此跳过 patching）"
        );

        let lib_dir = dir.join("libraries");
        std::fs::create_dir_all(&lib_dir).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&installer).unwrap()).unwrap();
        install_embedded_maven(&mut archive, &profile, &lib_dir).unwrap();

        // 断言：version.json 里 URL 为空的 forge client 库被真正解出且 sha1 命中
        let rel = format!(
            "net/minecraftforge/forge/{MC_VERSION}-{FORGE_VERSION}/forge-{MC_VERSION}-{FORGE_VERSION}.jar"
        );
        let dest = lib_dir.join(&rel);
        let size = dest
            .metadata()
            .unwrap_or_else(|e| panic!("未解出 {rel}：{e}"))
            .len();
        assert!(size > 1_000_000, "解出的构件过小：{size}");
        let declared = declared_sha1(&profile, &rel);
        assert!(!declared.is_empty(), "profile 应声明该构件的 sha1");
        assert!(
            crate::downloader::hash::file_sha1_matches(&dest, &declared),
            "解出的构件应匹配 profile 声明的 sha1"
        );
        eprintln!("✅ 1.12.2 legacy 内嵌构件已解出：{rel}（{size} 字节）");
    }

    /// Auto 模式须按目标版本声明的 major 选 JDK 而非缓存首项（列表刻意 JDK8 在前：装 1.20.1 选首项会以 Java 8 跑链 → UnsupportedClassVersionError）。
    #[test]
    fn auto_jdk_prefers_required_major_not_cache_first() {
        let mk = |path: &str, version: &str| jdk::model::JdkInfo {
            path: path.into(),
            version: version.into(),
            vendor: "OpenJDK".into(),
            bits: 64,
        };
        let jdks = vec![
            mk("/jdk8", "1.8.0_202"),
            mk("/jdk17", "17.0.8"),
            mk("/jdk21", "21"),
        ];

        assert_eq!(
            pick_auto_jdk(&jdks, 17).unwrap().path,
            "/jdk17",
            "需要 17 时必须选 JDK 17，而非缓存首项 JDK 8"
        );
        assert_eq!(pick_auto_jdk(&jdks, 21).unwrap().path, "/jdk21");
        assert_eq!(
            pick_auto_jdk(&jdks, 8).unwrap().path,
            "/jdk8",
            "需要 8 时精确命中"
        );
        // 未声明版本（0）→ 保持旧行为：缓存首项
        assert_eq!(pick_auto_jdk(&jdks, 0).unwrap().path, "/jdk8");
        // 缓存为空 → None（resolve_java 回退 JAVA_HOME）
        assert!(pick_auto_jdk(&[], 17).is_none());
    }
}
