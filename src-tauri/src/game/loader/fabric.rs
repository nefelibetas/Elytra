// ── Profile 式安装（Fabric / Quilt）：官方 meta profile JSON → merge_loader_profile 合并 ──

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::{merge_loader_profile, InstallCtx, LoaderEvent, VersionTarget};
use crate::config::downloader::DownloadSource;
use crate::downloader::fetcher::http;
use crate::game::resource::library;
use crate::game::version::model;

/// `dir_name`/`vdir` 由 runner 的目标目录解析传入（含冲突重命名 `{id}_n`）——
/// 安装器自行计算会与 runner 的目标分裂：冲突场景 json/jar 写到原始目录而
/// 新目录缺 json（启动报"未找到版本 JSON"）。
async fn install_loader_via_profile(
    mc_version: &str,
    loader_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    profile_url_tpl: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let VersionTarget { dir_name, vdir } = target;
    let InstallCtx { config, pc, hook, .. } = ctx;
    let source = config.file_primary_source().ok_or("no active source")?;
    let mc = config.minecraft_dir_abs();

    let profile_url = source.url(
        profile_url_tpl,
        &[("mc", mc_version), ("loaderVer", loader_version)],
    );
    let resp = http()
        .get(&profile_url)
        .send()
        .await
        .map_err(common_err_ctx!("获取 profile 失败"))?;
    let profile: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx!("解析 profile 失败"))?;

    let merged = merge_loader_profile(parent_text, &profile, dir_name, &source.maven_url)?;

    std::fs::create_dir_all(vdir).map_err(common_err_ctx!("创建目录失败"))?;
    let json_path = vdir.join(format!("{}.json", dir_name));
    let json_str =
        serde_json::to_string_pretty(&merged).map_err(common_err_ctx!("序列化失败"))?;
    std::fs::write(&json_path, &json_str).map_err(common_err_ctx!("写入 JSON 失败"))?;
    if let Some(h) = hook {
        h(LoaderEvent::ConfigDone);
    }

    let lib_dir = mc.join("libraries");
    let libraries: Vec<model::Library> =
        serde_json::from_value(merged["libraries"].clone()).unwrap_or_default();
    let sources: Vec<&DownloadSource> = config.file_sources_list();
    // LoaderLibs 步骤字节分母：需下载的 loader 依赖库总字节（本地扫描可算）
    let pending_bytes = library::pending_lib_bytes(&libraries, &lib_dir);
    if pending_bytes > 0 {
        pc.step_total.store(pending_bytes, Ordering::Relaxed);
    }
    // 文件计数保持静默（任务步骤不细分），字节计入外部计数器（收尾心跳显示速度/进度）
    let counter = Arc::new(AtomicU32::new(0));
    let mut pc_lib = pc.clone();
    pc_lib.files = counter;
    library::download_libraries(
        &libraries,
        &lib_dir,
        config.concurrency as usize,
        &sources,
        None, // loader 安装内库下载不走本地运行时复用（run_task 主流程才探测复用根）
        &pc_lib,
        None, // fabric 安装无取消检查（下载循环由 read_timeout 兜底）
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

    Ok(())
}

pub async fn install_fabric(
    mc_version: &str,
    loader_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let InstallCtx { config, .. } = ctx;
    let source = config.file_primary_source().ok_or("no active source")?;
    install_loader_via_profile(
        mc_version,
        loader_version,
        target,
        parent_text,
        &source.fabric_profile_url,
        ctx,
    )
    .await
}

pub async fn install_quilt(
    mc_version: &str,
    loader_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let InstallCtx { config, .. } = ctx;
    let source = config.file_primary_source().ok_or("no active source")?;
    install_loader_via_profile(
        mc_version,
        loader_version,
        target,
        parent_text,
        &source.quilt_profile_url,
        ctx,
    )
    .await
}
