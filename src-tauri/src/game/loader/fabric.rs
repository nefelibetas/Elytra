// ── Profile 式安装（Fabric / Quilt）：官方 meta profile JSON → merge_loader_profile 合并 ──

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::{merge_loader_profile, InstallCtx, LoaderEvent, VersionTarget};
use crate::config::downloader::DownloadSource;
use crate::downloader::fetcher::http;
use crate::game::resource::library;
use crate::game::version::model;

/// profile 模板按源选取：Fabric/Quilt 各源模板不同（镜像只在 `fabric_profile_url` 上体现 BMCLAPI fabric-meta），循环内
/// 必须用**当前源自己的**模板。`DownloadSource::url` 不用 self、只替换传入模板；统一传主源模板会让每源得到同一 URL。
#[derive(Clone, Copy)]
enum ProfileKind {
    Fabric,
    Quilt,
}

impl ProfileKind {
    fn url(self, src: &DownloadSource) -> &str {
        match self {
            ProfileKind::Fabric => &src.fabric_profile_url,
            ProfileKind::Quilt => &src.quilt_profile_url,
        }
    }
}

/// 逐源候选 `(源, URL)`，保持源顺序并跳过重复 URL（Quilt 两源模板相同 → 只请求一次，避免对同一 URL 再等一遍）。
fn profile_candidates<'a>(
    kind: ProfileKind,
    sources: &[&'a DownloadSource],
    mc_version: &str,
    loader_version: &str,
) -> Vec<(&'a DownloadSource, String)> {
    let mut out: Vec<(&'a DownloadSource, String)> = Vec::with_capacity(sources.len());
    for src in sources {
        let url = src.url(
            kind.url(src),
            &[("mc", mc_version), ("loaderVer", loader_version)],
        );
        if out.iter().any(|(_, u)| u == &url) {
            continue;
        }
        out.push((*src, url));
    }
    out
}

/// `dir_name`/`vdir` 由 runner 传入（含冲突重命名 `{id}_n`）；自行计算会与 runner 目标分裂（新目录缺 json → 启动报"未找到版本 JSON"）。
async fn install_loader_via_profile(
    mc_version: &str,
    loader_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    kind: ProfileKind,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let VersionTarget { dir_name, vdir } = target;
    let InstallCtx {
        config,
        pc,
        hook,
        cancel,
        ..
    } = ctx;
    let mc = config.minecraft_dir_abs();

    // profile 逐源回退：官方 maven 国内常不可达，镜像提供同一路径
    let mut fetched: Option<(serde_json::Value, &DownloadSource)> = None;
    let mut last_err = String::new();
    for (src, url) in profile_candidates(
        kind,
        &config.file_sources_list(),
        mc_version,
        loader_version,
    ) {
        match http().get(&url).send().await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(v) => {
                    fetched = Some((v, src));
                    break;
                }
                Err(e) => last_err = format!("解析 profile 失败\n{e}"),
            },
            Err(e) => last_err = format!("获取 profile 失败\n{e}"),
        }
    }
    let (profile, source) =
        fetched.ok_or_else(|| format!("获取 profile 失败（所有下载源均不可用）\n{last_err}"))?;

    let merged = merge_loader_profile(parent_text, &profile, dir_name, &source.maven_url)?;

    std::fs::create_dir_all(vdir)
        .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    let json_path = vdir.join(format!("{}.json", dir_name));
    let json_str = serde_json::to_string_pretty(&merged).map_err(common_err_ctx!("序列化失败"))?;
    std::fs::write(&json_path, &json_str)
        .map_err(common_err_ctx_key!("errors.file.write", "写入 JSON 失败"))?;
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
        Some(cancel),
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
    install_loader_via_profile(
        mc_version,
        loader_version,
        target,
        parent_text,
        ProfileKind::Fabric,
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
    install_loader_via_profile(
        mc_version,
        loader_version,
        target,
        parent_text,
        ProfileKind::Quilt,
        ctx,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::endpoints;

    /// 多源回退必须使用每个源自己的 profile 模板：官方与镜像得到同一 URL 时镜像从未被请求，官方不可达即安装失败。
    #[test]
    fn fabric_profile_candidates_include_mirror() {
        let sources = vec![endpoints::official_source(), endpoints::mirror_source()];
        let refs: Vec<&DownloadSource> = sources.iter().collect();
        let urls: Vec<String> = profile_candidates(ProfileKind::Fabric, &refs, "1.20.1", "0.15.0")
            .into_iter()
            .map(|(_, u)| u)
            .collect();
        assert_eq!(urls.len(), 2, "官方 + 镜像应各有一个候选 URL: {urls:?}");
        assert!(
            urls[0].contains("meta.fabricmc.net"),
            "首选应为官方 meta: {}",
            urls[0]
        );
        assert!(
            urls[1].contains("bmclapi"),
            "回退候选必须落到 BMCLAPI fabric-meta: {}",
            urls[1]
        );
        assert_ne!(urls[0], urls[1], "两源 URL 必须不同，否则回退是空转");
    }

    /// Quilt 两源模板相同（无 BMCLAPI 镜像）→ 去重只请求一次，避免对同一 URL 白等一轮。
    #[test]
    fn quilt_profile_candidates_dedupe_same_template() {
        let sources = vec![endpoints::official_source(), endpoints::mirror_source()];
        let refs: Vec<&DownloadSource> = sources.iter().collect();
        let urls = profile_candidates(ProfileKind::Quilt, &refs, "1.20.1", "0.20.0");
        assert_eq!(urls.len(), 1, "同模板应去重: {:?}", urls);
    }
}
