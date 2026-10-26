//! CurseForge 整合包导出：`mods/` 走指纹匹配写 `files[]`，未匹配文件物理打包进 `overrides/`。
//!
//! 指纹算法：CurseForge 的 MurmurHash2（seed 1）变体，哈希前剔除 ASCII 空白（9/10/13/32）。
//! 已用 CurseForge 指纹接口对真实 jar 对拍验证。

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::game::community::curseforge_fingerprint_matches;
use crate::launcher::args::find_version_json;
use crate::task::{LoaderChoice, LoaderKind};
use crate::util::{loader_from_dir, mc_version_from_dir};

use super::export::add_dir_to_zip;
use super::version_meta::loader_from_version_json;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CurseForgeExportStats {
    /// 指纹命中、以 `files[]` 引用的本地文件数
    pub matched: usize,
    /// 未命中、物理打进 `overrides/` 的文件数
    pub bundled: usize,
}

/// CurseForge 指纹：MurmurHash2（seed 1），哈希前剔除 ASCII 空白。
pub(crate) fn cf_fingerprint(bytes: &[u8]) -> u32 {
    let normalized: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, 9 | 10 | 13 | 32))
        .collect();
    murmur2(&normalized, 1)
}

fn murmur2(data: &[u8], seed: u32) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let mut hash = seed ^ (data.len() as u32);
    let (chunks, tail) = data.as_chunks::<4>();
    for chunk in chunks {
        let mut key = u32::from_le_bytes(*chunk);
        key = key.wrapping_mul(M);
        key ^= key >> 24;
        key = key.wrapping_mul(M);
        hash = hash.wrapping_mul(M);
        hash ^= key;
    }
    match tail.len() {
        3 => {
            hash ^= (tail[2] as u32) << 16;
            hash ^= (tail[1] as u32) << 8;
            hash ^= tail[0] as u32;
            hash = hash.wrapping_mul(M);
        }
        2 => {
            hash ^= (tail[1] as u32) << 8;
            hash ^= tail[0] as u32;
            hash = hash.wrapping_mul(M);
        }
        1 => {
            hash ^= tail[0] as u32;
            hash = hash.wrapping_mul(M);
        }
        _ => {}
    }
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(M);
    hash ^= hash >> 15;
    hash
}

/// 构建 `manifest.json`（规范字段：manifestType / manifestVersion / files / overrides / minecraft）。
pub(crate) fn build_manifest(
    version_name: &str,
    mc_version: &str,
    loader: Option<&LoaderChoice>,
    files: &[Value],
) -> Value {
    let mod_loaders = loader
        .map(|loader| json!([{ "id": loader_id(loader), "primary": true }]))
        .unwrap_or_else(|| json!([]));
    json!({
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": version_name,
        "version": version_name,
        "author": "Elytra",
        "files": files,
        "overrides": "overrides",
        "minecraft": {
            "version": mc_version,
            "modLoaders": mod_loaders,
        },
    })
}

fn loader_id(loader: &LoaderChoice) -> String {
    let kind = match loader.kind {
        LoaderKind::Fabric => "fabric",
        LoaderKind::Quilt => "quilt",
        LoaderKind::Forge => "forge",
        LoaderKind::NeoForge => "neoforge",
    };
    format!("{kind}-{}", loader.version)
}

/// 导出 CurseForge zip：指纹命中写引用，未命中与其它内容目录物理打包。
pub(crate) async fn pack_curseforge(
    vdir: &Path,
    version_name: &str,
    save_path: &str,
) -> Result<CurseForgeExportStats, String> {
    let (mc_version, loader) = resolve_meta(vdir, version_name);

    let mut by_fingerprint: HashMap<u32, Vec<String>> = HashMap::new();
    for path in mod_candidates(&vdir.join("mods")) {
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        by_fingerprint
            .entry(cf_fingerprint(&data))
            .or_default()
            .push(name);
    }

    let fingerprints: Vec<u32> = by_fingerprint.keys().copied().collect();
    let matches = curseforge_fingerprint_matches(&fingerprints).await?;

    let mut matched_names = HashSet::new();
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for matched in &matches {
        if seen.insert((matched.project_id, matched.file_id)) {
            files.push(json!({
                "projectID": matched.project_id,
                "fileID": matched.file_id,
                "required": true,
            }));
        }
        if let Some(names) = by_fingerprint.get(&matched.fingerprint) {
            matched_names.extend(names.iter().cloned());
        }
    }
    let bundled = by_fingerprint
        .values()
        .flatten()
        .filter(|name| !matched_names.contains(*name))
        .count();

    let file = std::fs::File::create(save_path)
        .map_err(common_err_ctx_key!("errors.file.write", "创建文件失败"))?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let manifest = build_manifest(version_name, &mc_version, loader.as_ref(), &files);
    let content = serde_json::to_vec_pretty(&manifest).map_err(common_err_ctx_key!(
        "errors.modpack.serializeFailed",
        "序列化失败"
    ))?;
    writer
        .start_file("manifest.json", opts)
        .map_err(common_err_ctx_key!("errors.file.write", "写入清单失败"))?;
    writer
        .write_all(&content)
        .map_err(common_err_ctx_key!("errors.file.write", "写入清单失败"))?;

    for sub in [
        "mods",
        "config",
        "shaderpacks",
        "resourcepacks",
        "datapacks",
    ] {
        let dir = vdir.join(sub);
        if !dir.is_dir() {
            continue;
        }
        let skip = (sub == "mods").then_some(&matched_names);
        add_dir_to_zip(&mut writer, &dir, &format!("overrides/{sub}"), opts, skip)?;
    }
    writer
        .finish()
        .map_err(common_err_ctx_key!("errors.file.write", "写入 zip 失败"))?;

    Ok(CurseForgeExportStats {
        matched: matched_names.len(),
        bundled,
    })
}

/// 版本元数据：版本 JSON 优先（libraries 提取 loader），目录名兜底。
fn resolve_meta(vdir: &Path, version_name: &str) -> (String, Option<LoaderChoice>) {
    let text =
        find_version_json(vdir, version_name).and_then(|path| std::fs::read_to_string(path).ok());
    let Some(text) = text else {
        return (
            mc_version_from_dir(version_name).to_string(),
            loader_from_dir_choice(version_name),
        );
    };
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let mc_version = value["inheritsFrom"]
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            value["id"]
                .as_str()
                .map(|id| mc_version_from_dir(id).to_string())
        })
        .unwrap_or_else(|| mc_version_from_dir(version_name).to_string());
    (mc_version, loader_from_version_json(&text))
}

/// 版本 JSON 缺失时按目录名解析 loader（`1.20.1-forge-47.2.0`）。
fn loader_from_dir_choice(version_name: &str) -> Option<LoaderChoice> {
    let (kind, version) = loader_from_dir(version_name)?;
    let kind = match kind {
        "forge" => LoaderKind::Forge,
        "fabric" => LoaderKind::Fabric,
        "quilt" => LoaderKind::Quilt,
        "neoforge" => LoaderKind::NeoForge,
        _ => return None,
    };
    Some(LoaderChoice {
        kind,
        version: version.to_string(),
    })
}

/// 与 MRPACK 导出保持一致的 mod 候选集合。
fn mod_candidates(mods_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && matches!(
                    path.extension().and_then(|ext| ext.to_str()),
                    Some("jar" | "zip" | "rar" | "disabled" | "old")
                )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::community::parse_fingerprint_matches;
    use std::io::Read as _;

    #[test]
    fn fingerprint_ignores_ascii_whitespace() {
        assert_eq!(cf_fingerprint(b"a b\nc\rd\te"), cf_fingerprint(b"abcde"));
    }

    #[test]
    fn murmur2_known_vectors() {
        assert_eq!(murmur2(b"", 1), 1_540_447_798);
        assert_eq!(murmur2(b"hello", 1), 2_788_266_382);
        assert_eq!(murmur2(b"abc", 1), 1_621_425_345);
    }

    #[test]
    fn manifest_matches_curseforge_spec() {
        let loader = LoaderChoice {
            kind: LoaderKind::Forge,
            version: "47.2.0".into(),
        };
        let files = vec![json!({"projectID": 1, "fileID": 2, "required": true})];
        let manifest = build_manifest("Demo", "1.20.1", Some(&loader), &files);
        assert_eq!(manifest["manifestType"], "minecraftModpack");
        assert_eq!(manifest["manifestVersion"], 1);
        assert_eq!(manifest["overrides"], "overrides");
        assert_eq!(manifest["minecraft"]["version"], "1.20.1");
        assert_eq!(manifest["minecraft"]["modLoaders"][0]["id"], "forge-47.2.0");
        assert_eq!(manifest["minecraft"]["modLoaders"][0]["primary"], true);
        assert_eq!(manifest["files"][0]["required"], true);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要 ELYTRA_SMOKE_CF_JAR 与 CurseForge Key"]
    async fn curseforge_export_smoke() {
        let Ok(jar) = std::env::var("ELYTRA_SMOKE_CF_JAR") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let version_name = "1.12.2-forge-14.23.5.2860";
        let vdir = dir.path().join(version_name);
        std::fs::create_dir_all(vdir.join("mods")).unwrap();
        std::fs::create_dir_all(vdir.join("config")).unwrap();
        std::fs::copy(&jar, vdir.join("mods").join("jei.jar")).unwrap();
        std::fs::write(vdir.join("config").join("a.txt"), b"a").unwrap();
        std::fs::write(
            vdir.join(format!("{version_name}.json")),
            r#"{"id":"1.12.2-forge-14.23.5.2860","inheritsFrom":"1.12.2","libraries":[{"name":"net.minecraftforge:forge:1.12.2-14.23.5.2860"}]}"#,
        )
        .unwrap();

        let out = dir.path().join("cf.zip");
        let stats = pack_curseforge(&vdir, version_name, out.to_str().unwrap())
            .await
            .expect("CF 导出应成功");
        assert!(stats.matched >= 1);
        assert_eq!(stats.bundled, 0);

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut manifest_text = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest_text)
            .unwrap();
        let manifest: Value = serde_json::from_str(&manifest_text).unwrap();
        assert_eq!(manifest["manifestVersion"], 1);
        assert_eq!(manifest["minecraft"]["version"], "1.12.2");
        assert_eq!(
            manifest["minecraft"]["modLoaders"][0]["id"],
            "forge-14.23.5.2860"
        );
        assert!(!manifest["files"].as_array().unwrap().is_empty());

        let names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        assert!(
            !names.iter().any(|name| name == "overrides/mods/jei.jar"),
            "匹配的 mod 不应物理打包"
        );
        assert!(names.iter().any(|name| name == "overrides/config/a.txt"));
    }

    #[test]
    fn parses_exact_fingerprint_matches() {
        let json = json!({
            "data": {
                "exactMatches": [{
                    "id": 238222,
                    "file": {
                        "id": 3040523,
                        "modId": 238222,
                        "fileFingerprint": 3089143260u32,
                    }
                }],
                "unmatchedFingerprints": [1]
            }
        });
        let matches = parse_fingerprint_matches(&json);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].project_id, 238222);
        assert_eq!(matches[0].file_id, 3040523);
        assert_eq!(matches[0].fingerprint, 3089143260);
    }
}
