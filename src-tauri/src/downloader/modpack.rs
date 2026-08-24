//! 整合包清单解析与安装计划（Modrinth `.mrpack` / CurseForge modpack zip）。
//!
//! 两种格式同构：zip = 清单 JSON + `overrides/` 目录。差异在文件下载地址：
//! Modrinth 清单自包含直链（零 API 调用）；CurseForge 只有 (projectID, fileID)
//! 数字引用，需经 `community::curseforge_files` 批量换链（作者关闭第三方分发
//! 时 downloadUrl 为 null → 记入 restricted，任务收尾提示手动下载）。
//!
//! 本模块只做纯解析/校验与安装计划组装（可单测），网络换链在 community.rs，
//! 任务编排（下载/解压/进度）在 task/modpack.rs。

use std::io::Read;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::downloader::model::DownloaderConfig;
use crate::task::{LoaderChoice, LoaderKind};

/// 整合包来源（命令入参；前端传 snake_case 键）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PackSource {
    /// 本地文件（.mrpack / CurseForge zip）
    Local { path: String },
    /// 在线下载（搜索结果 / 资源详情页的版本文件）
    Online {
        url: String,
        #[serde(default)]
        sha1: String,
        #[serde(default)]
        size: u64,
    },
}

/// 整合包安装请求（install_modpack 命令入参）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackRequest {
    pub name: String,
    pub source: PackSource,
    /// 平台项目图标 URL（在线安装由前端透传；本地导入无平台元数据为 None）。
    /// 安装期下载到 versions/{dir}/pack-icon.png，失败静默跳过（纯装饰内容）。
    #[serde(default)]
    pub icon_url: Option<String>,
}

/// 单个待下载文件的安装计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFile {
    /// 相对版本目录的目标路径（已过穿越防护，`/` 分隔）
    pub path: String,
    pub url: String,
    /// sha1（空串跳过校验，对齐 fetch_verified 语义）
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

/// overrides 目录引用：zip 内前缀 → 解压到版本目录根。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverridesRef {
    /// 包 zip 的本地缓存路径（prepare 阶段落盘）
    pub zip_path: String,
    /// zip 内成员前缀（恒为 "overrides/"）
    pub prefix: String,
}

/// mrpack 清单解析结果。
#[derive(Debug)]
pub struct ModrinthParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub files: Vec<PackFile>,
}

/// 安装计划（prepare 阶段产物，持久化为 JSON；content/base 阶段消费）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackPlan {
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader: Option<LoaderChoice>,
    /// 原版 version json URL（base 阶段 DownloadTask.version_url）
    pub version_url: String,
    /// 预定版本目录名 `{包名}-{mc}`（冲突 `_{n}`），三阶段共享保证一致
    pub version_dir_name: String,
    /// 包身份键（重装/更新同名包时据此复用已有版本目录，sha1 幂等跳过才生效）
    #[serde(default)]
    pub cache_key: String,
    #[serde(default)]
    pub files: Vec<PackFile>,
    /// 作者关闭第三方分发的 CF 文件名（无法自动下载，收尾提示手动放置）
    #[serde(default)]
    pub restricted: Vec<String>,
    #[serde(default)]
    pub overrides: Option<OverridesRef>,
}

/// 解析 modrinth.index.json 文本。
/// 规范：<https://docs.modrinth.com/#spec/modrinth-index-json>。
/// env.client == "unsupported" 的文件（服务端专属）剔除。
pub fn parse_modrinth_index(text: &str) -> Result<ModrinthParsed, String> {
    let idx: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("解析 modrinth.index.json 失败\n{e}"))?;
    if idx["formatVersion"].as_i64() != Some(1) {
        return Err(format!(
            "不支持的整合包格式版本\nformatVersion: {}",
            idx["formatVersion"]
        ));
    }
    if idx["game"].as_str() != Some("minecraft") {
        return Err(format!(
            "不是 Minecraft 整合包\ngame: {}",
            idx["game"].as_str().unwrap_or("(缺失)")
        ));
    }
    let mc_version = idx["dependencies"]["minecraft"]
        .as_str()
        .ok_or("清单缺少 Minecraft 版本（dependencies.minecraft）")?
        .to_string();
    let loader = modrinth_loader(&idx["dependencies"])?;

    let mut files = Vec::new();
    for f in idx["files"].as_array().ok_or("清单缺少 files 数组")? {
        // 服务端专属 mod 不进客户端实例；env 缺失视为需要
        if f["env"]["client"].as_str() == Some("unsupported") {
            continue;
        }
        let path = f["path"].as_str().ok_or("文件条目缺少 path")?.to_string();
        safe_rel_path(&path)?;
        let url = f["downloads"]
            .as_array()
            .and_then(|d| d.first())
            .and_then(|u| u.as_str())
            .ok_or(format!("文件缺少下载地址\n{path}"))?
            .to_string();
        files.push(PackFile {
            path,
            url,
            sha1: f["hashes"]["sha1"].as_str().unwrap_or_default().to_string(),
            size: f["fileSize"].as_u64().unwrap_or(0),
        });
    }
    Ok(ModrinthParsed {
        mc_version,
        loader,
        files,
    })
}

/// dependencies 对象 → LoaderChoice。键：fabric-loader / quilt-loader(含旧写法
/// quilt_loader) / forge / neoforge；未知 loader 键报错（装出来无法启动）。
fn modrinth_loader(deps: &serde_json::Value) -> Result<Option<LoaderChoice>, String> {
    let obj = deps.as_object().ok_or("清单缺少 dependencies 对象")?;
    for (key, val) in obj {
        let ver = match val.as_str() {
            Some(v) if !v.trim().is_empty() => v.to_string(),
            _ => continue,
        };
        let kind = match key.as_str() {
            "fabric-loader" => LoaderKind::Fabric,
            "quilt-loader" | "quilt_loader" => LoaderKind::Quilt,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            "minecraft" => continue,
            other => return Err(format!("暂不支持该整合包的加载器\nloader: {other}")),
        };
        return Ok(Some(LoaderChoice { kind, version: ver }));
    }
    Ok(None)
}

// ── CurseForge manifest.json ──

/// CF 清单解析结果（仅 ID 引用，URL 由 curseforge_files 换链）。
pub struct CfManifestParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    /// (projectID, fileID) 列表
    pub files: Vec<(i64, i64)>,
}

/// 解析 CurseForge manifest.json 文本。
/// 规范：manifestType == "minecraftModpack"、manifestVersion == 1；
/// modLoaders[].id 形如 "forge-47.3.0" / "fabric-0.15.11"。
pub fn parse_curseforge_manifest(text: &str) -> Result<CfManifestParsed, String> {
    let m: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("解析 manifest.json 失败\n{e}"))?;
    if m["manifestType"].as_str() != Some("minecraftModpack") {
        return Err(format!(
            "不是 CurseForge 整合包清单\nmanifestType: {}",
            m["manifestType"].as_str().unwrap_or("(缺失)")
        ));
    }
    let mc_version = m["minecraft"]["version"]
        .as_str()
        .ok_or("清单缺少 Minecraft 版本（minecraft.version）")?
        .to_string();

    let loader = m["minecraft"]["modLoaders"]
        .as_array()
        .and_then(|ls| ls.first())
        .and_then(|l| l["id"].as_str())
        .map(|id| {
            let (kind_str, ver) = id
                .split_once('-')
                .ok_or_else(|| format!("modLoader id 格式异常\n{id}"))?;
            let kind = match kind_str {
                "forge" => LoaderKind::Forge,
                "fabric" => LoaderKind::Fabric,
                "quilt" => LoaderKind::Quilt,
                "neoforge" => LoaderKind::NeoForge,
                other => return Err(format!("暂不支持该整合包的加载器\nloader: {other}")),
            };
            Ok(LoaderChoice {
                kind,
                version: ver.to_string(),
            })
        })
        .transpose()?;

    let mut files = Vec::new();
    for f in m["files"].as_array().ok_or("清单缺少 files 数组")? {
        let pid = f["projectID"].as_i64().ok_or("文件条目缺少 projectID")?;
        let fid = f["fileID"].as_i64().ok_or("文件条目缺少 fileID")?;
        files.push((pid, fid));
    }
    Ok(CfManifestParsed {
        mc_version,
        loader,
        files,
    })
}

/// CF 批量换链响应（`POST /v1/mods/files` 的 data 数组）→ PackFile 列表 +
/// 受限文件名列表（downloadUrl 为 null：作者关闭第三方分发）。
/// algo 1 = SHA1（CurseForge 枚举）。响应中多余/缺失条目按 ID 匹配跳过。
pub fn resolve_cf_files(
    data: &[serde_json::Value],
    wanted: &[(i64, i64)],
) -> (Vec<PackFile>, Vec<String>) {
    use std::collections::HashMap;
    let by_id: HashMap<i64, &serde_json::Value> = data
        .iter()
        .filter_map(|d| d["id"].as_i64().map(|i| (i, d)))
        .collect();
    let mut files = Vec::new();
    let mut restricted = Vec::new();
    for (_, fid) in wanted {
        let Some(d) = by_id.get(fid) else {
            restricted.push(format!("fileID {fid}（API 未返回详情）"));
            continue;
        };
        match d["downloadUrl"].as_str() {
            Some(url) => {
                let name = d["fileName"].as_str().unwrap_or("unknown.jar").to_string();
                let sha1 = d["hashes"]
                    .as_array()
                    .and_then(|hs| {
                        hs.iter()
                            .find(|h| h["algo"].as_i64() == Some(1))
                            .and_then(|h| h["value"].as_str())
                    })
                    .unwrap_or_default()
                    .to_string();
                files.push(PackFile {
                    path: format!("mods/{name}"),
                    url: url.to_string(),
                    sha1,
                    size: d["fileLength"].as_u64().unwrap_or(0),
                });
            }
            None => {
                let name = d["fileName"].as_str().unwrap_or("unknown.jar").to_string();
                restricted.push(name);
            }
        }
    }
    (files, restricted)
}

// ── 公共工具 ──

/// 词法路径穿越防护：整合包内声明的相对路径不得为绝对路径或含 `..` 段
/// （清单来自网络，防恶意条目写出版本目录；对齐 jdk::download::safe_runtime_path）。
pub fn safe_rel_path(rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim();
    if rel.is_empty() || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(format!("非法整合包文件路径\n{rel:?}"));
    }
    if Path::new(rel)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("非法整合包文件路径（含 ..）\n{rel:?}"));
    }
    Ok(PathBuf::from(rel))
}

// ── 清单读取与安装计划组装 ──

/// 清单格式（按 zip 成员名嗅探）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKind {
    Modrinth,
    CurseForge,
}

/// 从 zip 根读取清单文本（按成员名嗅探来源）。
pub fn read_manifest(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> Result<(String, ManifestKind), String> {
    for (name, kind) in [
        ("modrinth.index.json", ManifestKind::Modrinth),
        ("manifest.json", ManifestKind::CurseForge),
    ] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| format!("读取清单失败\n{e}"))?;
            return Ok((text, kind));
        }
    }
    Err("不是有效的整合包文件\n缺少 modrinth.index.json 或 manifest.json".into())
}

/// 组装 PackPlan（纯函数，无网络调用）：overrides 目录检测、内容非空校验、
/// 目标路径穿越防护、版本目录命名（`{净化包名}-{mc}`）与冲突消解。
/// `version_url` 由调用方解析官方版本清单后传入；CF 换链在 `build_plan` 完成。
/// 8 个参数均为语义必要（zip/包名/版本/loader/文件/受限/官方URL/游戏目录），
/// 对齐 count_assets_all 先例不强行合并。
/// 扫描 versions/ 下各整合包版本目录的身份标记，返回与 `cache_key` 匹配的目录名。
/// 标记由安装阶段写入 versions/{dir}/elytra-pack.json。
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

#[allow(clippy::too_many_arguments)]
pub fn assemble_plan(
    zip_path: &Path,
    pack_name: &str,
    cache_key: &str,
    mc_version: &str,
    loader: Option<LoaderChoice>,
    files: Vec<PackFile>,
    restricted: Vec<String>,
    version_url: String,
    minecraft_dir: &Path,
) -> Result<PackPlan, String> {
    let mc_version = mc_version.trim().to_string();

    // overrides 目录检测（重新开一次 zip：函数保持无借用纠缠，便于单测）
    let has_overrides = {
        let file = std::fs::File::open(zip_path).map_err(|e| format!("打开整合包失败\n{e}"))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取整合包失败\n{e}"))?;
        (0..archive.len()).any(|i| {
            archive
                .by_index(i)
                .map(|e| e.name().starts_with("overrides/"))
                .unwrap_or(false)
        })
    };

    if files.is_empty() && !has_overrides && restricted.is_empty() {
        return Err("整合包没有可安装的内容（无文件且无覆盖）".into());
    }
    // 目标路径穿越防护提前到计划期：坏条目解析期即报错，不带病安装
    for f in &files {
        safe_rel_path(&f.path)?;
    }

    // 包名净化统一走 util（Windows 保留名回退 "modpack"）；目录冲突 `_{n}`
    let base_dir = format!(
        "{}-{}",
        crate::util::sanitize_fs_name(pack_name, "modpack"),
        mc_version
    );
    // 目录选择优先复用：已存在同 cache_key 的整合包版本目录 → 更新语义，
    // 内容文件 sha1 幂等跳过才真正生效；否则新建并按 `_{n}` 消解冲突
    let version_dir_name = find_reusable_pack_dir(minecraft_dir, cache_key).unwrap_or_else(|| {
        crate::util::unique_child_dir(&minecraft_dir.join("versions"), &base_dir)
    });

    Ok(PackPlan {
        name: pack_name.to_string(),
        cache_key: cache_key.to_string(),
        mc_version,
        loader,
        version_url,
        version_dir_name,
        files,
        restricted,
        overrides: has_overrides.then(|| OverridesRef {
            zip_path: zip_path.display().to_string(),
            prefix: "overrides/".into(),
        }),
    })
}

/// 读 zip 清单 → 构建 PackPlan。CF 走批量换链网络请求；原版 version json URL
/// 经官方清单多源回退 + TTL 缓存解析。组装细节见 `assemble_plan`。
pub async fn build_plan(
    zip_path: &Path,
    request: &ModpackRequest,
    config: &DownloaderConfig,
) -> Result<PackPlan, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开整合包失败\n{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取整合包失败\n{e}"))?;

    let (index_text, kind) = read_manifest(&mut archive)?;
    drop(archive);
    let (mc_version, loader, files, restricted) = match kind {
        ManifestKind::Modrinth => {
            let parsed = parse_modrinth_index(&index_text)?;
            (parsed.mc_version, parsed.loader, parsed.files, Vec::new())
        }
        ManifestKind::CurseForge => {
            let parsed = parse_curseforge_manifest(&index_text)?;
            let ids: Vec<i64> = parsed.files.iter().map(|(_, fid)| *fid).collect();
            // 批量换链（一次 POST 换全部直链；作者关闭第三方分发的文件进 restricted）
            let data = crate::downloader::community::curseforge_files(&ids).await?;
            let (resolved, locked) = resolve_cf_files(&data, &parsed.files);
            (parsed.mc_version, parsed.loader, resolved, locked)
        }
    };

    // 原版 version json URL（官方清单多源回退 + TTL 缓存）
    let manifest = crate::downloader::manifest::fetch_version_list(config).await?;
    let trimmed = mc_version.trim();
    let version_url = manifest
        .versions
        .iter()
        .find(|v| v.id == trimmed)
        .map(|v| v.url.clone())
        .ok_or_else(|| format!("官方源未收录该 Minecraft 版本\n{trimmed}"))?;

    assemble_plan(
        zip_path,
        &request.name,
        &cache_key(request),
        &mc_version,
        loader,
        files,
        restricted,
        version_url,
        &config.minecraft_dir_abs(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    const MRPACK_INDEX: &str = r#"{
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "v1.0",
        "name": "Demo Pack",
        "files": [
            {
                "path": "mods/sodium.jar",
                "hashes": {"sha1": "aaa", "sha512": "xxx"},
                "env": {"client": "required", "server": "required"},
                "downloads": ["https://cdn.modrinth.com/sodium.jar"],
                "fileSize": 1234
            },
            {
                "path": "mods/serveronly.jar",
                "hashes": {"sha1": "bbb"},
                "env": {"client": "unsupported", "server": "required"},
                "downloads": ["https://cdn.modrinth.com/serveronly.jar"],
                "fileSize": 10
            },
            {
                "path": "config/opt.cfg",
                "downloads": ["https://cdn.modrinth.com/opt.cfg"],
                "fileSize": 5
            }
        ],
        "dependencies": {"minecraft": "1.21.1", "fabric-loader": "0.16.9"}
    }"#;

    #[test]
    fn parse_modrinth_index_filters_env_and_maps_loader() {
        let p = parse_modrinth_index(MRPACK_INDEX).unwrap();
        assert_eq!(p.mc_version, "1.21.1");
        let l = p.loader.expect("应识别 fabric-loader");
        assert_eq!(l.kind, LoaderKind::Fabric);
        assert_eq!(l.version, "0.16.9");
        assert_eq!(p.files.len(), 2, "server-only 与无 env 各留其一");
        assert_eq!(p.files[0].path, "mods/sodium.jar");
        assert_eq!(p.files[0].sha1, "aaa");
        assert_eq!(p.files[1].path, "config/opt.cfg");
    }

    #[test]
    fn parse_modrinth_rejects_wrong_format_and_unknown_loader() {
        assert!(parse_modrinth_index("{\"formatVersion\": 2}").is_err());
        assert!(parse_modrinth_index(
            r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1","forge":"14.23.5.2860"},"files":[]}"#
        )
        .map(|p| {
            assert_eq!(p.loader.unwrap().kind, LoaderKind::Forge);
        })
        .is_ok());
        let bad = r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1","rift":"1.0"},"files":[]}"#;
        let err = parse_modrinth_index(bad).unwrap_err();
        assert!(err.contains("rift"), "未知 loader 应报错: {err}");
    }

    #[test]
    fn parse_curseforge_manifest_maps_loaders_and_ids() {
        let text = r#"{
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "CF Demo",
            "minecraft": {
                "version": "1.20.1",
                "modLoaders": [{"id": "forge-47.3.0", "primary": true}]
            },
            "files": [
                {"projectID": 306612, "fileID": 4631262, "required": true},
                {"projectID": 2222, "fileID": 3333, "required": true}
            ]
        }"#;
        let p = parse_curseforge_manifest(text).unwrap();
        assert_eq!(p.mc_version, "1.20.1");
        let l = p.loader.unwrap();
        assert_eq!(l.kind, LoaderKind::Forge);
        assert_eq!(l.version, "47.3.0");
        assert_eq!(p.files, vec![(306612, 4631262), (2222, 3333)]);

        let fabric = r#"{"manifestType":"minecraftModpack","manifestVersion":1,"minecraft":{"version":"1.21","modLoaders":[{"id":"neoforge-21.1.95"}]},"files":[]}"#;
        let p2 = parse_curseforge_manifest(fabric).unwrap();
        assert_eq!(p2.loader.unwrap().kind, LoaderKind::NeoForge);
        assert!(parse_curseforge_manifest(r#"{"manifestType":"other"}"#).is_err());
    }

    #[test]
    fn resolve_cf_files_splits_restricted_and_hashes() {
        let data = serde_json::json!([
            {
                "id": 4631262,
                "fileName": "jei.jar",
                "downloadUrl": "https://edge.forgecdn.net/files/4631/262/jei.jar",
                "fileLength": 4096,
                "hashes": [
                    {"algo": 2, "value": "md5-ignored"},
                    {"algo": 1, "value": "sha1-ok"}
                ]
            },
            {
                "id": 3333,
                "fileName": "locked.jar",
                "downloadUrl": null
            }
        ]);
        let (files, restricted) =
            resolve_cf_files(data.as_array().unwrap(), &[(1, 4631262), (2, 3333)]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "mods/jei.jar");
        assert_eq!(files[0].sha1, "sha1-ok");
        assert_eq!(files[0].size, 4096);
        assert_eq!(restricted, vec!["locked.jar".to_string()]);
    }

    #[test]
    fn safe_rel_path_blocks_traversal() {
        assert!(safe_rel_path("mods/a.jar").is_ok());
        assert!(safe_rel_path("config/nested/b.json").is_ok());
        assert!(safe_rel_path("../evil").is_err());
        assert!(safe_rel_path("/abs").is_err());
        assert!(safe_rel_path("\\abs").is_err());
        assert!(safe_rel_path("").is_err());
    }

    /// 构造测试 mrpack zip：modrinth.index.json + overrides 内容（assemble_plan 单测用）。
    fn write_mrpack_zip(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("modrinth.index.json", opts).unwrap();
        w.write_all(MRPACK_INDEX.as_bytes()).unwrap();
        w.start_file("overrides/mods/a.jar", opts).unwrap();
        w.write_all(b"jar-bytes").unwrap();
        w.finish().unwrap();
    }

    /// assemble_plan：临时 zip + 清单文本 → 计划字段（净化包名/trim 版本/
    /// overrides 引用/穿越校验），目录冲突 `_{n}` 后缀，空内容报错。
    #[test]
    fn assemble_plan_builds_fields_and_suffixes_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let mc = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_mrpack_zip(&zip_path);

        let mut ar = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let (text, kind) = read_manifest(&mut ar).unwrap();
        assert_eq!(kind, ManifestKind::Modrinth, "成员名嗅探应识别 Modrinth");
        drop(ar);
        let parsed = parse_modrinth_index(&text).unwrap();

        let plan = assemble_plan(
            &zip_path,
            r#"Demo <Pack>"#,
            "key-a",
            " 1.21.1 ",
            parsed.loader,
            parsed.files,
            Vec::new(),
            "https://piston-meta/v1.json".into(),
            mc.path(),
        )
        .unwrap();
        assert_eq!(plan.name, r#"Demo <Pack>"#, "展示名原样保留");
        assert_eq!(plan.mc_version, "1.21.1", "版本号两侧空白应 trim");
        assert!(plan.loader.is_some(), "loader 透传");
        assert_eq!(plan.version_url, "https://piston-meta/v1.json");
        assert_eq!(
            plan.version_dir_name, "Demo _Pack_-1.21.1",
            "非法字符替换 '_' 后拼 净化名-mc",
        );
        assert_eq!(plan.files.len(), 2);
        assert!(plan.restricted.is_empty());
        let ov = plan.overrides.expect("含 overrides 目录应生成引用");
        assert_eq!(ov.prefix, "overrides/");
        assert_eq!(ov.zip_path, zip_path.display().to_string());

        // 冲突消解：同名版本目录已存在 → _{n} 后缀（util::unique_child_dir 接入）
        std::fs::create_dir_all(mc.path().join("versions").join("Demo _Pack_-1.21.1")).unwrap();
        let reparsed = parse_modrinth_index(&text).unwrap();
        let plan2 = assemble_plan(
            &zip_path,
            r#"Demo <Pack>"#,
            "key-b",
            "1.21.1",
            reparsed.loader,
            reparsed.files,
            Vec::new(),
            "u".into(),
            mc.path(),
        )
        .unwrap();
        assert_eq!(plan2.version_dir_name, "Demo _Pack_-1.21.1_1");

        // 无文件且无覆盖 → 明确报错
        let plain = dir.path().join("empty.mrpack");
        let f = std::fs::File::create(&plain).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("modrinth.index.json", opts).unwrap();
        w.write_all(
            br#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
        )
        .unwrap();
        w.finish().unwrap();
        let p = parse_modrinth_index(
            r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
        )
        .unwrap();
        let err = assemble_plan(
            &plain,
            "E",
            "key-c",
            "1.20.1",
            p.loader,
            p.files,
            Vec::new(),
            "u".into(),
            mc.path(),
        )
        .unwrap_err();
        assert!(err.contains("没有可安装的内容"), "空内容应报错: {err}");
    }

    /// 包名净化接入 util::sanitize_fs_name：Windows 非法字符替换 `_`，
    /// 全非法字符与 Windows 保留名回退 "modpack"。
    #[test]
    fn plan_naming_falls_back_on_illegal_or_reserved_names() {
        let dir = tempfile::tempdir().unwrap();
        let mc = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_mrpack_zip(&zip_path);
        let mk = |name: &str| {
            let parsed = parse_modrinth_index(MRPACK_INDEX).unwrap();
            assemble_plan(
                &zip_path,
                name,
                "key-d",
                "1.21.1",
                parsed.loader,
                parsed.files,
                Vec::new(),
                "u".into(),
                mc.path(),
            )
            .unwrap()
            .version_dir_name
        };
        assert!(mk(r#"My<Pack>:"X""#).starts_with("My_Pack___X_-"));
        assert_eq!(mk("***"), "modpack-1.21.1", "全非法字符回退 modpack");
        assert_eq!(mk("CON"), "modpack-1.21.1", "Windows 保留名回退 modpack");
        assert_eq!(mk("  tail.  "), "tail-1.21.1", "去尾点");
    }

    /// 同 cache_key 重装 → 复用标记目录（更新语义，sha1 跳过才生效）；
    /// 不同包不受他人标记影响，走常规 `{净化名}-{mc}` 命名。
    #[test]
    fn assemble_plan_reuses_dir_marked_with_same_cache_key() {
        let dir = tempfile::tempdir().unwrap();
        let mc = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_mrpack_zip(&zip_path);
        let mk = |key: &str| {
            let parsed = parse_modrinth_index(MRPACK_INDEX).unwrap();
            assemble_plan(
                &zip_path,
                "Demo Pack",
                key,
                "1.21.1",
                parsed.loader,
                parsed.files,
                Vec::new(),
                "u".into(),
                mc.path(),
            )
            .unwrap()
            .version_dir_name
        };

        // 预置上次安装留下的身分标记目录（含冲突后缀形态 _3）
        let existing = mc.path().join("versions").join("Demo-1.21.1_3");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(
            existing.join("elytra-pack.json"),
            r#"{"cache_key":"key-same"}"#,
        )
        .unwrap();

        assert_eq!(
            mk("key-same"),
            "Demo-1.21.1_3",
            "同包重装应复用原目录（更新语义）"
        );
        assert_eq!(
            mk("key-other"),
            "Demo Pack-1.21.1",
            "不同包不受标记影响，走常规命名"
        );
    }
}
