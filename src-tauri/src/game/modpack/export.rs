//! 整合包导出（标准 MRPACK）与在线分发文件识别。

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::downloader::hash::{sha1_hex, sha512_hex};
use crate::game::community::modrinth_version_files;
use crate::launcher::args::find_version_json;
use crate::util::{infer_loader_from_json, loader_from_dir, mc_version_from_dir};

// ── 导出侧（与上方导入解析对称；命令层 export_modpack 只做壳）──

#[derive(serde::Serialize)]
pub(crate) struct HostedPackFile {
    path: String,
    hashes: serde_json::Value,
    downloads: Vec<String>,
    #[serde(rename = "fileSize")]
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
    let dir_mc = mc_version_from_dir(version_name);
    let dir_loader = loader_from_dir(version_name);
    let Some(text) =
        find_version_json(vdir, version_name).and_then(|p| std::fs::read_to_string(p).ok())
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
                .map(|s| mc_version_from_dir(s).to_string())
        })
        .unwrap_or_else(|| dir_mc.to_string());
    let json_kind = infer_loader_from_json(&text);
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

    let file = std::fs::File::create(save_path)
        .map_err(common_err_ctx_key!("errors.file.write", "创建文件失败"))?;
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
        "versionId": version_name,
        "name": version_name,
        "summary": "",
        "files": hosted,
        "dependencies": dependencies,
    });
    let content = serde_json::to_vec_pretty(&manifest).map_err(common_err_ctx_key!(
        "errors.modpack.serializeFailed",
        "序列化失败"
    ))?;
    writer
        .start_file("modrinth.index.json", opts)
        .map_err(common_err_ctx_key!("errors.file.write", "写入清单失败"))?;
    writer
        .write_all(&content)
        .map_err(common_err_ctx_key!("errors.file.write", "写入清单失败"))?;

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
    writer
        .finish()
        .map_err(common_err_ctx_key!("errors.file.write", "写入 zip 失败"))?;
    Ok(())
}

/// 递归写目录到 zip（保留相对路径；目录条目本身也记录，空目录才可保留）。
pub(super) fn add_dir_to_zip(
    writer: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
    skip_files: Option<&std::collections::HashSet<String>>,
) -> Result<(), String> {
    writer
        .add_directory(format!("{prefix}/"), opts)
        .map_err(common_err_ctx_key!("errors.file.write", "写入目录失败"))?;
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
            let mut f = std::fs::File::open(&path)
                .map_err(common_err_ctx_key!("errors.file.read", "打开文件失败"))?;
            writer
                .start_file(name, opts)
                .map_err(common_err_ctx_key!("errors.file.write", "写入文件失败"))?;
            std::io::copy(&mut f, writer)
                .map_err(common_err_ctx_key!("errors.file.write", "写入文件失败"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::HostedPackFile;

    #[test]
    fn hosted_pack_file_serializes_spec_file_size() {
        let f = HostedPackFile {
            path: "mods/a.jar".into(),
            hashes: serde_json::json!({"sha1": "a", "sha512": "b"}),
            downloads: vec!["https://cdn.modrinth.com/a.jar".into()],
            file_size: 123,
        };
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v["fileSize"], 123);
        assert!(v.get("file_size").is_none(), "MRPACK 字段必须是 fileSize");
    }
}
