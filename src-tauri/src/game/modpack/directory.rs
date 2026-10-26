//! 完整游戏目录包（`versions/<id>/<id>.json`）解析。
//!
//! 只接受唯一版本目录；版本目录作为覆盖前缀，`libraries/assets/logs` 等共享目录不落盘。

use crate::util::{infer_loader_from_json, mc_version_from_dir};

use super::detect::Detected;
use super::model::{OverridesRef, PackMetaInfo};
use super::version_meta::loader_from_version_json;
use crate::task::LoaderChoice;

pub struct DirectoryParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub overrides: Vec<OverridesRef>,
    pub meta: PackMetaInfo,
}

/// 解析版本 JSON：`inheritsFrom` 优先，其次从版本 id 剥离 loader 标记。
pub fn parse_directory_pack(
    detected: &Detected,
    zip_path: &str,
) -> Result<DirectoryParsed, String> {
    let value: serde_json::Value = serde_json::from_str(&detected.manifest).map_err(
        common_err_ctx_key!("errors.install.modpack", "解析版本 JSON 失败"),
    )?;
    let content_dir = detected.content_dir.as_deref().ok_or_else(|| {
        keyed_msg!(
            "errors.modpack.directoryMissingContent",
            "完整游戏目录缺少版本目录"
        )
    })?;
    let id = value["id"]
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            content_dir
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .map(str::to_string)
        })
        .ok_or_else(|| keyed_msg!("errors.modpack.directoryMissingId", "版本 JSON 缺少 id"))?;

    let mc_version = value["inheritsFrom"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| mc_version_from_dir(&id).to_string());

    let loader = loader_from_version_json(&detected.manifest);
    let inferred = infer_loader_from_json(&detected.manifest);
    if loader.is_none() && inferred != "vanilla" {
        bail_key!(
            "errors.modpack.unsupportedLoader",
            "暂不支持该整合包的加载器\nloader: {inferred}"
        );
    }

    let prefix = format!("{}{}", detected.base, content_dir);
    let skip = [
        "libraries/",
        "assets/",
        "logs/",
        "crash-reports/",
        "backups/",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    Ok(DirectoryParsed {
        mc_version,
        loader,
        overrides: vec![OverridesRef {
            zip_path: zip_path.to_string(),
            prefix,
            skip,
        }],
        meta: PackMetaInfo {
            name: id,
            version: String::new(),
            author: String::new(),
        },
    })
}
