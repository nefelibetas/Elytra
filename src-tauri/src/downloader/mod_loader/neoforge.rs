// ── NeoForge 安装（installer 式，与 Forge 共用 download_and_extract 流程）──

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

use super::forge::{download_and_extract, resolve_java, wait_for_vanilla_client};
use super::LoaderHook;
use crate::config::downloader::model::DownloaderConfig;

#[allow(clippy::too_many_arguments)]
pub async fn install_neoforge(
    mc_version: &str,
    neo_version: &str,
    parent_text: &str,
    config: &DownloaderConfig,
    bytes: &Arc<AtomicU64>,
    lib_bytes: &Arc<AtomicU64>,
    lib_total: &Arc<AtomicU64>,
    cancel: &Arc<AtomicBool>,
    hook: Option<LoaderHook<'_>>,
) -> Result<(), String> {
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();
    let dir_name = format!("{}-neoforge-{}", mc_version, neo_version);
    let vdir = mc.join("versions").join(&dir_name);
    // 安装程序也走多源回退：官方 maven 对国内网络不稳（连接重置/超时），
    // 按 file_source_mode 的完整源顺序各构造一个 URL，fetch_fallback 依次尝试
    let installer_urls: Vec<String> = config
        .file_sources_list()
        .iter()
        .map(|s| s.url(&s.neoforge_installer_url, &[("ver", neo_version)]))
        .collect();
    download_and_extract(
        &installer_urls,
        source,
        config,
        &vdir,
        &dir_name,
        parent_text,
        bytes,
        lib_bytes,
        lib_total,
        cancel,
        hook,
    )
    .await
}

/// NeoForge 的 universal jar：install_profile.libraries 才有（version.json 无）。
/// 官方部署中它不进启动 classpath——GameLocator production 分支经 -DlibraryDirectory
/// 从 libraries 目录直接加载（作为 mod 加入 pipeline）。若放进 classpath，
/// RequiredSystemFiles 会找到 NeoForge 类而缺 MC 类 → 误报 "The patched Minecraft jar is missing"。
/// 因此这里仅下载文件到 libraries 目录，不写进展开 JSON。
pub(super) async fn download_neoforge_universal(
    uni: &serde_json::Value,
    lib_dir: &Path,
    config: &DownloaderConfig,
) -> Result<(), String> {
    let name = uni["name"].as_str().ok_or("NeoForge universal 坐标缺失")?;
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 4 {
        return Err(format!("NeoForge universal 坐标格式异常\n{name}"));
    }
    let (group, artifact, version, classifier) = (parts[0], parts[1], parts[2], parts[3]);
    let file = format!("{artifact}-{version}-{classifier}.jar");
    let rel = format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file
    );
    let dest = lib_dir.join(&rel);
    if dest.exists()
        && dest
            .metadata()
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false)
    {
        return Ok(());
    }
    let sources: Vec<&crate::config::downloader::model::DownloadSource> =
        config.file_sources_list();
    let urls: Vec<String> = sources
        .iter()
        .map(|s| s.mirror_url(&format!("https://maven.neoforged.net/releases/{rel}")))
        .collect();
    crate::downloader::fetcher::fetch_fallback(&urls, &dest, None::<fn(u64, u64)>)
        .await
        .map_err(|e| format!("下载 NeoForge universal 失败\n{e}"))?;
    Ok(())
}

/// NeoForge 26.x：运行 installertools `PROCESS_MINECRAFT_JAR` 生成
/// `libraries/net/neoforged/minecraft-client-patched/{ver}/minecraft-client-patched-{ver}.jar`
/// （GameLocator 新安装方式优先使用它；旧方式缺 client-srg.jar 会报 corrupted）。
/// 输入：vanilla client + installer 内 `data/client.lzma`（工具内部 LZMA 解压，与 Forge 相同）。
#[allow(clippy::too_many_arguments)]
pub(super) async fn extract_neoforge_client(
    archive: &mut zip::ZipArchive<std::fs::File>,
    _libraries: &[crate::downloader::model::Library],
    lib_dir: &Path,
    mc_dir: &Path,
    dir_name: &str,
    neo_version: &str,
    config: &DownloaderConfig,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let patched_name = format!("minecraft-client-patched-{neo_version}.jar");
    let dest = lib_dir
        .join("net/neoforged/minecraft-client-patched")
        .join(neo_version)
        .join(&patched_name);
    // 幂等跳过：patched jar 无公开 sha1，按"存在且 ≥1MB"粗校验（processor_outputs_are_current），
    // 重装/修复场景不重新运行 installertools（避免杀软误报 + 数秒～数十秒 JVM 开销）。
    if super::processor_outputs_are_current(&[(&dest, None)]) {
        return Ok(());
    }

    // 1. 提取 data/client.lzma
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
            return Ok(());
        }
    }

    // 2. 等待 vanilla client 就绪（runner 的 ClientJar 步骤与之并行，轮询至多 120s）
    // 稳定检测：连续 3 次（间隔 500ms）大小不变即视为下载完成（部分写入会损坏 zip）。
    let vanilla_jar = wait_for_vanilla_client(mc_dir, dir_name, cancel).await?;

    // 3. 下载 installertools fatjar（maven.neoforged.net + 镜像）
    let tools = tmp.join("neoforge-tools");
    std::fs::create_dir_all(&tools).map_err(|e| format!("创建目录失败\n{e}"))?;
    let tool_path =
        "net/neoforged/installertools/installertools/4.0.12/installertools-4.0.12-fatjar.jar";
    let tool_dest = tools.join("installertools-fatjar.jar");
    if !tool_dest.exists() {
        let sources: Vec<&crate::config::downloader::model::DownloadSource> =
            config.file_sources_list();
        let urls: Vec<String> = sources
            .iter()
            .map(|s| s.mirror_url(&format!("https://maven.neoforged.net/releases/{tool_path}")))
            .collect();
        crate::downloader::fetcher::fetch_fallback(&urls, &tool_dest, None::<fn(u64, u64)>)
            .await
            .map_err(|e| format!("下载 installertools 工具失败\n{e}"))?;
    }

    // 4. 运行 PROCESS_MINECRAFT_JAR（轮询取消：cancel 置位时 kill 工具进程）
    let java = resolve_java()?;
    let child = tokio::process::Command::new(&java)
        .args([
            "-cp",
            &tool_dest.display().to_string(),
            "net.neoforged.installertools.ConsoleTool",
            "--task",
            "PROCESS_MINECRAFT_JAR",
            "--no-mod-manifest",
            "--input",
            &vanilla_jar.display().to_string(),
            "--output",
            &dest.display().to_string(),
            "--extract-libraries-to",
            &lib_dir.display().to_string(),
            "--apply-patches",
            &client_lzma.display().to_string(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 NeoForge 补丁工具失败\n{e}"))?;
    let output = super::run_installer_command(child, cancel, "NeoForge 补丁工具").await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(15).collect();
        let tail = tail.iter().rev().cloned().collect::<Vec<_>>().join("\n");
        return Err(format!(
            "NeoForge 补丁工具退出码 {:?}\n{}",
            output.status.code().unwrap_or(-1),
            tail
        ));
    }
    if !dest.exists() || dest.metadata().map(|m| m.len() < 1_000_000).unwrap_or(true) {
        return Err("NeoForge patched client 生成失败".into());
    }
    Ok(())
}
