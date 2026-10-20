// ── OptiFine 安装：分场景（纯净版 classpath / Forge-like mods / Fabric-like OptiFabric）──
// 纯净版 → 继承式独立版本 + jar 作 classpath 库（1.13+ 为预补丁客户端，类覆盖 vanilla）；Forge/NeoForge → jar 放 mods/；Fabric/Quilt → 须搭配 OptiFabric 桥接一起放 mods/。

use super::{InstallCtx, VersionTarget};
use crate::downloader::{fetcher, progress};
use crate::game::community;

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

/// OptiFabric 的 CurseForge slug（官方唯一发布渠道；Modrinth 同名项目是"替代整合包"）。
const OPTIFABRIC_CF_SLUG: &str = "optifabric";

/// 从 CurseForge 文件列表响应挑出最新文件：(文件名, 下载 URL, sha1)。纯函数便于单测。
/// 下载 URL 为空（CF 对部分请求不返回）时返回 None。
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

/// 查询并下载 OptiFabric 桥接 mod 到 mods 目录：CurseForge API（slug 查项目 → gameVersion+fabric 取最新文件）→
/// forgecdn 直链（x-api-key 仅查询需要）。无匹配版本明确报错——OptiFabric 仅覆盖至 1.20.x（1.21+ 不应静默装无效 jar）。
async fn download_optifabric(
    mc_version: &str,
    mods_dir: &std::path::Path,
    pc: &progress::ProgressCounters,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    // 1. 项目 ID（slug 查，避免硬编码 ID 随项目迁移失效）
    let search = community::curseforge_get(
        "/mods/search",
        &[
            ("slug", OPTIFABRIC_CF_SLUG.into()),
            ("pageSize", "1".into()),
        ],
    )
    .await
    .map_err(common_err_ctx_key!("errors.install.loader", "查询 OptiFabric 项目失败"))?;
    let mod_id = search["data"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|m| m["id"].as_u64())
        .ok_or("查询 OptiFabric 项目失败\nCurseForge 未找到 OptiFabric 项目")?;

    // 2. 该 MC 版本 + Fabric 的最新文件
    let files = community::curseforge_get(
        &format!("/mods/{mod_id}/files"),
        &[
            ("gameVersion", mc_version.to_string()),
            ("modLoaderType", "4".into()), // fabric（OptiFabric 官方只声明 fabric；Quilt 兼容 fabric mod）
            ("pageSize", "1".into()),
        ],
    )
    .await
    .map_err(common_err_ctx_key!("errors.install.loader", "查询 OptiFabric 版本失败"))?;
    let Some((filename, download_url, sha1)) = pick_curseforge_file(&files) else {
        bail!(
            "下载 OptiFabric 失败\nMC {mc_version} 暂无兼容的 OptiFabric 版本（OptiFabric 仅支持至 1.20.x）"
        );
    };

    // 3. forgecdn 直链下载（x-api-key 由查询环节承担，下载可匿名）
    std::fs::create_dir_all(mods_dir).map_err(common_err_ctx_key!("errors.file.createDir", "创建 mods 目录失败"))?;
    let dest = mods_dir.join(&filename);
    // CurseForge 文件条目自带 sha1，用它校验（否则镜像/CDN 截断内容会被静默接受）
    let sha1 = sha1.unwrap_or_default();
    if sha1.is_empty() {
        fetcher::fetch_bounded_c(
            &[download_url],
            &dest,
            Some(pc.step_progress()),
            Some(cancel),
        )
        .await
        .map_err(common_err_ctx_key!("errors.download.failed", "下载 OptiFabric 失败"))?;
    } else {
        fetcher::fetch_verified_c(
            &[download_url],
            &dest,
            Some(pc.step_progress()),
            &sha1,
            Some(cancel),
        )
        .await
        .map_err(common_err_ctx_key!("errors.download.failed", "下载 OptiFabric 失败"))?;
    }
    Ok(())
}

/// 生成纯净版（继承式）version.json。`artifact_path` 为 libraries 相对路径（须与 jar 实际落位一致）；`artifact_url` 为真实可下载 URL（不用 file://——reqwest 不支持）。
fn build_vanilla_version_json(
    dir_name: &str,
    mc_version: &str,
    patch: &str,
    artifact_path: &str,
    artifact_url: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": dir_name,
        // 统一官方主类：1.13+ 已移除 launchwrapper，写死必崩；≤1.12.2 的 Main 亦然（本实现不注入 tweak，行为等价于原版启动）
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

pub async fn install_optifine(
    mc_version: &str,
    patch: &str,
    of_type: &str,
    target: VersionTarget<'_>,
    mode: OptifineMode,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    let VersionTarget { dir_name, vdir } = target;
    let InstallCtx {
        config, pc, cancel, ..
    } = ctx;
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

    // 落位随场景：纯净版 → libraries/{dir_name}/（classpath 与缺失检查按 artifact.path 找）；
    // Forge-like / Fabric-like → 版本目录 mods/（ModLoader 扫描）。
    let (dest, artifact_path) = match mode {
        OptifineMode::Vanilla => {
            let lib_dir = config.minecraft_dir_abs().join("libraries").join(dir_name);
            (lib_dir.join(&filename), format!("{dir_name}/{filename}"))
        }
        OptifineMode::ForgeLike | OptifineMode::FabricLike => {
            let mods_dir = vdir.join("mods");
            (mods_dir.join(&filename), String::new())
        }
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    }

    // 字节级进度：step_progress 工厂（增量累加进全局与 OptiFine 步骤计数，total 为分母）。
    // 不用 byte_callback：后者不写 step_total，而 OptiFine jar 需要步骤分母显示总字节。
    fetcher::fetch(&jar_url, &dest, Some(pc.step_progress()))
        .await
        .map_err(common_err_ctx_key!("errors.download.failed", "下载 OptiFine 失败"))?;

    match mode {
        OptifineMode::Vanilla => {
            // 继承式 version.json：继承原版，assetIndex/arguments/父库等由启动端
            // env.rs load_version_json_with_inheritance 展开补齐（父版本须已安装，缺失时明确报错引导补装）。
            let json =
                build_vanilla_version_json(dir_name, mc_version, patch, &artifact_path, &jar_url);
            let json_path = vdir.join(format!("{dir_name}.json"));
            std::fs::write(
                &json_path,
                serde_json::to_string_pretty(&json).map_err(common_err_ctx!("序列化失败"))?,
            )
            .map_err(common_err_ctx_key!("errors.file.write", "写入 version.json 失败"))?;
        }
        OptifineMode::ForgeLike => {
            // loader 的合并 version.json 由 install_forge/neoforge 生成，OptiFine 不写 json（否则会顶掉它）
        }
        OptifineMode::FabricLike => {
            download_optifabric(mc_version, &vdir.join("mods"), pc, cancel).await?;
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
        assert_eq!(
            j["mainClass"], "net.minecraft.client.main.Main",
            "1.13+ 无 launchwrapper"
        );
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
        assert!(
            pick_curseforge_file(&data).is_none(),
            "空下载 URL 视为无可用文件"
        );
        // 空 data / 缺字段也不 panic
        assert!(pick_curseforge_file(&serde_json::json!({})).is_none());
        assert!(pick_curseforge_file(&serde_json::json!({ "data": [] })).is_none());
    }
}
