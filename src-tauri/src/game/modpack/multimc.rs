//! MultiMC / Prism 实例包解析（`mmc-pack.json` 组件 + `instance.cfg` 回退）。

use crate::task::{LoaderChoice, LoaderKind};

use super::detect::Detected;
use super::model::{OverridesRef, PackMetaInfo};

#[derive(Debug)]
pub struct MmcParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub optifine: Option<String>,
    pub overrides: Vec<OverridesRef>,
    pub meta: PackMetaInfo,
}

/// 解析 MMC 实例：MC/loader 来自 `mmc-pack.json` 组件，缺失时回退 `instance.cfg` 的 `IntendedVersion`。
pub fn parse_mmc_pack(detected: &Detected, zip_path: &str) -> Result<MmcParsed, String> {
    let mut mc_version = None;
    let mut loader: Option<LoaderChoice> = None;
    let mut optifine: Option<String> = None;

    if !detected.manifest.is_empty() {
        let pack: serde_json::Value = serde_json::from_str(&detected.manifest).map_err(
            common_err_ctx_key!("errors.install.modpack", "解析 mmc-pack.json 失败"),
        )?;
        if let Some(components) = pack["components"].as_array() {
            for component in components {
                let Some(uid) = component["uid"].as_str() else {
                    continue;
                };
                let version = component["version"]
                    .as_str()
                    .map(str::trim)
                    .filter(|version| !version.is_empty());
                let Some(version) = version else {
                    continue;
                };
                match uid {
                    "net.minecraft" => mc_version = Some(version.to_string()),
                    "net.minecraftforge" => set_loader(&mut loader, LoaderKind::Forge, version),
                    "net.neoforged" => set_loader(&mut loader, LoaderKind::NeoForge, version),
                    "net.fabricmc.fabric-loader" => {
                        set_loader(&mut loader, LoaderKind::Fabric, version)
                    }
                    "org.quiltmc.quilt-loader" => {
                        set_loader(&mut loader, LoaderKind::Quilt, version)
                    }
                    "optifine" | "net.optifine" => {
                        optifine = Some(version.to_string());
                    }
                    "com.mumfrey.liteloader" => bail_key!(
                        "errors.modpack.mmcUnsupportedLoader",
                        "暂不支持该 MultiMC 整合包的加载器\nLiteLoader {version}"
                    ),
                    _ => {}
                }
            }
        }
    }

    let instance_cfg = detected.instance_cfg.as_deref();
    if mc_version.is_none() {
        mc_version = instance_cfg.and_then(|cfg| instance_value(cfg, "IntendedVersion"));
    }
    let mc_version = mc_version.ok_or_else(|| {
        keyed_msg!(
            "errors.modpack.mmcMissingGame",
            "MultiMC 整合包缺少 Minecraft 版本\nmmc-pack.json 无 net.minecraft，instance.cfg 无 IntendedVersion"
        )
    })?;

    if let Some(loader) = loader.as_mut() {
        if loader.kind == LoaderKind::Forge
            && matches!(mc_version.as_str(), "1.7.10" | "1.8.8" | "1.8.9")
            && !loader.version.ends_with(&format!("-{mc_version}"))
        {
            // Prism 的 mmc-pack 存裸 Forge 版本；老版本 maven 构件名需要 `-{mc}` 后缀。
            loader.version.push_str(&format!("-{mc_version}"));
        }
    }

    let content_dir = detected.content_dir.as_deref().ok_or_else(|| {
        keyed_msg!(
            "errors.modpack.mmcMissingContent",
            "MultiMC 整合包缺少实例目录\n未找到 minecraft/ 或 .minecraft/"
        )
    })?;

    let overrides = vec![OverridesRef {
        zip_path: zip_path.to_string(),
        prefix: format!("{}{}", detected.base, content_dir),
        skip: Vec::new(),
    }];

    Ok(MmcParsed {
        mc_version,
        loader,
        optifine,
        overrides,
        meta: PackMetaInfo {
            name: instance_cfg
                .and_then(|cfg| instance_value(cfg, "name"))
                .unwrap_or_default(),
            version: String::new(),
            author: String::new(),
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

/// `instance.cfg` 是 Properties/INI 形态：按行 `key=value`，忽略注释与 BOM。
fn instance_value(cfg: &str, key: &str) -> Option<String> {
    for line in cfg.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim().trim_matches('"').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}
