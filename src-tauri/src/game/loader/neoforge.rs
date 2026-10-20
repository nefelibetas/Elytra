// ── NeoForge 安装（installer 式，与 Forge 共用 download_and_extract 流程）──

use std::path::Path;

use super::forge::{download_and_extract, wait_for_vanilla_client};
use super::processors;
use super::{InstallCtx, VersionTarget};
use crate::config::downloader::{DownloadSource, DownloaderConfig};
use crate::downloader::hash;
use crate::downloader::{endpoints, fetcher};

pub async fn install_neoforge(
    neo_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let InstallCtx { config, .. } = ctx;
    // 多源回退：官方 maven 对国内网络不稳，按完整源顺序各构造 URL，fetch_fallback 依次尝试
    let installer_urls: Vec<String> = config
        .file_sources_list()
        .iter()
        .map(|s| s.url(&s.neoforge_installer_url, &[("ver", neo_version)]))
        .collect();
    download_and_extract(&installer_urls, target, parent_text, ctx).await
}

/// NeoForge 的 universal jar：install_profile.libraries 才有（version.json 无）。它不进启动 classpath——GameLocator
/// production 分支经 -DlibraryDirectory 从 libraries 直接加载；若进 classpath，RequiredSystemFiles 会找到 NeoForge 类而缺
/// MC 类 → 误报 "The patched Minecraft jar is missing"。故仅下载文件到 libraries，不写进展开 JSON。
pub(super) async fn download_neoforge_universal(
    uni: &serde_json::Value,
    lib_dir: &Path,
    config: &DownloaderConfig,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    let name = uni["name"].as_str().ok_or("NeoForge universal 坐标缺失")?;
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 4 {
        bail_key!("errors.install.loader", "NeoForge universal 坐标格式异常\n{name}");
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
    // 期望 sha1 由 install_profile 的 universal 条目声明；跳过判断按 sha1 命中（否则镜像截断的 >1MB 半成品会被永久接受）
    let sha1 = uni["downloads"]["artifact"]["sha1"]
        .as_str()
        .unwrap_or_default();
    if !sha1.is_empty() && hash::file_sha1_matches(&dest, sha1) {
        return Ok(());
    }
    if sha1.is_empty()
        && dest
            .metadata()
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false)
    {
        return Ok(());
    }
    let sources: Vec<&DownloadSource> = config.file_sources_list();
    let urls: Vec<String> = sources
        .iter()
        .map(|s| s.mirror_url(&format!("{}/releases/{}", endpoints::NEOFORGE_MAVEN, rel)))
        .collect();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    }
    if sha1.is_empty() {
        fetcher::fetch_fallback_c(&urls, &dest, None::<fn(u64, u64)>, Some(cancel))
            .await
            .map_err(common_err_ctx_key!("errors.install.loader", "下载 NeoForge universal 失败"))?;
    } else {
        fetcher::fetch_verified_c(&urls, &dest, None::<fn(u64, u64)>, sha1, Some(cancel))
            .await
            .map_err(common_err_ctx_key!("errors.install.loader", "下载 NeoForge universal 失败"))?;
    }
    Ok(())
}

/// NeoForge：按 installer 声明的 `install_profile.processors` 执行 client 侧安装，产物路径取自 profile 的
/// `{PATCHED}`。**不能写死配方**（工具与补丁包格式逐代变化，写死某代会报 `Invalid bundle signature`）。详见 [`super::processors`]。
pub(super) async fn extract_neoforge_client(
    archive: &mut zip::ZipArchive<std::fs::File>,
    installer_path: &Path,
    lib_dir: &Path,
    mc_dir: &Path,
    dir_name: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let InstallCtx { config, cancel, .. } = ctx;

    // 1. 读 installer 内的 install_profile.json
    let profile_text = {
        let mut f = archive
            .by_name("install_profile.json")
            .map_err(common_err_ctx!("安装程序缺少 install_profile.json"))?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut f, &mut text)
            .map_err(common_err_ctx_key!("errors.install.loader", "读取 install_profile.json 失败"))?;
        text
    };
    let profile = processors::parse_profile(&profile_text)?;
    if profile.processors.is_empty() {
        // 1.12- 的老式 installer 用 install/versionInfo 字段，不在 processors 体系内
        return Ok(());
    }

    // 2. 幂等：`{PATCHED}` 完好（完整 zip 且 ≥1MB）则跳过整条链（省数十秒 JVM 开销）；半成品/截断不算完好，会被重跑覆盖
    let patched = processors::output_path(&profile, "PATCHED", lib_dir, mc_dir)?;
    if processors::intact_archive(&patched) {
        return Ok(());
    }

    // 3. 等 vanilla client 就绪（runner 的 ClientJar 步骤与之并行）
    let vanilla_jar = wait_for_vanilla_client(mc_dir, dir_name, cancel).await?;

    // 4. 按声明跑完整条处理器链
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
        bail_key!("errors.install.loader", 
            "NeoForge 安装链执行完毕但未生成客户端\n{}",
            patched.display()
        );
    }
    Ok(())
}
