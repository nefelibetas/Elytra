//! HMCL 原生包（`modpack.json` + `minecraft/pack.json`）。
//!
//! `pack.json` 是 HMCL 的 `GameInstanceManifest`（可能含 `patches[]`），只用来提取 MC/loader；
//! 游戏本体仍按官方版本清单重装，`minecraft/` 下除清单外的文件整体作为覆盖内容。

use crate::task::LoaderChoice;
use crate::util::{infer_loader_from_json, mc_version_from_dir};

use super::detect::Detected;
use super::model::{OverridesRef, PackMetaInfo};
use super::version_meta::loader_from_version_json;

/// HMCL 导出默认排除的共享/日志目录；`pack.json` 绝不能落进实例目录。
const SKIP: [&str; 7] = [
    "pack.json",
    "libraries/",
    "assets/",
    "versions/",
    "logs/",
    "crash-reports/",
    "backups/",
];

#[derive(Debug)]
pub struct HmclParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub overrides: Vec<OverridesRef>,
    pub meta: PackMetaInfo,
}

pub fn parse_hmcl_pack(detected: &Detected, zip_path: &str) -> Result<HmclParsed, String> {
    let info: serde_json::Value = serde_json::from_str(&detected.manifest).map_err(
        common_err_ctx_key!("errors.install.modpack", "解析 modpack.json 失败"),
    )?;
    let version_value = detected
        .version_json
        .as_deref()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());

    let mut mc_version = version_value
        .as_ref()
        .and_then(|value| value["inheritsFrom"].as_str().map(str::to_string));
    if mc_version.is_none() {
        mc_version = info["gameVersion"]
            .as_str()
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .map(str::to_string);
    }
    if mc_version.is_none() {
        mc_version = version_value
            .as_ref()
            .and_then(|value| value["id"].as_str())
            .map(|id| mc_version_from_dir(id).to_string());
    }
    let mc_version = mc_version.ok_or_else(|| {
        keyed_msg!(
            "errors.modpack.missingMinecraftVersion",
            "清单缺少 Minecraft 版本（modpack.json.gameVersion）"
        )
    })?;

    let loader = detected
        .version_json
        .as_deref()
        .and_then(loader_from_version_json);
    if let Some(text) = detected.version_json.as_deref() {
        if loader.is_none() && infer_loader_from_json(text) != "vanilla" {
            bail_key!(
                "errors.modpack.unsupportedLoader",
                "暂不支持该整合包的加载器\n无法从 pack.json 提取版本"
            );
        }
    }

    let content_dir = detected.content_dir.as_deref().unwrap_or("minecraft/");
    let prefix = format!("{}{}", detected.base, content_dir);
    let overrides = vec![OverridesRef {
        zip_path: zip_path.to_string(),
        prefix,
        skip: SKIP.into_iter().map(str::to_string).collect(),
    }];

    Ok(HmclParsed {
        mc_version,
        loader,
        overrides,
        meta: PackMetaInfo {
            name: info["name"].as_str().unwrap_or_default().to_string(),
            version: info["version"].as_str().unwrap_or_default().to_string(),
            author: info["author"].as_str().unwrap_or_default().to_string(),
        },
    })
}
