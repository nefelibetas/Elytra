//! Modrinth MRPACK（`modrinth.index.json`）解析。

use crate::task::{LoaderChoice, LoaderKind};
use crate::util::safe_relative_path;

use super::model::{PackFile, PackMetaInfo};

#[derive(Debug)]
pub struct ModrinthParsed {
    pub mc_version: String,
    pub loader: Option<LoaderChoice>,
    pub files: Vec<PackFile>,
    pub meta: PackMetaInfo,
}

/// 解析 modrinth.index.json（规范：docs.modrinth.com/#spec/modrinth-index-json）；剔除 env.client == "unsupported"。
pub fn parse_modrinth_index(text: &str) -> Result<ModrinthParsed, String> {
    let idx: serde_json::Value = serde_json::from_str(text).map_err(common_err_ctx_key!(
        "errors.install.modpack",
        "解析 modrinth.index.json 失败"
    ))?;
    if idx["formatVersion"].as_i64() != Some(1) {
        bail_key!(
            "errors.modpack.unsupportedFormatVersion",
            "不支持的整合包格式版本\nformatVersion: {}",
            idx["formatVersion"]
        );
    }
    if idx["game"].as_str() != Some("minecraft") {
        bail_key!(
            "errors.modpack.notMinecraft",
            "不是 Minecraft 整合包\ngame: {}",
            idx["game"].as_str().unwrap_or("(缺失)")
        );
    }
    let mc_version = idx["dependencies"]["minecraft"]
        .as_str()
        .ok_or_else(|| {
            keyed_msg!(
                "errors.modpack.missingMinecraft",
                "清单缺少 Minecraft 版本（dependencies.minecraft）"
            )
        })?
        .to_string();
    let loader = modrinth_loader(&idx["dependencies"])?;

    let mut files = Vec::new();
    for f in idx["files"]
        .as_array()
        .ok_or_else(|| keyed_msg!("errors.modpack.missingFiles", "清单缺少 files 数组"))?
    {
        // 服务端专属 mod 不进客户端实例；env 缺失视为需要
        if f["env"]["client"].as_str() == Some("unsupported") {
            continue;
        }
        let path = f["path"]
            .as_str()
            .ok_or_else(|| keyed_msg!("errors.modpack.fileMissingPath", "文件条目缺少 path"))?
            .to_string();
        safe_relative_path(&path)?;
        let url = f["downloads"]
            .as_array()
            .and_then(|d| d.first())
            .and_then(|u| u.as_str())
            .ok_or_else(|| keyed_msg!("errors.modpack.fileMissingUrl", "文件缺少下载地址\n{path}"))?
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
        meta: PackMetaInfo {
            name: idx["name"].as_str().unwrap_or_default().to_string(),
            version: idx["versionId"].as_str().unwrap_or_default().to_string(),
            author: String::new(),
        },
    })
}

/// dependencies → LoaderChoice：按固定顺序取第一个已知键；未知键（optifine 等）忽略，不阻断导入。
fn modrinth_loader(deps: &serde_json::Value) -> Result<Option<LoaderChoice>, String> {
    if !deps.is_object() {
        return Err(keyed_msg!(
            "errors.modpack.missingDependencies",
            "清单缺少 dependencies 对象"
        ));
    }
    const ORDER: [(&str, LoaderKind); 5] = [
        ("fabric-loader", LoaderKind::Fabric),
        ("quilt-loader", LoaderKind::Quilt),
        ("quilt_loader", LoaderKind::Quilt),
        ("forge", LoaderKind::Forge),
        ("neoforge", LoaderKind::NeoForge),
    ];
    for (key, kind) in ORDER {
        let version = deps
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(version) = version {
            return Ok(Some(LoaderChoice {
                kind,
                version: version.to_string(),
            }));
        }
    }
    Ok(None)
}
