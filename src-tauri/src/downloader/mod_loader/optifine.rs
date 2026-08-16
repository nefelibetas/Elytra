// ── OptiFine 安装：分场景（纯净版 classpath / Forge-like mods / Fabric-like OptiFabric）──
// 历史背景：OptiFine 本质是客户端补丁（mod 形态），但接入方式随环境不同：
// - 纯净版（无 ModLoader）：官方安装器生成「继承式独立版本」——OptiFine jar 作为
//   classpath 库（1.13+ 的 jar 是预补丁客户端，类覆盖 vanilla 生效）。
// - Forge / NeoForge（1.13+）：jar 作为普通 mod 放 mods 目录。
// - Fabric / Quilt：OptiFine 不是 Fabric mod 结构，必须搭配 OptiFabric 桥接 mod
//   一起放 mods 目录。
// 旧实现把三种场景一律当纯净版处理且 jar 落位与 artifact.path 错位（jar 在 versions/
// 而 classpath/missing 检查按 libraries/ 找）→ 启动被完整性检查拦截；mainClass 写死
// launchwrapper → 1.13+ 无此类必崩。本文件按场景落位。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::downloader::model::DownloaderConfig;
use crate::downloader::fetcher;

/// OptiFine 安装场景（由 runner 按「主 ModLoader 组合」判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptifineMode {
    /// 纯净版（无 ModLoader）：独立版本 + classpath library
    Vanilla,
    /// Forge / NeoForge：jar 放 mods 目录
    ForgeLike,
    /// Fabric / Quilt：OptiFine + OptiFabric 桥接一起放 mods 目录
    FabricLike,
}

/// OptiFabric 的 CurseForge slug（官方唯一发布渠道；Modrinth 同名项目是"替代整合包"，GitHub 已停更旧版）。
const OPTIFABRIC_CF_SLUG: &str = "optifabric";

/// 从 CurseForge 文件列表响应中挑出最新文件：(文件名, 下载 URL, sha1)。
/// 纯函数便于单测。下载 URL 为空（CF 对部分请求不返回）时返回 None。
fn pick_curseforge_file(data: &serde_json::Value) -> Option<(String, String, Option<String>)> {
    let file = data["data"].as_array()?.first()?;
    let filename = file["fileName"].as_str()?.to_string();
    let url = file["downloadUrl"].as_str()?.to_string();
    if url.is_empty() {
        return None;
    }
    let sha1 = file["hashes"]
        .as_array()?
        .iter()
        .find(|h| h["algo"].as_u64() == Some(1))
        .and_then(|h| h["value"].as_str())
        .map(str::to_string);
    Some((filename, url, sha1))
}

/// 查询并下载 OptiFabric 桥接 mod 到 mods 目录。
/// 渠道：CurseForge 官方 API（slug 查项目 → gameVersion+fabric 过滤取最新文件）→
/// forgecdn 直链下载（实测可匿名，x-api-key 仅查询需要）。查询失败/无匹配版本时
/// 明确报错——OptiFabric 维护缓慢（CF 最新 1.14.3，2024-01，仅覆盖至 1.20.x），
/// 现代 MC（1.21+）无兼容版本时不应静默装个无效 jar。
async fn download_optifabric(
    mc_version: &str,
    mods_dir: &std::path::Path,
    bytes: &Arc<AtomicU64>,
    step_bytes: &Arc<AtomicU64>,
    step_total: &Arc<AtomicU64>,
) -> Result<(), String> {
    // 1. 项目 ID（slug 查，避免硬编码 ID 随项目迁移失效）
    let search = crate::downloader::mods::curseforge_get(
        "/mods/search",
        &[("slug", OPTIFABRIC_CF_SLUG.into()), ("pageSize", "1".into())],
    )
    .await
    .map_err(|e| format!("查询 OptiFabric 项目失败\n{e}"))?;
    let mod_id = search["data"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|m| m["id"].as_u64())
        .ok_or("查询 OptiFabric 项目失败\nCurseForge 未找到 OptiFabric 项目")?;

    // 2. 该 MC 版本 + Fabric 的最新文件
    let files = crate::downloader::mods::curseforge_get(
        &format!("/mods/{mod_id}/files"),
        &[
            ("gameVersion", mc_version.to_string()),
            ("modLoaderType", "4".into()), // fabric（OptiFabric 官方只声明 fabric；Quilt 兼容 fabric mod）
            ("pageSize", "1".into()),
        ],
    )
    .await
    .map_err(|e| format!("查询 OptiFabric 版本失败\n{e}"))?;
    let Some((filename, download_url, _sha1)) = pick_curseforge_file(&files) else {
        return Err(format!(
            "下载 OptiFabric 失败\nMC {mc_version} 暂无兼容的 OptiFabric 版本（OptiFabric 仅支持至 1.20.x）"
        ));
    };

    // 3. forgecdn 直链下载（与单文件下载同一通道；x-api-key 由查询环节承担，下载可匿名）
    std::fs::create_dir_all(mods_dir).map_err(|e| format!("创建 mods 目录失败\n{e}"))?;
    let dest = mods_dir.join(&filename);
    fetcher::fetch_bounded(
        &[download_url],
        &dest,
        Some(step_progress(bytes, step_bytes, step_total)),
    )
    .await
    .map_err(|e| format!("下载 OptiFabric 失败\n{e}"))?;
    Ok(())
}

/// 生成纯净版（继承式）version.json。
/// `artifact_path`：libraries 相对路径（与 jar 实际落位一致，classpath/missing 检查按它找）；
/// `artifact_url`：真实可下载 URL（修复任务能重下；不用 file://——reqwest 不支持该 scheme）。
fn build_vanilla_version_json(
    dir_name: &str,
    mc_version: &str,
    patch: &str,
    artifact_path: &str,
    artifact_url: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": dir_name,
        // 统一官方客户端主类：1.13+ 客户端已移除 launchwrapper，写死它必崩；
        // ≤1.12.2 的 Main 类同样存在（launchwrapper 仅是 tweak 壳，旧版 OptiFine
        // 需 --tweakClass optifine.OptiFineTweaker 注入才生效，本实现不注入——
        // 此时用 launchwrapper 或 Main 行为等价：OptiFine 不加载、游戏原版启动）。
        "mainClass": "net.minecraft.client.main.Main",
        "libraries": [{
            "name": format!("optifine:OptiFine:{}", patch),
            "downloads": {
                "artifact": {
                    "path": artifact_path,
                    "url": artifact_url,
                    "sha1": "", "size": 0
                }
            }
        }],
        "inheritsFrom": mc_version,
        "type": "release"
    })
}

/// 每文件字节进度：增量累加进全局计数（收尾心跳显示速度）与 OptiFine 步骤计数；
/// total（content-length）写入步骤分母。OptiFine jar 与 OptiFabric 共用同一步骤计数。
fn step_progress(
    bytes: &Arc<AtomicU64>,
    step_bytes: &Arc<AtomicU64>,
    step_total: &Arc<AtomicU64>,
) -> impl Fn(u64, u64) {
    let file_done = Arc::new(AtomicU64::new(0));
    let b = bytes.clone();
    let sb = step_bytes.clone();
    let st = step_total.clone();
    move |downloaded: u64, total: u64| {
        let prev = file_done.swap(downloaded, Ordering::Relaxed);
        let delta = downloaded.saturating_sub(prev);
        b.fetch_add(delta, Ordering::Relaxed);
        sb.fetch_add(delta, Ordering::Relaxed);
        if total > 0 {
            st.store(total, Ordering::Relaxed);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn install_optifine(
    mc_version: &str,
    patch: &str,
    of_type: &str,
    dir_name: &str,
    vdir: &std::path::Path,
    config: &DownloaderConfig,
    mode: OptifineMode,
    bytes: &Arc<AtomicU64>,
    step_bytes: &Arc<AtomicU64>,
    step_total: &Arc<AtomicU64>,
) -> Result<(), String> {
    let config = config.clone();
    let source = config
        .file_primary_source()
        .ok_or("no active source")?
        .clone();

    let filename = format!("OptiFine_{}_{}_{}.jar", mc_version, of_type, patch);
    let jar_url = source.url(
        &source.optifine_download_url,
        &[("mc", mc_version), ("type", of_type), ("patch", patch)],
    );

    // 落位随场景：纯净版 → libraries/{dir_name}/（classpath 与缺失检查都按 artifact.path 找）；
    // Forge-like / Fabric-like → 版本目录 mods/（ModLoader 扫描）。
    let (dest, artifact_path) = match mode {
        OptifineMode::Vanilla => {
            let lib_dir = config.minecraft_dir_abs().join("libraries").join(dir_name);
            (
                lib_dir.join(&filename),
                format!("{dir_name}/{filename}"),
            )
        }
        OptifineMode::ForgeLike | OptifineMode::FabricLike => {
            let mods_dir = vdir.join("mods");
            (mods_dir.join(&filename), String::new())
        }
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败\n{e}"))?;
    }

    // 字节级进度：增量累加进全局计数与 OptiFine 步骤计数（与旧实现一致）
    let file_done = Arc::new(AtomicU64::new(0));
    let progress = {
        let fd = file_done.clone();
        let b = bytes.clone();
        let sb = step_bytes.clone();
        let st = step_total.clone();
        move |downloaded: u64, total: u64| {
            let prev = fd.swap(downloaded, Ordering::Relaxed);
            let delta = downloaded.saturating_sub(prev);
            b.fetch_add(delta, Ordering::Relaxed);
            sb.fetch_add(delta, Ordering::Relaxed);
            if total > 0 {
                st.store(total, Ordering::Relaxed);
            }
        }
    };
    fetcher::fetch(&jar_url, &dest, Some(progress))
        .await
        .map_err(|e| format!("下载 OptiFine 失败\n{e}"))?;

    match mode {
        OptifineMode::Vanilla => {
            // 继承式 version.json：继承原版（assetIndex/arguments/父库等由启动端
            // env.rs load_version_json_with_inheritance 展开补齐；父版本须已安装，
            // 缺失时启动端明确报错引导补装原版）。
            let json = build_vanilla_version_json(
                dir_name,
                mc_version,
                patch,
                &artifact_path,
                &jar_url,
            );
            let json_path = vdir.join(format!("{dir_name}.json"));
            std::fs::write(
                &json_path,
                serde_json::to_string_pretty(&json)
                    .map_err(|e| format!("序列化失败\n{e}"))?,
            )
            .map_err(|e| format!("写入 version.json 失败\n{e}"))?;
        }
        OptifineMode::ForgeLike => {
            // loader 的合并 version.json 由 install_forge/neoforge 生成，OptiFine 不写 json——
            // 旧实现无条件覆盖 minimal json，会把 forge 生成的 json 顶掉。
        }
        OptifineMode::FabricLike => {
            download_optifabric(mc_version, &vdir.join("mods"), bytes, step_bytes, step_total)
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_json_uses_official_main_class() {
        let j = build_vanilla_version_json(
            "1.20.1-optifine-HD_U_I6",
            "1.20.1",
            "HD_U_I6",
            "1.20.1-optifine-HD_U_I6/OptiFine_1.20.1_HD_U_I6.jar",
            "https://bmclapi2.bangbang93.com/optifine/1.20.1/HD_U/I6",
        );
        assert_eq!(j["mainClass"], "net.minecraft.client.main.Main", "1.13+ 无 launchwrapper");
        assert_eq!(j["inheritsFrom"], "1.20.1", "继承原版（启动端展开）");
        assert_eq!(j["id"], "1.20.1-optifine-HD_U_I6");
        let artifact = &j["libraries"][0]["downloads"]["artifact"];
        // artifact.path 与 jar 实际落位一致（classpath/缺失检查按它找）
        assert_eq!(
            artifact["path"],
            "1.20.1-optifine-HD_U_I6/OptiFine_1.20.1_HD_U_I6.jar"
        );
        // 真实可下载 URL：不用 file://（reqwest 不支持该 scheme，修复任务能重下）
        assert!(
            artifact["url"].as_str().unwrap().starts_with("http"),
            "url 必须是 http(s) 可下载地址: {}",
            artifact["url"]
        );
    }

    #[test]
    fn curseforge_file_pick_latest_with_sha1() {
        let data = serde_json::json!({
            "data": [{
                "fileName": "optifabric-1.14.3.jar",
                "downloadUrl": "https://edge.forgecdn.net/files/5025/647/optifabric-1.14.3.jar",
                "hashes": [
                    { "algo": 1, "value": "abc123" },
                    { "algo": 2, "value": "sha512..." }
                ]
            }]
        });
        let (name, url, sha1) = pick_curseforge_file(&data).expect("应挑出最新文件");
        assert_eq!(name, "optifabric-1.14.3.jar");
        assert!(url.starts_with("https://edge.forgecdn.net/"));
        assert_eq!(sha1.as_deref(), Some("abc123"));
    }

    #[test]
    fn curseforge_file_pick_rejects_empty_url() {
        let data = serde_json::json!({ "data": [{ "fileName": "x.jar", "downloadUrl": "" }] });
        assert!(pick_curseforge_file(&data).is_none(), "空下载 URL 视为无可用文件");
        // 空 data / 缺字段也不 panic
        assert!(pick_curseforge_file(&serde_json::json!({})).is_none());
        assert!(pick_curseforge_file(&serde_json::json!({ "data": [] })).is_none());
    }
}
