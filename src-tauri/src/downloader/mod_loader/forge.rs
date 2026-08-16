// ── Installer 式安装（Forge / NeoForge 共用流程）：官方 installer.jar 下载 + ZIP 解析 ──
// NeoForge 专属逻辑（install_neoforge / universal / patched client）在 neoforge.rs。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher::{self, http};

use super::neoforge::{download_neoforge_universal, extract_neoforge_client};
use super::{expand_inheritance, LoaderEvent, LoaderHook};

#[allow(clippy::too_many_arguments)]
pub async fn install_forge(
    mc_version: &str,
    forge_version: &str,
    dir_name: &str,
    vdir: &Path,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &Arc<AtomicU64>,
    lib_bytes: &Arc<AtomicU64>,
    lib_total: &Arc<AtomicU64>,
    cancel: &Arc<AtomicBool>,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    // 安装程序也走多源回退：官方 maven 对国内网络不稳（连接重置/超时），
    // 按 file_source_mode 的完整源顺序各构造一个 URL，fetch_fallback 依次尝试
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
    download_and_extract(
        &installer_urls,
        source,
        config,
        vdir,
        dir_name,
        parent_text,
        bytes,
        lib_bytes,
        lib_total,
        cancel,
        hook,
    )
    .await
}

/// 10 参数均为必需数据（URL/源/配置/目标/字节计数/阶段钩子），私有实现细节，不抽象。
/// Forge 与 NeoForge 共用：version.json/jar 解析后按分支处理各自专属构件。
#[allow(clippy::too_many_arguments)]
pub(super) async fn download_and_extract(
    installer_urls: &[String],
    source: &DownloadSource,
    config: &DownloaderConfig,
    version_dir: &Path,
    dir_name: &str,
    parent_text: &str,
    bytes: &Arc<AtomicU64>,
    lib_bytes: &Arc<AtomicU64>,
    lib_total: &Arc<AtomicU64>,
    cancel: &Arc<AtomicBool>,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let mc_dir = config.minecraft_dir_abs();
    let tmp = mc_dir.join(".elytra").join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败\n{}", e))?;
    let installer_name = installer_urls
        .first()
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or("installer.jar");
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
    fetcher::fetch_fallback(installer_urls, &installer_path, Some(progress))
        .await
        .map_err(|e| format!("下载安装程序失败\n{}", e))?;

    let file =
        std::fs::File::open(&installer_path).map_err(|e| format!("打开安装程序失败\n{}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取安装程序失败\n{}", e))?;

    std::fs::create_dir_all(version_dir).map_err(|e| format!("创建目录失败\n{}", e))?;
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
    let found_jar = resolve_jar(
        &mut archive,
        &profile,
        source,
        &jar_path,
        dir_name,
        &json_path,
    )
    .await?;

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

    // ── 下载展开后 JSON 的全部库（vanilla 库已存在自动跳过，实际拉取 ModLoader 增量）──
    // Forge/NeoForge 26.x 的本体（universal/fancymodloader 等）以 library 形式进 classpath，
    // 安装阶段必须一并下载，否则启动时缺库。
    let lib_dir = mc_dir.join("libraries");
    let json_text =
        std::fs::read_to_string(&json_path).map_err(|e| format!("读取 version.json 失败\n{e}"))?;
    let merged: serde_json::Value =
        serde_json::from_str(&json_text).map_err(|e| format!("解析 version.json 失败\n{e}"))?;
    let libraries: Vec<crate::downloader::model::Library> =
        serde_json::from_value(merged["libraries"].clone()).unwrap_or_default();
    // Forge 的 client 合并 jar（79MB）在 installer 的 data/client.lzma（LZMA 压缩）里，
    // version.json 中该库 URL 为空：经官方 binarypatcher 生成（vanilla client + client.lzma），
    // NeoForge 无此文件跳过。
    extract_forge_client_lzma(
        &mut archive,
        &libraries,
        &lib_dir,
        &mc_dir,
        dir_name,
        config,
        cancel,
    )
    .await?;
    // NeoForge：install_profile.libraries 含 `net.neoforged:neoforge:{ver}:universal`
    // （不在 version.json）。官方部署中 universal 不进 classpath——GameLocator production
    // 分支从 libraries 目录加载它；放进 classpath 会因"找到 NeoForge 类而无 MC 类"
    // 误报 "The patched Minecraft jar is missing"。这里仅下载文件到 libraries。
    // 同时运行 installertools PROCESS_MINECRAFT_JAR 生成 minecraft-client-patched
    // （GameLocator 新安装方式优先使用它，避免缺 client-srg.jar 报 corrupted）。
    if let Some(uni) = profile.as_ref().and_then(|p| {
        p["libraries"].as_array().and_then(|a| {
            a.iter().find(|l| {
                l["name"].as_str().is_some_and(|n| {
                    n.starts_with("net.neoforged:neoforge:") && n.ends_with(":universal")
                })
            })
        })
    }) {
        let neo_version = uni["name"]
            .as_str()
            .and_then(|n| n.split(':').nth(2))
            .unwrap_or("")
            .to_string();
        if neo_version.is_empty() {
            return Err("NeoForge universal 坐标缺少版本号".into());
        }
        download_neoforge_universal(uni, &lib_dir, config).await?;
        // 运行 installertools 生成 patched（前置：等待 vanilla 下载完成）
        extract_neoforge_client(
            &mut archive,
            &libraries,
            &lib_dir,
            &mc_dir,
            dir_name,
            &neo_version,
            config,
            cancel,
        )
        .await?;
        // 其余库走统一下载（universal 已单独下载，JSON 无其条目 → 不进 classpath）
        let sources: Vec<&DownloadSource> = config.file_sources_list();
        let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
        crate::downloader::library::download_libraries(
            &libraries,
            &lib_dir,
            config.concurrency as usize,
            &sources,
            None, // loader 安装内库下载不走复用（run_task 主流程才探测复用根）
            counter,
            bytes.clone(),
            lib_bytes.clone(),
            None, // 安装器内库下载小且少，取消由外层步骤检查兜底
        )
        .await
        .map_err(|e| format!("下载 {} 支持库失败:\n{}", dir_name, e.join("\n")))?;
        if let Some(h) = hook {
            h(LoaderEvent::LibsDone);
        }
        return Ok(());
    }
    // 其余库走统一下载（多源回退 + sha1 校验；URL 为空的库在此被跳过）
    let sources: Vec<&DownloadSource> = config.file_sources_list();
    let counter = Arc::new(std::sync::atomic::AtomicU32::new(0));
    crate::downloader::library::download_libraries(
        &libraries,
        &lib_dir,
        config.concurrency as usize,
        &sources,
        None, // 同上：loader 安装内库下载不走复用
        counter,
        bytes.clone(),
        lib_bytes.clone(),
        None, // 同上
    )
    .await
    .map_err(|e| format!("下载 {} 支持库失败:\n{}", dir_name, e.join("\n")))?;

    if let Some(h) = hook {
        h(LoaderEvent::LibsDone);
    }
    Ok(())
}

/// Forge 26.x：用官方 binarypatcher 生成 forge client jar（79MB）。
/// 流程与 install_profile 的 processors 一致：vanilla client + `data/client.lzma`
/// （LZMA 压缩补丁）经官方工具合成 → 写入 libraries 中 `forge:{ver}:client` 路径。
/// 需要 JDK（与启动游戏同一套）运行官方工具；NeoForge/旧版无该库时静默跳过。
async fn extract_forge_client_lzma(
    archive: &mut zip::ZipArchive<std::fs::File>,
    libraries: &[crate::downloader::model::Library],
    lib_dir: &Path,
    mc_dir: &Path,
    dir_name: &str,
    config: &crate::config::downloader::model::DownloaderConfig,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    // 定位 URL 为空的 forge client 库（其 path/sha1 由 version.json 给出）
    let Some(art) = libraries.iter().find_map(|l| {
        let a = l.downloads.as_ref()?.artifact.as_ref()?;
        if a.url.is_empty() && l.name.contains(":forge:") {
            Some(a)
        } else {
            None
        }
    }) else {
        return Ok(());
    };
    let dest = lib_dir.join(&art.path);
    // 幂等跳过：输出（client 合并 jar）已存在且 sha1 匹配 → 不运行补丁工具。
    // 声明式输出校验在运行前完成（processor_outputs_are_current），
    // 重装/修复场景直接进下一步，不再 spawn JVM。
    if super::processor_outputs_are_current(&[(&dest, Some(&art.sha1))]) {
        return Ok(());
    }

    // 1. 从 installer 提取 data/client.lzma
    let tmp = mc_dir.join(".elytra").join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败\n{e}"))?;
    let client_lzma = tmp.join("client.lzma");
    {
        let mut found = false;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("读取安装程序条目失败\n{e}"))?;
            if !entry.name().ends_with("client.lzma") {
                continue;
            }
            let mut out =
                std::fs::File::create(&client_lzma).map_err(|e| format!("创建文件失败\n{e}"))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("提取 client.lzma 失败\n{e}"))?;
            found = true;
            break;
        }
        if !found {
            return Ok(()); // 无补丁文件（NeoForge 等），跳过
        }
    }

    // 2. 等待 vanilla client 就绪（runner 的 ClientJar 步骤与之并行，轮询至多 120s）
    // 注意：不能只看大小 >1MB——部分写入的文件也会超过该阈值，zip 损坏会导致工具退出码 1。
    // 稳定检测：连续 3 次（间隔 500ms）大小不变即视为下载完成。
    let vanilla_jar = wait_for_vanilla_client(mc_dir, dir_name, cancel).await?;

    // 3. 下载官方工具（binarypatcher + classpath 依赖，来自 maven.minecraftforge.net）
    let tools = tmp.join("forge-tools");
    std::fs::create_dir_all(&tools).map_err(|e| format!("创建目录失败\n{e}"))?;
    const TOOL_JARS: &[(&str, &str)] = &[
        (
            "binarypatcher",
            "net/minecraftforge/binarypatcher/1.3.4/binarypatcher-1.3.4.jar",
        ),
        (
            "hash-utils",
            "net/minecraftforge/hash-utils/0.2.3/hash-utils-0.2.3.jar",
        ),
        (
            "srgutils",
            "net/minecraftforge/srgutils/0.6.6/srgutils-0.6.6.jar",
        ),
        (
            "jopt-simple",
            "net/sf/jopt-simple/jopt-simple/6.0-alpha-3/jopt-simple-6.0-alpha-3.jar",
        ),
        (
            "lzma-java",
            "com/github/jponge/lzma-java/1.3/lzma-java-1.3.jar",
        ),
        (
            "javaxdelta",
            "com/nothome/javaxdelta/2.0.1/javaxdelta-2.0.1.jar",
        ),
        ("trove", "trove/trove/1.0.2/trove-1.0.2.jar"),
    ];
    let sources: Vec<&crate::config::downloader::model::DownloadSource> =
        config.file_sources_list();
    for (name, maven_path) in TOOL_JARS {
        let tool_dest = tools.join(format!("{name}.jar"));
        if tool_dest.exists() {
            continue;
        }
        let urls: Vec<String> = sources
            .iter()
            .map(|s| s.mirror_url(&format!("https://maven.minecraftforge.net/{maven_path}")))
            .collect();
        crate::downloader::fetcher::fetch_fallback(&urls, &tool_dest, None::<fn(u64, u64)>)
            .await
            .map_err(|e| format!("下载 {name} 工具失败\n{e}"))?;
    }

    // 4. 运行 binarypatcher（与 install_profile 的 client 侧 processor 参数一致）。
    // 轮询 cancel 标志：用户取消任务时 kill 工具进程，避免"取消后工具仍跑数十秒"。
    let java = resolve_java()?;
    let sep = if cfg!(windows) { ";" } else { ":" };
    let classpath = TOOL_JARS
        .iter()
        .map(|(n, _)| tools.join(format!("{n}.jar")).display().to_string())
        .collect::<Vec<_>>()
        .join(sep);
    let child = tokio::process::Command::new(&java)
        .args([
            "-cp",
            &classpath,
            "net.minecraftforge.binarypatcher.ConsoleTool",
            "--clean",
            &vanilla_jar.display().to_string(),
            "--output",
            &dest.display().to_string(),
            "--apply",
            &client_lzma.display().to_string(),
            "--data",
            "--unpatched",
            "--store",
            "--marker",
            ".forge_patched_minecraft",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 Forge 补丁工具失败\n{e}"))?;
    let output = crate::downloader::mod_loader::run_installer_command(
        child,
        cancel,
        "Forge 补丁工具",
    )
    .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(15).collect();
        let tail = tail.iter().rev().cloned().collect::<Vec<_>>().join("\n");
        return Err(format!(
            "Forge 补丁工具退出码 {:?}\n{}",
            output.status.code().unwrap_or(-1),
            tail
        ));
    }

    // 5. sha1 校验
    if !crate::downloader::hash::file_sha1_matches(&dest, &art.sha1) {
        let _ = std::fs::remove_file(&dest);
        return Err("Forge client.jar 生成后 sha1 校验失败".into());
    }
    Ok(())
}

/// 等待 runner 的 ClientJar 步骤下载完成（与 install 步骤并行）：
/// 不能只看大小>1MB（部分写入也会超阈值），连续 3 次（间隔 500ms）大小不变即视为完成。
/// 上限 120s，超时返回错误；任务取消时立即返回（kill 由调用方处理）。
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
            return Err("任务已取消".into());
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
    Err("等待原版客户端下载超时（120s），无法生成 ModLoader client".into())
}

/// 运行补丁工具所需的 Java 可执行文件（与游戏启动同一套 JDK 配置）。
pub(super) fn resolve_java() -> Result<std::path::PathBuf, String> {
    use crate::jdk::model::JdkSelection;
    let cfg = crate::jdk::read();
    match cfg.jdk_selection {
        JdkSelection::Manual { path } => Ok(crate::jdk::java_bin_path(std::path::Path::new(&path))),
        JdkSelection::Auto => {
            let jdk = cfg.discovered_jdks.first().ok_or(
                "未检测到 Java 环境，无法生成 Forge client（需要 JDK 运行官方补丁工具，请先在设置中配置 Java）",
            )?;
            Ok(crate::jdk::java_bin_path(std::path::Path::new(&jdk.path)))
        }
    }
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
            serde_json::to_string_pretty(&merged).map_err(|e| format!("序列化失败\n{}", e))?;
        std::fs::write(json_path, &out).map_err(|e| format!("写入 version.json 失败\n{}", e))
    };

    // Strategy 1: install_profile has "json" field → extract from ZIP
    if let Some(p) = profile {
        if let Some(json_path_in_zip) = p["json"].as_str() {
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| format!("读取安装程序条目失败\n{}", e))?;
                if entry.name() == json_path_in_zip.trim_start_matches('/') {
                    let mut buf = String::new();
                    entry
                        .read_to_string(&mut buf)
                        .map_err(|e| format!("读取 JSON 失败\n{}", e))?;
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
            .map_err(|e| format!("读取安装程序条目失败\n{}", e))?;
        let name = entry.name().to_string();
        if name.ends_with("version.json") {
            let mut buf = String::new();
            entry
                .read_to_string(&mut buf)
                .map_err(|e| format!("读取 JSON 失败\n{}", e))?;
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&buf) {
                write_extracted(v)?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Resolve client jar from ZIP, maven download, or fallback to original manifest.
async fn resolve_jar(
    archive: &mut zip::ZipArchive<std::fs::File>,
    profile: &Option<serde_json::Value>,
    source: &DownloadSource,
    jar_path: &Path,
    dir_name: &str,
    json_path: &Path,
) -> Result<bool, String> {
    // 新版 Forge（26.x+）：install_profile 无 install 字段，ZIP 里只有 shim/universal（属 library，
    // 进 classpath 不进版本目录）；client.jar 由 version.json 的 downloads.client（继承 vanilla）提供，
    // runner 的 ClientJar 步骤下载。此时不需要（也不应）提取任何 Forge jar 覆盖 client。
    if let Some(p) = profile {
        if p.get("install").is_none() {
            if let Ok(json) = std::fs::read_to_string(json_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                    if v.pointer("/downloads/client").is_some() {
                        return Ok(true);
                    }
                }
            }
        }
    }

    // Strategy 1: install_profile → install.filePath → extract from ZIP
    if let Some(p) = profile {
        if let Some(fp) = p.pointer("/install/filePath").and_then(|v| v.as_str()) {
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| format!("读取安装程序条目失败\n{}", e))?;
                if entry.name() == fp {
                    crate::downloader::extract::extract_entry_to(&mut entry, jar_path)?;
                    return Ok(true);
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
            // Forge 构件在 maven.minecraftforge.net（source.maven_url 是 Mojang 库源，不适用）；
            // 镜像源经 mirror_url 重写到 BMCLAPI /maven/
            let jar_url = source.mirror_url(&format!("https://maven.minecraftforge.net/{path}"));
            return Ok(fetcher::fetch(&jar_url, jar_path, None::<fn(u64, u64)>)
                .await
                .is_ok());
        }
    }

    // Strategy 3: scan ZIP for any .jar entry (exclude installer/sources/javadoc)
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取安装程序条目失败\n{}", e))?;
        let name = entry.name().to_string();
        if name.ends_with(".jar")
            && !name.contains("installer")
            && !name.contains("sources")
            && !name.contains("javadoc")
        {
            crate::downloader::extract::extract_entry_to(&mut entry, jar_path)?;
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
