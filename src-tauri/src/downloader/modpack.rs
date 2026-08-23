//! 整合包清单解析与安装计划（Modrinth `.mrpack` / CurseForge modpack zip）。
//!
//! 两种格式同构：zip = 清单 JSON + `overrides/` 目录。差异在文件下载地址：
//! Modrinth 清单自包含直链（零 API 调用）；CurseForge 只有 (projectID, fileID)
//! 数字引用，需经 `community::curseforge_files` 批量换链（作者关闭第三方分发
//! 时 downloadUrl 为 null → 记入 restricted，任务收尾提示手动下载）。
//!
//! 本模块只做纯解析/校验（可单测），网络换链在 community.rs，
//! 任务编排（下载/解压/进度）在 task/modpack.rs。

use crate::task::{LoaderChoice, LoaderKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
            sha1: f["hashes"]["sha1"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
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
    let by_id: HashMap<i64, &serde_json::Value> =
        data.iter().filter_map(|d| d["id"].as_i64().map(|i| (i, d))).collect();
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

/// 整合包展示名 → 版本目录名安全段：非法文件名字符替换 `_`
/// （对齐 files.rs sanitize_world_name；Windows 保留尾点处理一致）。
pub fn sanitize_pack_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || c.is_control()
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    let no_tail_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
    if no_tail_dot.is_empty() || no_tail_dot.chars().all(|c| c == '_') {
        "modpack".to_string()
    } else {
        no_tail_dot.to_string()
    }
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

    #[test]
    fn safe_rel_path_blocks_traversal() {
        assert!(safe_rel_path("mods/a.jar").is_ok());
        assert!(safe_rel_path("config/nested/b.json").is_ok());
        assert!(safe_rel_path("../evil").is_err());
        assert!(safe_rel_path("/abs").is_err());
        assert!(safe_rel_path("\\abs").is_err());
        assert!(safe_rel_path("").is_err());
    }

    #[test]
    fn sanitize_pack_name_handles_windows_forbidden() {
        // Windows 非法字符逐个替换 '_'（<>:"/\|?* 与控制字符）
        let out = sanitize_pack_name(r#"My<Pack>:"X""#);
        assert_eq!(out, r#"My_Pack___X_"#);
        assert!(!out.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']));
        assert_eq!(sanitize_pack_name("  tail.  "), "tail");
        assert_eq!(sanitize_pack_name("***"), "modpack");
    }
}
