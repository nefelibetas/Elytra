//! 安装计划组装（纯函数）与包体清单获取。

use std::path::{Path, PathBuf};

use crate::config::downloader::DownloaderConfig;
use crate::game::community::curseforge_files;
use crate::game::resource::extract::open_zip;
use crate::game::version::manifest::fetch_version_list;
use crate::task::LoaderChoice;
use crate::util::safe_relative_path;

use super::curseforge::{direct_pack_file, parse_curseforge_manifest, resolve_cf_files};
use super::detect::{detect_pack, existing_overrides};
use super::directory::parse_directory_pack;
use super::hmcl::parse_hmcl_pack;
use super::mcbbs::parse_mcbbs_pack;
use super::model::{
    ModpackRequest, OverridesRef, PackFile, PackKind, PackMetaInfo, PackPlan, PackSource,
};
use super::modrinth::parse_modrinth_index;
use super::multimc::parse_mmc_pack;

/// 内嵌整合包上限（PCL 带启动器包可能含完整游戏目录）。
const MAX_EMBEDDED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// 包体缓存目录（在线包、内嵌包与恢复计划共用）。
pub(crate) fn cache_dir() -> PathBuf {
    crate::util::config_dir().join("modpacks")
}

fn find_reusable_pack_dir(mc: &Path, cache_key: &str) -> Option<String> {
    let entries = std::fs::read_dir(mc.join("versions")).ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(dir.join("elytra-pack.json")) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if v["cache_key"].as_str() == Some(cache_key) {
            return Some(dir.file_name()?.to_string_lossy().to_string());
        }
    }
    None
}

/// 包身份缓存键：`{净化包名}-{来源哈希8}`（包体缓存 / 计划文件 / 版本身份标记共用）。
pub fn cache_key(request: &ModpackRequest) -> String {
    let seed = match &request.source {
        PackSource::Local { path } => path.clone(),
        PackSource::Online { url, .. } => url.clone(),
    };
    format!(
        "{}-{:08x}",
        crate::util::sanitize_fs_name(&request.name, "modpack"),
        crate::util::fnv1a64(&seed)
    )
}

pub struct PackMeta<'a> {
    pub pack_name: &'a str,
    pub cache_key: &'a str,
    pub mc_version: &'a str,
    pub loader: Option<LoaderChoice>,
    pub optifine: Option<String>,
    pub files: Vec<PackFile>,
    pub restricted: Vec<String>,
    pub overrides: Vec<OverridesRef>,
    pub version_url: String,
    pub minecraft_dir: &'a Path,
    pub kind: PackKind,
    pub meta: PackMetaInfo,
}

/// 组装 PackPlan（纯函数，无网络）：非空校验、路径穿越防护、`{净化包名}-{mc}` 命名与 `_{n}` 冲突消解；overrides 由格式解析器提供。
pub fn assemble_plan(meta: PackMeta<'_>) -> Result<PackPlan, String> {
    let PackMeta {
        pack_name,
        cache_key,
        mc_version,
        loader,
        optifine,
        files,
        restricted,
        overrides,
        version_url,
        minecraft_dir,
        kind,
        meta,
    } = meta;
    let mc_version = mc_version.trim().to_string();

    if files.is_empty() && overrides.is_empty() && restricted.is_empty() {
        return Err(keyed_msg!(
            "errors.modpack.emptyContent",
            "整合包没有可安装的内容（无文件且无覆盖）"
        ));
    }
    // 路径穿越防护提前到计划期：坏条目解析期即报错，不带病安装
    for f in &files {
        safe_relative_path(&f.path)?;
    }
    // 覆盖前缀来自外部清单（CF 自定义目录 / MMC 包装目录），同样必须在计划期校验
    for ov in &overrides {
        let rel = ov.prefix.trim_end_matches('/');
        if rel.is_empty() || safe_relative_path(rel).is_err() {
            return Err(keyed_msg!(
                "errors.modpack.badOverridesDir",
                "整合包覆盖目录名无效\n{rel}"
            ));
        }
        for skip in &ov.skip {
            let skip_rel = skip.trim_end_matches('/');
            if skip_rel.is_empty() || safe_relative_path(skip_rel).is_err() {
                return Err(keyed_msg!(
                    "errors.modpack.badOverridesDir",
                    "整合包覆盖目录名无效\n{skip}"
                ));
            }
        }
    }

    // 包名净化统一走 util（Windows 保留名回退 "modpack"）；目录冲突 `_{n}`
    let base_dir = format!(
        "{}-{}",
        crate::util::sanitize_fs_name(pack_name, "modpack"),
        mc_version
    );
    // 优先复用同 cache_key 的版本目录（更新语义，sha1 幂等跳过才生效）；否则新建并按 `_{n}` 消解冲突
    let version_dir_name = find_reusable_pack_dir(minecraft_dir, cache_key).unwrap_or_else(|| {
        crate::util::unique_child_dir(&minecraft_dir.join("versions"), &base_dir)
    });

    Ok(PackPlan {
        name: pack_name.to_string(),
        cache_key: cache_key.to_string(),
        mc_version,
        loader,
        optifine,
        version_url,
        version_dir_name,
        files,
        restricted,
        overrides,
        kind: Some(kind),
        meta: Some(meta),
    })
}

/// 读包体 → 嗅探格式 → 构建 PackPlan：CF 直链优先、其余批量换链；版本 json URL 经官方清单多源回退 + TTL 缓存解析。
pub async fn build_plan(
    zip_path: &Path,
    request: &ModpackRequest,
    config: &DownloaderConfig,
) -> Result<PackPlan, String> {
    build_plan_depth(zip_path, request, config, 0).await
}

async fn build_plan_depth(
    zip_path: &Path,
    request: &ModpackRequest,
    config: &DownloaderConfig,
    depth: usize,
) -> Result<PackPlan, String> {
    if depth > 2 {
        bail_key!(
            "errors.modpack.embeddedTooDeep",
            "整合包嵌套层数过多\n最多支持两层内嵌包"
        );
    }
    let mut archive = open_zip(zip_path, "整合包")?;
    let detected = detect_pack(&mut archive)?;
    drop(archive);
    if let Some(member) = detected.embedded.as_deref() {
        let inner = extract_embedded(zip_path, member, request)?;
        return Box::pin(build_plan_depth(&inner, request, config, depth + 1)).await;
    }
    let zip_path_text = zip_path.display().to_string();

    let (mc_version, loader, optifine, files, restricted, overrides, meta) = match detected.kind {
        PackKind::Modrinth => {
            let parsed = parse_modrinth_index(&detected.manifest)?;
            let overrides = existing_overrides(
                &detected.members,
                &zip_path_text,
                &[
                    format!("{}overrides/", detected.base),
                    format!("{}client-overrides/", detected.base),
                ],
            );
            (
                parsed.mc_version,
                parsed.loader,
                None,
                parsed.files,
                Vec::new(),
                overrides,
                parsed.meta,
            )
        }
        PackKind::CurseForge => {
            let parsed = parse_curseforge_manifest(&detected.manifest)?;
            let mut files = Vec::new();
            let mut api_entries = Vec::new();
            for entry in &parsed.files {
                match direct_pack_file(entry) {
                    Some(file) => files.push(file),
                    None => api_entries.push(entry),
                }
            }
            let mut restricted = Vec::new();
            if !api_entries.is_empty() {
                let wanted: Vec<(i64, i64)> = api_entries
                    .iter()
                    .map(|entry| (entry.project_id, entry.file_id))
                    .collect();
                let ids: Vec<i64> = wanted.iter().map(|(_, file_id)| *file_id).collect();
                // 无 API key / 网络失败时按文件名降级为受限清单，让包内覆盖目录的物理副本仍可复用
                match curseforge_files(&ids).await {
                    Ok(data) => {
                        let (resolved, locked) = resolve_cf_files(&data, &wanted);
                        files.extend(resolved);
                        restricted.extend(locked);
                    }
                    Err(e) => return Err(e),
                }
            }
            let overrides = existing_overrides(
                &detected.members,
                &zip_path_text,
                &[format!(
                    "{}{}/",
                    detected.base,
                    parsed.overrides_dir.trim_matches('/')
                )],
            );
            (
                parsed.mc_version,
                parsed.loader,
                None,
                files,
                restricted,
                overrides,
                parsed.meta,
            )
        }
        PackKind::MultiMc => {
            let parsed = parse_mmc_pack(&detected, &zip_path_text)?;
            (
                parsed.mc_version,
                parsed.loader,
                parsed.optifine,
                Vec::new(),
                Vec::new(),
                parsed.overrides,
                parsed.meta,
            )
        }
        PackKind::Directory => {
            let parsed = parse_directory_pack(&detected, &zip_path_text)?;
            (
                parsed.mc_version,
                parsed.loader,
                None,
                Vec::new(),
                Vec::new(),
                parsed.overrides,
                parsed.meta,
            )
        }
        PackKind::Hmcl => {
            let parsed = parse_hmcl_pack(&detected, &zip_path_text)?;
            (
                parsed.mc_version,
                parsed.loader,
                None,
                Vec::new(),
                Vec::new(),
                parsed.overrides,
                parsed.meta,
            )
        }
        PackKind::Mcbbs => {
            let parsed = parse_mcbbs_pack(&detected, &zip_path_text)?;
            let mut files = parsed.files;
            let mut restricted = Vec::new();
            if !parsed.curse.is_empty() {
                let wanted: Vec<(i64, i64)> = parsed
                    .curse
                    .iter()
                    .map(|file| (file.project_id, file.file_id))
                    .collect();
                let ids: Vec<i64> = wanted.iter().map(|(_, file_id)| *file_id).collect();
                match curseforge_files(&ids).await {
                    Ok(data) => {
                        let (resolved, locked) = resolve_cf_files(&data, &wanted);
                        files.extend(resolved);
                        restricted.extend(locked);
                    }
                    Err(_) => {
                        for file in &parsed.curse {
                            restricted.push(
                                file.file_name
                                    .clone()
                                    .unwrap_or_else(|| format!("fileID {}", file.file_id)),
                            );
                        }
                    }
                }
            }
            (
                parsed.mc_version,
                parsed.loader,
                parsed.optifine,
                files,
                restricted,
                parsed.overrides,
                parsed.meta,
            )
        }
        PackKind::PclLauncher => {
            bail_key!(
                "errors.modpack.embeddedMissing",
                "带启动器整合包缺少内嵌整合包"
            );
        }
    };

    let manifest = fetch_version_list(config).await?;
    let trimmed = mc_version.trim();
    let version_url = manifest
        .versions
        .iter()
        .find(|v| v.id == trimmed)
        .map(|v| v.url.clone())
        .ok_or_else(|| {
            keyed_msg!(
                "errors.modpack.mcVersionNotFound",
                "官方源未收录该 Minecraft 版本\n{trimmed}"
            )
        })?;

    assemble_plan(PackMeta {
        pack_name: &request.name,
        cache_key: &cache_key(request),
        mc_version: &mc_version,
        loader,
        optifine,
        files,
        restricted,
        overrides,
        version_url,
        minecraft_dir: &config.minecraft_dir_abs(),
        kind: detected.kind,
        meta,
    })
}

/// PCL 带启动器包：把内嵌整合包解到缓存（不执行包内程序）；同一外层包复用已解出的副本。
pub(super) fn extract_embedded(
    outer: &Path,
    member: &str,
    request: &ModpackRequest,
) -> Result<PathBuf, String> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir)
        .map_err(common_err_ctx_key!("errors.file.createDir", "创建目录失败"))?;
    // 外层文件被同名覆盖时尺寸/mtime 会变化，不能用旧内嵌包。
    let stamp = std::fs::metadata(outer)
        .map(|m| format!("{}-{}", m.len(), crate::util::modified_secs(&m)))
        .unwrap_or_else(|_| "unknown".into());
    let dest = dir.join(format!("{}-embedded-{stamp}.zip", cache_key(request)));
    if dest.is_file() {
        return Ok(dest);
    }

    let mut archive = open_zip(outer, "整合包")?;
    let mut entry = archive.by_name(member).map_err(common_err_ctx_key!(
        "errors.file.read",
        "读取内嵌整合包失败"
    ))?;
    if entry.size() > MAX_EMBEDDED_BYTES {
        bail_key!(
            "errors.modpack.embeddedTooLarge",
            "内嵌整合包过大\n{member}"
        );
    }
    let tmp = dest.with_extension("zip.tmp");
    let mut out = std::fs::File::create(&tmp)
        .map_err(common_err_ctx_key!("errors.file.write", "创建文件失败"))?;
    // 中央目录的 size 可被伪造，复制时再兜一层硬上限
    let copied = std::io::copy(
        &mut std::io::Read::take(&mut entry, MAX_EMBEDDED_BYTES + 1),
        &mut out,
    )
    .map_err(common_err_ctx_key!("errors.file.write", "写入文件失败"))?;
    drop(out);
    if copied > MAX_EMBEDDED_BYTES {
        std::fs::remove_file(&tmp).ok();
        bail_key!(
            "errors.modpack.embeddedTooLarge",
            "内嵌整合包过大\n{member}"
        );
    }
    std::fs::rename(&tmp, &dest)
        .map_err(common_err_ctx_key!("errors.file.write", "写入文件失败"))?;
    Ok(dest)
}
