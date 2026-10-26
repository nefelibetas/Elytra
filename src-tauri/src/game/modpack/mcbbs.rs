//! MCBBS 清单（`mcbbs.packmeta` 或含 `addons` 的旧版 `manifest.json`）。
//!
//! `addons` 声明 game 与 loader；`files` 分 addon（相对 overrides 的 path + sha1，走 `fileApi`）
//! 与 curse（projectID/fileID，走 CF API）。`libraries`/`launchInfo` 暂不消费。

use crate::task::{LoaderChoice, LoaderKind};
use crate::util::{safe_relative_path, sanitize_fs_name};

use super::detect::{existing_overrides, Detected};
use super::model::{OverridesRef, PackFile, PackMetaInfo};

#[derive(Debug)]
pub struct McbbsCurseFile {
    pub project_id: i64,
    pub file_id: i64,
    pub file_name: Option<String>,
}

#[derive(Debug)]
pub struct McbbsParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub optifine: Option<String>,
    /// addon 直链文件（`fileApi` 非空时生成；包内覆盖目录有同 path 副本时由 C0 优先复用）
    pub files: Vec<PackFile>,
    pub curse: Vec<McbbsCurseFile>,
    pub overrides: Vec<OverridesRef>,
    pub meta: PackMetaInfo,
}

pub fn parse_mcbbs_pack(detected: &Detected, zip_path: &str) -> Result<McbbsParsed, String> {
    let value: serde_json::Value = serde_json::from_str(&detected.manifest).map_err(
        common_err_ctx_key!("errors.install.modpack", "解析 MCBBS 清单失败"),
    )?;

    let mut mc_version = None;
    let mut loader = None;
    let mut optifine = None;
    if let Some(addons) = value["addons"].as_array() {
        for addon in addons {
            let id = addon["id"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            let version = addon["version"].as_str().map(str::trim).unwrap_or("");
            if version.is_empty() {
                continue;
            }
            match id.as_str() {
                "game" | "minecraft" => mc_version = Some(version.to_string()),
                "forge" => set_loader(&mut loader, LoaderKind::Forge, version),
                "neoforge" => set_loader(&mut loader, LoaderKind::NeoForge, version),
                "fabric" => set_loader(&mut loader, LoaderKind::Fabric, version),
                "quilt" => set_loader(&mut loader, LoaderKind::Quilt, version),
                "optifine" => optifine = Some(version.to_string()),
                "liteloader" => bail_key!(
                    "errors.modpack.mcbbsUnsupportedLoader",
                    "暂不支持该 MCBBS 整合包的加载器\nid: {id}"
                ),
                _ => {}
            }
        }
    }
    let mc_version = mc_version.ok_or_else(|| {
        keyed_msg!(
            "errors.modpack.mcbbsMissingGame",
            "MCBBS 清单缺少 game 组件"
        )
    })?;

    let file_api = value["fileApi"]
        .as_str()
        .map(str::trim)
        .unwrap_or_default()
        .trim_end_matches('/');
    let mut files = Vec::new();
    let mut curse = Vec::new();
    if let Some(entries) = value["files"].as_array() {
        for entry in entries {
            if let Some(path) = entry["path"]
                .as_str()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                safe_relative_path(path)?;
                if !file_api.is_empty() {
                    files.push(PackFile {
                        path: path.to_string(),
                        url: format!("{file_api}/overrides/{path}"),
                        sha1: entry["hash"].as_str().unwrap_or_default().to_string(),
                        size: 0,
                    });
                }
                continue;
            }

            let (Some(project_id), Some(file_id)) =
                (entry["projectID"].as_i64(), entry["fileID"].as_i64())
            else {
                continue;
            };
            let file_name = entry["fileName"].as_str().map(str::to_string);
            let direct = entry["url"].as_str().map(str::trim).filter(|url| {
                (url.starts_with("https://") || url.starts_with("http://"))
                    && !url.contains("curseforge.com/")
            });
            if let Some(url) = direct {
                let raw = file_name
                    .clone()
                    .unwrap_or_else(|| format!("{project_id}-{file_id}.jar"));
                let name = sanitize_fs_name(&raw, "download.jar");
                let path = format!("mods/{name}");
                if safe_relative_path(&path).is_ok() {
                    files.push(PackFile {
                        path,
                        url: url.to_string(),
                        sha1: String::new(),
                        size: 0,
                    });
                    continue;
                }
            }
            curse.push(McbbsCurseFile {
                project_id,
                file_id,
                file_name,
            });
        }
    }

    let overrides = existing_overrides(
        &detected.members,
        zip_path,
        &[format!("{}overrides/", detected.base)],
    );

    Ok(McbbsParsed {
        mc_version,
        loader,
        optifine,
        files,
        curse,
        overrides,
        meta: PackMetaInfo {
            name: value["name"].as_str().unwrap_or_default().to_string(),
            version: value["version"].as_str().unwrap_or_default().to_string(),
            author: value["author"].as_str().unwrap_or_default().to_string(),
        },
    })
}

fn set_loader(slot: &mut Option<LoaderChoice>, kind: LoaderKind, version: &str) {
    if slot.is_none() {
        *slot = Some(LoaderChoice {
            kind,
            version: version.to_string(),
        });
    }
}
