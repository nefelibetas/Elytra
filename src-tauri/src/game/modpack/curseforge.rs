//! CurseForge `manifest.json` 解析（含旧版 MCBBS 清单的区分）。

use std::collections::HashMap;

use crate::task::{LoaderChoice, LoaderKind};
use crate::util::{safe_relative_path, sanitize_fs_name};

use super::model::{PackFile, PackMetaInfo};

/// CF 文件引用：标准形态只有 (projectID, fileID)；FTB 等变体带直链与文件名。
#[derive(Debug)]
pub struct CfFileEntry {
    pub project_id: i64,
    pub file_id: i64,
    pub file_name: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug)]
pub struct CfManifestParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub files: Vec<CfFileEntry>,
    /// overrides 目录（不带尾部 `/`；manifest 的 `overrides` 字段可自定义）
    pub overrides_dir: String,
    pub meta: PackMetaInfo,
}

/// 解析 CurseForge manifest.json：manifestType == "minecraftModpack"；modLoaders[].id 形如 "forge-47.3.0"。
pub fn parse_curseforge_manifest(text: &str) -> Result<CfManifestParsed, String> {
    let m: serde_json::Value = serde_json::from_str(text).map_err(common_err_ctx_key!(
        "errors.install.modpack",
        "解析 manifest.json 失败"
    ))?;
    if m["manifestType"].as_str() != Some("minecraftModpack") {
        bail_key!(
            "errors.modpack.notCurseforge",
            "不是 CurseForge 整合包清单\nmanifestType: {}",
            m["manifestType"].as_str().unwrap_or("(缺失)")
        );
    }
    // 字段缺失的旧包仍可解析；显式声明非 1 才拒绝
    if let Some(version) = m["manifestVersion"].as_i64() {
        if version != 1 {
            bail_key!(
                "errors.modpack.unsupportedManifestVersion",
                "不支持的 CurseForge 清单版本\nmanifestVersion: {version}"
            );
        }
    }
    let mc_version = m["minecraft"]["version"]
        .as_str()
        .ok_or_else(|| {
            keyed_msg!(
                "errors.modpack.missingMinecraftVersion",
                "清单缺少 Minecraft 版本（minecraft.version）\n旧版 MCBBS 清单请用对应启动器导入"
            )
        })?
        .to_string();

    let loader = m["minecraft"]["modLoaders"]
        .as_array()
        .and_then(|loaders| {
            // primary 标记主加载器；缺失时取第一个可解析的
            loaders
                .iter()
                .find(|loader| loader["primary"].as_bool() == Some(true))
                .or_else(|| loaders.first())
        })
        .and_then(|loader| loader["id"].as_str())
        .map(parse_loader_id)
        .transpose()?;

    let overrides_dir = normalize_overrides_dir(m["overrides"].as_str())?;

    let mut files = Vec::new();
    for f in m["files"]
        .as_array()
        .ok_or_else(|| keyed_msg!("errors.modpack.missingFiles", "清单缺少 files 数组"))?
    {
        let pid = f["projectID"].as_i64().ok_or_else(|| {
            keyed_msg!(
                "errors.modpack.fileMissingProjectId",
                "文件条目缺少 projectID"
            )
        })?;
        let fid = f["fileID"]
            .as_i64()
            .ok_or_else(|| keyed_msg!("errors.modpack.fileMissingFileId", "文件条目缺少 fileID"))?;
        files.push(CfFileEntry {
            project_id: pid,
            file_id: fid,
            file_name: f["fileName"].as_str().map(str::to_string),
            url: f["url"].as_str().map(str::to_string),
        });
    }
    Ok(CfManifestParsed {
        mc_version,
        loader,
        files,
        overrides_dir,
        meta: PackMetaInfo {
            name: m["name"].as_str().unwrap_or_default().to_string(),
            version: m["version"].as_str().unwrap_or_default().to_string(),
            author: m["author"].as_str().unwrap_or_default().to_string(),
        },
    })
}

fn parse_loader_id(id: &str) -> Result<LoaderChoice, String> {
    let (kind_str, version) = id.split_once('-').ok_or_else(|| {
        keyed_msg!(
            "errors.modpack.badModLoaderId",
            "modLoader id 格式异常\n{id}"
        )
    })?;
    let kind = match kind_str {
        "forge" => LoaderKind::Forge,
        "fabric" => LoaderKind::Fabric,
        "quilt" => LoaderKind::Quilt,
        "neoforge" => LoaderKind::NeoForge,
        other => bail_key!(
            "errors.modpack.unsupportedLoader",
            "暂不支持该整合包的加载器\nloader: {other}"
        ),
    };
    Ok(LoaderChoice {
        kind,
        version: version.to_string(),
    })
}

/// manifest 的 `overrides` 目录名（默认 `overrides`；拒绝绝对路径与 `..`）。
fn normalize_overrides_dir(raw: Option<&str>) -> Result<String, String> {
    let raw = raw.map(str::trim).unwrap_or_default();
    let dir = if raw.is_empty() {
        "overrides"
    } else {
        raw.trim_start_matches("./").trim_matches('/')
    };
    if dir.is_empty() {
        bail_key!(
            "errors.modpack.badOverridesDir",
            "整合包覆盖目录名无效\n(空)"
        );
    }
    if safe_relative_path(dir).is_err() {
        bail_key!(
            "errors.modpack.badOverridesDir",
            "整合包覆盖目录名无效\n{dir}"
        );
    }
    Ok(dir.to_string())
}

/// 清单自带直链的文件（FTB legacy 等）：固定落到 `mods/`，文件名取 `fileName` 或 URL 末段。
pub fn direct_pack_file(entry: &CfFileEntry) -> Option<PackFile> {
    let url = entry
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))?;
    let raw_name = entry
        .file_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| url_file_name(url))?;
    let name = sanitize_fs_name(&raw_name, "download.jar");
    let path = format!("mods/{name}");
    safe_relative_path(&path).ok()?;
    Some(PackFile {
        path,
        url: url.to_string(),
        sha1: String::new(),
        size: 0,
    })
}

fn url_file_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let last = path.rsplit('/').next()?.trim();
    if last.is_empty() {
        return None;
    }
    Some(percent_decode(last))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
        let raw_name = d["fileName"].as_str().unwrap_or("unknown.jar");
        match d["downloadUrl"].as_str() {
            Some(url) => {
                let name = sanitize_fs_name(raw_name, "download.jar");
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
            None => restricted.push(raw_name.to_string()),
        }
    }
    (files, restricted)
}
