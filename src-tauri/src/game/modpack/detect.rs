//! 整合包格式嗅探：按标记文件判定，与扩展名无关。
//!
//! 标记优先级：`modrinth.index.json` → `mcbbs.packmeta`（MCBBS）→ `manifest.json`（含 `addons` 为旧版 MCBBS，
//! 否则 CurseForge）→ `mmc-pack.json` / `instance.cfg`（MultiMC/Prism）→ `modpack.json`（HMCL）→ 内嵌 `modpack.zip`（PCL）
//! → 唯一 `versions/<id>/<id>.json`（完整目录）。

use std::collections::BTreeSet;
use std::io::Read;

use zip::ZipArchive;

use super::model::{OverridesRef, PackKind};

/// 单份清单读取上限（防 zip 炸弹；正常清单远小于此）。
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) struct Detected {
    pub kind: PackKind,
    /// 包根前缀（"" 或 "Dir/"）
    pub base: String,
    /// 主清单文本（modrinth.index.json / manifest.json / mmc-pack.json）
    pub manifest: String,
    /// MultiMC 旧格式的 instance.cfg
    pub instance_cfg: Option<String>,
    /// MultiMC 实例内容目录 / 目录包的版本目录（相对 base）
    pub content_dir: Option<String>,
    /// PCL 带启动器包的内嵌成员名（`modpack.zip` / `modpack.mrpack`）
    pub embedded: Option<String>,
    /// HMCL `minecraft/pack.json`（版本 JSON；其他格式为空）
    pub version_json: Option<String>,
    /// zip 成员名（已过滤系统垃圾）
    pub members: Vec<String>,
}

pub(crate) fn detect_pack(archive: &mut ZipArchive<std::fs::File>) -> Result<Detected, String> {
    let members: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
        .filter(|name| !is_noise(name))
        .collect();

    let mut hits = Vec::new();
    for base in candidate_bases(&members) {
        if let Some(found) = detect_in_base(archive, &members, &base)? {
            hits.push(found);
        }
    }

    match hits.len() {
        0 => match find_directory_pack(archive, &members)? {
            Some(found) => Ok(found),
            None => Err(unsupported_notice(&members)),
        },
        1 => {
            let mut found = hits.remove(0);
            found.members = members;
            Ok(found)
        }
        _ => Err(keyed_msg!(
            "errors.modpack.multiplePacks",
            "包内存在多个整合包\n请解压后只保留一个"
        )),
    }
}

/// 候选包根：根目录优先，其次含标记文件的一级目录（避免遍历所有顶层目录）。
fn candidate_bases(members: &[String]) -> Vec<String> {
    const MARKERS: [&str; 8] = [
        "modrinth.index.json",
        "mcbbs.packmeta",
        "manifest.json",
        "mmc-pack.json",
        "instance.cfg",
        "modpack.json",
        "modpack.zip",
        "modpack.mrpack",
    ];
    let mut bases = vec![String::new()];
    let mut seen = BTreeSet::new();
    for name in members {
        let Some((first, rest)) = name.split_once('/') else {
            continue;
        };
        if first.is_empty() || first == "." || first == ".." {
            continue;
        }
        if MARKERS.contains(&rest) {
            seen.insert(format!("{first}/"));
        }
    }
    bases.extend(seen);
    bases
}

/// 完整游戏目录：恰好一个 `versions/<id>/<id>.json`（允许 `.minecraft/` 等一层前缀）。
fn find_directory_pack(
    archive: &mut ZipArchive<std::fs::File>,
    members: &[String],
) -> Result<Option<Detected>, String> {
    let mut hits: Vec<(String, String, String)> = Vec::new();
    for name in members {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() < 3 {
            continue;
        }
        let (Some(file), Some(id), Some(versions)) = (
            parts.last(),
            parts.get(parts.len() - 2),
            parts.get(parts.len() - 3),
        ) else {
            continue;
        };
        if *versions != "versions" || file.strip_suffix(".json") != Some(*id) {
            continue;
        }
        let base = if parts.len() == 3 {
            String::new()
        } else {
            format!("{}/", parts[..parts.len() - 3].join("/"))
        };
        hits.push((base, (*id).to_string(), name.clone()));
    }
    hits.sort();
    hits.dedup();
    match hits.len() {
        0 => Ok(None),
        1 => {
            let (base, id, member) = hits.remove(0);
            Ok(Some(Detected {
                kind: PackKind::Directory,
                base,
                manifest: read_member(archive, &member)?,
                instance_cfg: None,
                content_dir: Some(format!("versions/{id}/")),
                embedded: None,
                version_json: None,
                members: members.to_vec(),
            }))
        }
        _ => Err(keyed_msg!(
            "errors.modpack.multipleVersions",
            "包内存在多个游戏版本\n请只保留一个"
        )),
    }
}

fn detect_in_base(
    archive: &mut ZipArchive<std::fs::File>,
    members: &[String],
    base: &str,
) -> Result<Option<Detected>, String> {
    let has = |name: &str| members.iter().any(|member| member == name);
    let full = |name: &str| format!("{base}{name}");

    if has(&full("modrinth.index.json")) {
        return Ok(Some(Detected {
            kind: PackKind::Modrinth,
            base: base.to_string(),
            manifest: read_member(archive, &full("modrinth.index.json"))?,
            instance_cfg: None,
            content_dir: None,
            embedded: None,
            version_json: None,
            members: Vec::new(),
        }));
    }

    if has(&full("mcbbs.packmeta")) {
        return Ok(Some(Detected {
            kind: PackKind::Mcbbs,
            base: base.to_string(),
            manifest: read_member(archive, &full("mcbbs.packmeta"))?,
            instance_cfg: None,
            content_dir: None,
            embedded: None,
            version_json: None,
            members: Vec::new(),
        }));
    }

    if has(&full("manifest.json")) {
        let text = read_member(archive, &full("manifest.json"))?;
        if has_addons(&text) {
            return Ok(Some(Detected {
                kind: PackKind::Mcbbs,
                base: base.to_string(),
                manifest: text,
                instance_cfg: None,
                content_dir: None,
                embedded: None,
                version_json: None,
                members: Vec::new(),
            }));
        }
        return Ok(Some(Detected {
            kind: PackKind::CurseForge,
            base: base.to_string(),
            manifest: text,
            instance_cfg: None,
            content_dir: None,
            embedded: None,
            version_json: None,
            members: Vec::new(),
        }));
    }

    let has_mmc_pack = has(&full("mmc-pack.json"));
    let has_instance_cfg = has(&full("instance.cfg"));
    if has_mmc_pack || has_instance_cfg {
        let manifest = if has_mmc_pack {
            read_member(archive, &full("mmc-pack.json"))?
        } else {
            String::new()
        };
        let instance_cfg = if has_instance_cfg {
            Some(read_member(archive, &full("instance.cfg"))?)
        } else {
            None
        };
        let content_dir = ["minecraft/", ".minecraft/"]
            .into_iter()
            .find(|dir| {
                members
                    .iter()
                    .any(|member| member.starts_with(&format!("{base}{dir}")))
            })
            .map(str::to_string);
        return Ok(Some(Detected {
            kind: PackKind::MultiMc,
            base: base.to_string(),
            manifest,
            instance_cfg,
            content_dir,
            embedded: None,
            version_json: None,
            members: Vec::new(),
        }));
    }

    if has(&full("modpack.json")) {
        let has_content = members
            .iter()
            .any(|member| member.starts_with(&format!("{base}minecraft/")));
        if !has_content {
            return Err(keyed_msg!(
                "errors.modpack.hmclMissingMinecraft",
                "HMCL 整合包缺少 minecraft 目录"
            ));
        }
        let version_json = if members
            .iter()
            .any(|member| member == &format!("{base}minecraft/pack.json"))
        {
            Some(read_member(archive, &full("minecraft/pack.json"))?)
        } else {
            None
        };
        return Ok(Some(Detected {
            kind: PackKind::Hmcl,
            base: base.to_string(),
            manifest: read_member(archive, &full("modpack.json"))?,
            instance_cfg: None,
            content_dir: Some("minecraft/".to_string()),
            embedded: None,
            version_json,
            members: Vec::new(),
        }));
    }

    // PCL 带启动器包：外层 zip 内嵌真正的整合包
    for marker in ["modpack.zip", "modpack.mrpack"] {
        if has(&full(marker)) {
            return Ok(Some(Detected {
                kind: PackKind::PclLauncher,
                base: base.to_string(),
                manifest: String::new(),
                instance_cfg: None,
                content_dir: None,
                embedded: Some(full(marker)),
                version_json: None,
                members: Vec::new(),
            }));
        }
    }

    Ok(None)
}

fn has_addons(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| value.get("addons").map(serde_json::Value::is_array))
        .unwrap_or(false)
}

/// 已存在的覆盖目录（按 `prefixes` 顺序保留；不读内容）。
pub(crate) fn existing_overrides(
    members: &[String],
    zip_path: &str,
    prefixes: &[String],
) -> Vec<OverridesRef> {
    prefixes
        .iter()
        .filter(|prefix| {
            members
                .iter()
                .any(|member| member.starts_with(prefix.as_str()))
        })
        .map(|prefix| OverridesRef {
            zip_path: zip_path.to_string(),
            prefix: prefix.clone(),
            skip: Vec::new(),
        })
        .collect()
}

fn read_member(archive: &mut ZipArchive<std::fs::File>, name: &str) -> Result<String, String> {
    let entry = archive
        .by_name(name)
        .map_err(common_err_ctx_key!("errors.file.read", "读取清单失败"))?;
    if entry.size() > MAX_MANIFEST_BYTES {
        bail_key!("errors.modpack.manifestTooLarge", "整合包清单过大\n{name}");
    }
    let mut text = String::new();
    entry
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(common_err_ctx_key!("errors.file.read", "读取清单失败"))?;
    Ok(text)
}

fn is_noise(name: &str) -> bool {
    name.starts_with("__MACOSX/")
        || name == ".DS_Store"
        || name.ends_with("/.DS_Store")
        || name == "Thumbs.db"
        || name.ends_with("/Thumbs.db")
}

fn unsupported_notice(members: &[String]) -> String {
    let has_suffix = |suffix: &str| members.iter().any(|name| name.ends_with(suffix));
    if has_suffix("server-manifest.json") {
        return keyed_msg!(
            "errors.modpack.serverPackUnsupported",
            "识别为服务器自动更新包，暂不支持导入"
        );
    }
    keyed_msg!(
        "errors.modpack.notModpack",
        "不是有效的整合包文件\n支持 Modrinth / CurseForge / MultiMC-Prism / HMCL / MCBBS / PCL / 完整游戏目录"
    )
}
