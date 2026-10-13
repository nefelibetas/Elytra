//! 整合包清单解析与安装计划（Modrinth `.mrpack` / CurseForge zip）。
//!
//! 两格式同构：zip = 清单 JSON + `overrides/`。Modrinth 清单自含直链；CurseForge 只有 (projectID, fileID)，
//! 需经 `community::curseforge_files` 换链（downloadUrl 为 null = 作者禁第三方分发 → 记入 restricted）。
//!
//! 本模块只做纯解析/校验与计划组装（可单测）；网络换链在 community.rs，任务编排在 task/modpack.rs。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::downloader::DownloaderConfig;
use crate::downloader::hash::{sha1_hex, sha512_hex};
use crate::game::community::{curseforge_files, modrinth_version_files};
use crate::game::resource::extract::open_zip;
use crate::game::version::manifest::fetch_version_list;
use crate::launcher::args::find_version_json;
use crate::task::{LoaderChoice, LoaderKind};
use crate::util::safe_relative_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PackSource {
    Local { path: String },
    Online {
        url: String,
        #[serde(default)]
        sha1: String,
        #[serde(default)]
        size: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackRequest {
    pub name: String,
    pub source: PackSource,
    /// 平台项目图标 URL（本地导入为 None）；安装期下载到 versions/{dir}/pack-icon.png，失败静默跳过。
    #[serde(default)]
    pub icon_url: Option<String>,
}

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

/// 解析 modrinth.index.json（规范：docs.modrinth.com/#spec/modrinth-index-json）；剔除 env.client == "unsupported"。
pub fn parse_modrinth_index(text: &str) -> Result<ModrinthParsed, String> {
    let idx: serde_json::Value =
        serde_json::from_str(text).map_err(common_err_ctx!("解析 modrinth.index.json 失败"))?;
    if idx["formatVersion"].as_i64() != Some(1) {
        bail!(
            "不支持的整合包格式版本\nformatVersion: {}",
            idx["formatVersion"]
        );
    }
    if idx["game"].as_str() != Some("minecraft") {
        bail!(
            "不是 Minecraft 整合包\ngame: {}",
            idx["game"].as_str().unwrap_or("(缺失)")
        );
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
        safe_relative_path(&path)?;
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

/// dependencies 对象 → LoaderChoice：fabric-loader / quilt-loader(旧写法 quilt_loader) / forge / neoforge；未知键报错。
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
            other => bail!("暂不支持该整合包的加载器\nloader: {other}"),
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

/// 解析 CurseForge manifest.json：manifestType == "minecraftModpack"、manifestVersion == 1；modLoaders[].id 形如 "forge-47.3.0"。
pub fn parse_curseforge_manifest(text: &str) -> Result<CfManifestParsed, String> {
    let m: serde_json::Value =
        serde_json::from_str(text).map_err(common_err_ctx!("解析 manifest.json 失败"))?;
    if m["manifestType"].as_str() != Some("minecraftModpack") {
        bail!(
            "不是 CurseForge 整合包清单\nmanifestType: {}",
            m["manifestType"].as_str().unwrap_or("(缺失)")
        );
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
                other => bail!("暂不支持该整合包的加载器\nloader: {other}"),
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

/// CF 批量换链响应（`POST /v1/mods/files` 的 data 数组）→ PackFile + 受限文件名（downloadUrl 为 null = 禁第三方分发）；algo 1 = SHA1，多余/缺失条目按 ID 匹配跳过。
pub fn resolve_cf_files(
    data: &[serde_json::Value],
    wanted: &[(i64, i64)],
) -> (Vec<PackFile>, Vec<String>) {
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

// ── 清单读取与安装计划组装 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKind {
    Modrinth,
    CurseForge,
}

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
                .map_err(common_err_ctx!("读取清单失败"))?;
            return Ok((text, kind));
        }
    }
    Err("不是有效的整合包文件\n缺少 modrinth.index.json 或 manifest.json".into())
}

/// 扫描 versions/ 下各版本目录的 elytra-pack.json 身份标记，返回与 `cache_key` 匹配的目录名。
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
    pub files: Vec<PackFile>,
    pub restricted: Vec<String>,
    pub version_url: String,
    pub minecraft_dir: &'a Path,
}

/// 组装 PackPlan（纯函数，无网络）：overrides 检测、非空校验、路径穿越防护、`{净化包名}-{mc}` 命名与 `_{n}` 冲突消解；version_url 由调用方传入。
pub fn assemble_plan(zip_path: &Path, meta: PackMeta<'_>) -> Result<PackPlan, String> {
    let PackMeta {
        pack_name,
        cache_key,
        mc_version,
        loader,
        files,
        restricted,
        version_url,
        minecraft_dir,
    } = meta;
    let mc_version = mc_version.trim().to_string();

    // overrides 目录检测（重开 zip 保持无借用纠缠，便于单测）
    let has_overrides = {
        let mut archive = open_zip(zip_path, "整合包")?;
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
    // 路径穿越防护提前到计划期：坏条目解析期即报错，不带病安装
    for f in &files {
        safe_relative_path(&f.path)?;
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

/// 读 zip 清单 → 构建 PackPlan：CF 走批量换链；version json URL 经官方清单多源回退 + TTL 缓存解析（组装细节见 assemble_plan）。
pub async fn build_plan(
    zip_path: &Path,
    request: &ModpackRequest,
    config: &DownloaderConfig,
) -> Result<PackPlan, String> {
    let mut archive = open_zip(zip_path, "整合包")?;

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
            // 一次 POST 批量换链（禁第三方分发的文件进 restricted）
            let data = curseforge_files(&ids).await?;
            let (resolved, locked) = resolve_cf_files(&data, &parsed.files);
            (parsed.mc_version, parsed.loader, resolved, locked)
        }
    };

    let manifest = fetch_version_list(config).await?;
    let trimmed = mc_version.trim();
    let version_url = manifest
        .versions
        .iter()
        .find(|v| v.id == trimmed)
        .map(|v| v.url.clone())
        .ok_or_else(|| format!("官方源未收录该 Minecraft 版本\n{trimmed}"))?;

    assemble_plan(
        zip_path,
        PackMeta {
            pack_name: &request.name,
            cache_key: &cache_key(request),
            mc_version: &mc_version,
            loader,
            files,
            restricted,
            version_url,
            minecraft_dir: &config.minecraft_dir_abs(),
        },
    )
}

// ── 导出侧（与上方导入解析对称；命令层 export_modpack 只做壳）──

#[derive(serde::Serialize)]
pub(crate) struct HostedPackFile {
    path: String,
    hashes: serde_json::Value,
    downloads: Vec<String>,
    file_size: u64,
}

/// 扫描 mods/ 下压缩类文件 → sha1 → Modrinth version_files 匹配，返回 (files[] 条目, 需排除的文件名)；网络失败/无匹配则留在 overrides 物理打包（静默降级）。
pub(crate) async fn resolve_hosted_mods(
    vdir: &Path,
) -> (Vec<HostedPackFile>, std::collections::HashSet<String>) {
    let mods_dir = vdir.join("mods");
    let Ok(rd) = std::fs::read_dir(&mods_dir) else {
        return (Vec::new(), std::collections::HashSet::new());
    };
    // 压缩/资源类文件（与 PCL2 导出集合一致）
    let mut candidates: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && matches!(
                    p.extension().and_then(|x| x.to_str()),
                    Some("jar" | "zip" | "rar" | "disabled" | "old")
                )
        })
        .collect();
    if candidates.is_empty() {
        return (Vec::new(), std::collections::HashSet::new());
    }
    let mut sha1s: Vec<String> = Vec::with_capacity(candidates.len());
    let mut kept: Vec<(PathBuf, Vec<u8>, String)> = Vec::with_capacity(candidates.len());
    for path in candidates.drain(..) {
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let sha1 = sha1_hex(&data);
        kept.push((path, data, sha1.clone()));
        sha1s.push(sha1);
    }
    if kept.is_empty() {
        return (Vec::new(), std::collections::HashSet::new());
    }
    let raw = match modrinth_version_files(&sha1s).await {
        Ok(v) => v,
        Err(_) => return (Vec::new(), std::collections::HashSet::new()), // 离线降级
    };
    let mut hosted = Vec::new();
    let mut names = std::collections::HashSet::new();
    for (path, data, sha1) in kept {
        let Some(hit) = raw.get(&sha1) else { continue };
        let Some(files) = hit.get("files").and_then(|f| f.as_array()) else {
            continue;
        };
        let Some(first) = files.first() else { continue };
        let Some(url) = first.get("url").and_then(|u| u.as_str()) else {
            continue;
        };
        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        hosted.push(HostedPackFile {
            path: format!("mods/{fname}"),
            hashes: serde_json::json!({
                "sha1": sha1,
                "sha512": sha512_hex(&data),
            }),
            downloads: vec![url.to_string()],
            file_size: data.len() as u64,
        });
        names.insert(fname);
    }
    (hosted, names)
}

/// 权威版本元数据：真实 MC / loader kind / loader 版本号（None = 无 loader 或未知）。
/// version json 优先、目录名兜底：MC = json `inheritsFrom` → `id`（剥 loader 标记）→ 目录名；
/// loader kind = json 推断 `infer_loader_from_json` → 目录名标记；版本号仅在两者同 kind 时取（防伪标记注入错误版本号）。
pub(crate) fn resolve_version_meta(
    vdir: &Path,
    version_name: &str,
) -> (String, String, Option<String>) {
    let dir_mc = crate::util::mc_version_from_dir(version_name);
    let dir_loader = crate::util::loader_from_dir(version_name);
    let Some(text) = find_version_json(vdir, version_name)
        .and_then(|p| std::fs::read_to_string(p).ok())
    else {
        // 无 json（异常目录）：目录名兜底
        let (kind, ver) = match dir_loader {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => ("vanilla".to_string(), None),
        };
        return (dir_mc.to_string(), kind, ver);
    };
    let value: Option<serde_json::Value> = serde_json::from_str(&text).ok();
    let mc = value
        .as_ref()
        .and_then(|v| v["inheritsFrom"].as_str())
        .map(str::to_string)
        .or_else(|| {
            value
                .as_ref()
                .and_then(|v| v["id"].as_str())
                .map(|s| crate::util::mc_version_from_dir(s).to_string())
        })
        .unwrap_or_else(|| dir_mc.to_string());
    let json_kind = crate::util::infer_loader_from_json(&text);
    let (kind, ver) = if json_kind != "vanilla" {
        let ver = match dir_loader {
            Some((k, v)) if k == json_kind.as_str() => Some(v.to_string()),
            _ => None,
        };
        (json_kind, ver)
    } else if let Some((k, v)) = dir_loader {
        (k.to_string(), Some(v.to_string()))
    } else {
        ("vanilla".to_string(), None)
    };
    (mc, kind, ver)
}

/// 导出标准 MRPACK（v1）：内容物理打包进 `overrides/`，`modrinth.index.json` 声明版本、loader 依赖与 hosted files[]；存档（saves）不打包。
pub(crate) fn pack_modpack(
    vdir: &Path,
    version_name: &str,
    save_path: &str,
    hosted: &[HostedPackFile],
    hosted_names: &std::collections::HashSet<String>,
) -> Result<(), String> {
    // 收集存在的顶层目录（datapacks 为 1.13+ 数据包目录，一并导出）
    let subdirs: Vec<&str> = [
        "mods",
        "config",
        "shaderpacks",
        "resourcepacks",
        "datapacks",
    ]
    .iter()
    .copied()
    .filter(|d| vdir.join(d).is_dir())
    .collect();

    let file = std::fs::File::create(save_path).map_err(common_err_ctx!("创建文件失败"))?;
    let mut writer = zip::ZipWriter::new(file);
    // mrpack 惯例 Deflated（config/资源包压缩收益明显）
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let (mc_version, kind, loader_version) = resolve_version_meta(vdir, version_name);
    let mut dependencies = serde_json::Map::new();
    dependencies.insert("minecraft".into(), serde_json::json!(mc_version));
    // loader 依赖键与导入侧 modrinth_loader 一致：fabric-loader / quilt-loader / forge / neoforge；版本号来自目录名约定，不明确时省略依赖而非写假值。
    if let Some(ver) = loader_version {
        let dep_key = match kind.as_str() {
            "fabric" => "fabric-loader",
            "quilt" => "quilt-loader",
            "forge" | "neoforge" => kind.as_str(),
            _ => "",
        };
        if !dep_key.is_empty() {
            dependencies.insert(dep_key.into(), serde_json::json!(ver));
        }
    }

    let manifest = serde_json::json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": mc_version,
        "name": version_name,
        "summary": "",
        "files": hosted,
        "dependencies": dependencies,
    });
    let content = serde_json::to_vec_pretty(&manifest).map_err(common_err_ctx!("序列化失败"))?;
    writer
        .start_file("modrinth.index.json", opts)
        .map_err(common_err_ctx!("写入清单失败"))?;
    writer
        .write_all(&content)
        .map_err(common_err_ctx!("写入清单失败"))?;

    for sub in subdirs {
        // 在线分发 mod 不物理打包进 overrides
        let skip = if sub == "mods" {
            Some(hosted_names)
        } else {
            None
        };
        add_dir_to_zip(
            &mut writer,
            &vdir.join(sub),
            &format!("overrides/{sub}"),
            opts,
            skip,
        )?;
    }
    writer.finish().map_err(common_err_ctx!("写入 zip 失败"))?;
    Ok(())
}

/// 递归写目录到 zip（保留相对路径；目录条目本身也记录，空目录才可保留）。
fn add_dir_to_zip(
    writer: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
    skip_files: Option<&std::collections::HashSet<String>>,
) -> Result<(), String> {
    writer
        .add_directory(format!("{prefix}/"), opts)
        .map_err(|err| format!("写入目录失败\n{err}"))?;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = format!("{}/{}", prefix, e.file_name().to_string_lossy());
        // 顶层 hosted 文件跳过（在线分发，不物理打包）
        if path.is_file()
            && skip_files
                .is_some_and(|set| set.contains(&e.file_name().to_string_lossy().to_string()))
        {
            continue;
        }
        if path.is_dir() {
            add_dir_to_zip(writer, &path, &name, opts, None)?;
        } else if path.is_file() {
            let mut f = std::fs::File::open(&path).map_err(|err| format!("打开文件失败\n{err}"))?;
            writer
                .start_file(name, opts)
                .map_err(|err| format!("写入文件失败\n{err}"))?;
            std::io::copy(&mut f, writer).map_err(|err| format!("写入文件失败\n{err}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// assemble_plan：临时 zip + 清单文本 → 计划字段，冲突 `_{n}` 后缀，空内容报错。
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
            PackMeta {
                pack_name: r#"Demo <Pack>"#,
                cache_key: "key-a",
                mc_version: " 1.21.1 ",
                loader: parsed.loader,
                files: parsed.files,
                restricted: Vec::new(),
                version_url: "https://piston-meta/v1.json".into(),
                minecraft_dir: mc.path(),
            },
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

        std::fs::create_dir_all(mc.path().join("versions").join("Demo _Pack_-1.21.1")).unwrap();
        let reparsed = parse_modrinth_index(&text).unwrap();
        let plan2 = assemble_plan(
            &zip_path,
            PackMeta {
                pack_name: r#"Demo <Pack>"#,
                cache_key: "key-b",
                mc_version: "1.21.1",
                loader: reparsed.loader,
                files: reparsed.files,
                restricted: Vec::new(),
                version_url: "u".into(),
                minecraft_dir: mc.path(),
            },
        )
        .unwrap();
        assert_eq!(plan2.version_dir_name, "Demo _Pack_-1.21.1_1");

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
            PackMeta {
                pack_name: "E",
                cache_key: "key-c",
                mc_version: "1.20.1",
                loader: p.loader,
                files: p.files,
                restricted: Vec::new(),
                version_url: "u".into(),
                minecraft_dir: mc.path(),
            },
        )
        .unwrap_err();
        assert!(err.contains("没有可安装的内容"), "空内容应报错: {err}");
    }

    /// 包名净化走 util::sanitize_fs_name：Windows 非法字符替换 `_`，全非法字符与保留名回退 "modpack"。
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
                PackMeta {
                    pack_name: name,
                    cache_key: "key-d",
                    mc_version: "1.21.1",
                    loader: parsed.loader,
                    files: parsed.files,
                    restricted: Vec::new(),
                    version_url: "u".into(),
                    minecraft_dir: mc.path(),
                },
            )
            .unwrap()
            .version_dir_name
        };
        assert!(mk(r#"My<Pack>:"X""#).starts_with("My_Pack___X_-"));
        assert_eq!(mk("***"), "modpack-1.21.1", "全非法字符回退 modpack");
        assert_eq!(mk("CON"), "modpack-1.21.1", "Windows 保留名回退 modpack");
        assert_eq!(mk("  tail.  "), "tail-1.21.1", "去尾点");
    }

    /// 同 cache_key 重装复用标记目录（更新语义，sha1 跳过才生效）；不同包走常规 `{净化名}-{mc}` 命名。
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
                PackMeta {
                    pack_name: "Demo Pack",
                    cache_key: key,
                    mc_version: "1.21.1",
                    loader: parsed.loader,
                    files: parsed.files,
                    restricted: Vec::new(),
                    version_url: "u".into(),
                    minecraft_dir: mc.path(),
                },
            )
            .unwrap()
            .version_dir_name
        };

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

    /// install_modpack 入参契约：`{ request: { name, source } }`，`source` 为 serde 内部标签（`type: "local" | "online"`）；Windows 反斜杠路径必须原样反序列化。
    #[test]
    fn request_deserializes_frontend_payload() {
        let req: ModpackRequest = serde_json::from_str(
            r#"{"name":"愚者0.3.2","source":{"type":"local","path":"C:\\environment\\愚者0.3.2.zip"}}"#,
        )
        .expect("前端本地包载荷应能反序列化");
        match req.source {
            PackSource::Local { path } => assert_eq!(path, r"C:\environment\愚者0.3.2.zip"),
            other => panic!("来源类型应为 local：{other:?}"),
        }

        let online: ModpackRequest = serde_json::from_str(
            r#"{"name":"P","source":{"type":"online","url":"https://x/a.mrpack","sha1":"","size":0},"icon_url":"https://x/i.png"}"#,
        )
        .expect("在线包载荷应能反序列化");
        assert!(matches!(online.source, PackSource::Online { .. }));
        assert_eq!(online.icon_url.as_deref(), Some("https://x/i.png"));
    }
}
