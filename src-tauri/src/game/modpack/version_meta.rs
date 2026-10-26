//! 从版本 JSON 提取 MC/loader 元数据（HMCL `pack.json` 与完整目录包共用）。
//!
//! 兼容官方 version json 与 HMCL `GameInstanceManifest`（`patches[]` 内也可能声明 `libraries`/`arguments`）。

use serde_json::Value;

use crate::task::{LoaderChoice, LoaderKind};

/// 解析 libraries 坐标与 FML 参数，按 fabric → quilt → neoforge → forge 顺序取第一个已知 loader。
pub(crate) fn loader_from_version_json(text: &str) -> Option<LoaderChoice> {
    let value: Value = serde_json::from_str(text).ok()?;
    let mut libraries = Vec::new();
    let mut fml_neo_forge = None;
    let mut fml_forge = None;
    collect(&value, &mut libraries, &mut fml_neo_forge, &mut fml_forge);

    // Quilt 的清单同时含 fabric 系库，必须先判 quilt-loader
    for (group, artifact, version) in &libraries {
        if group == "org.quiltmc" && artifact == "quilt-loader" {
            return choice(LoaderKind::Quilt, version);
        }
    }
    for (group, artifact, version) in &libraries {
        if group == "net.fabricmc" && artifact == "fabric-loader" {
            return choice(LoaderKind::Fabric, version);
        }
    }
    for (group, artifact, version) in &libraries {
        if group == "net.neoforged" && artifact == "neoforge" {
            return choice(LoaderKind::NeoForge, version);
        }
    }
    for (group, artifact, version) in &libraries {
        if group == "net.neoforged" && artifact == "forge" {
            return choice(LoaderKind::NeoForge, &strip_mc_prefix(version));
        }
    }
    if let Some(version) = fml_neo_forge {
        return choice(LoaderKind::NeoForge, &version);
    }
    for (group, artifact, version) in &libraries {
        if group == "net.minecraftforge"
            && matches!(artifact.as_str(), "forge" | "fmlloader" | "minecraftforge")
        {
            return choice(LoaderKind::Forge, &strip_mc_prefix(version));
        }
    }
    fml_forge.and_then(|version| choice(LoaderKind::Forge, &version))
}

/// OptiFine 版本串 → `(of_type, patch)`：`1.20.1_HD_U_I6` / `HD_U_I6` / `I6`（无 MC 前缀时按 `HD_U` 处理）。
pub(crate) fn parse_optifine_version(mc_version: &str, raw: &str) -> (String, String) {
    let mut rest = raw.trim();
    if let Some(stripped) = rest.strip_prefix(mc_version) {
        rest = stripped.trim_start_matches('_');
    }
    match rest.rsplit_once('_') {
        Some((of_type, patch)) if !of_type.is_empty() && !patch.is_empty() => {
            (of_type.to_string(), patch.to_string())
        }
        _ => ("HD_U".to_string(), rest.to_string()),
    }
}

fn choice(kind: LoaderKind, version: &str) -> Option<LoaderChoice> {
    let version = version.trim();
    (!version.is_empty()).then(|| LoaderChoice {
        kind,
        version: version.to_string(),
    })
}

/// `1.20.1-47.2.0` → `47.2.0`；无 MC 前缀时原样返回。
fn strip_mc_prefix(version: &str) -> String {
    version
        .split_once('-')
        .map(|(_, rest)| rest)
        .unwrap_or(version)
        .to_string()
}

fn collect(
    value: &Value,
    libraries: &mut Vec<(String, String, String)>,
    fml_neo_forge: &mut Option<String>,
    fml_forge: &mut Option<String>,
) {
    match value {
        Value::Object(map) => {
            if let Some(list) = map.get("libraries").and_then(Value::as_array) {
                for library in list {
                    let Some(name) = library.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(coord) = parse_coordinate(name) {
                        libraries.push(coord);
                    }
                }
            }
            if let Some(args) = map
                .get("arguments")
                .and_then(|arguments| arguments.get("game"))
                .and_then(Value::as_array)
            {
                collect_fml_args(args, fml_neo_forge, fml_forge);
            }
            for child in map.values() {
                collect(child, libraries, fml_neo_forge, fml_forge);
            }
        }
        Value::Array(list) => {
            for child in list {
                collect(child, libraries, fml_neo_forge, fml_forge);
            }
        }
        _ => {}
    }
}

fn collect_fml_args(
    args: &[Value],
    fml_neo_forge: &mut Option<String>,
    fml_forge: &mut Option<String>,
) {
    let mut index = 0;
    while index + 1 < args.len() {
        let key = args[index].as_str();
        let value = args[index + 1].as_str().map(str::trim);
        match (key, value) {
            (Some("--fml.neoForgeVersion"), Some(version)) if !version.is_empty() => {
                *fml_neo_forge = Some(version.to_string());
            }
            (Some("--fml.forgeVersion"), Some(version)) if !version.is_empty() => {
                *fml_forge = Some(version.to_string());
            }
            _ => {}
        }
        index += 1;
    }
}

fn parse_coordinate(name: &str) -> Option<(String, String, String)> {
    let mut parts = name.split(':');
    let group = parts.next()?.trim();
    let artifact = parts.next()?.trim();
    let version = parts.next()?.trim();
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    Some((group.to_string(), artifact.to_string(), version.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: &str) -> Option<LoaderChoice> {
        loader_from_version_json(json)
    }

    #[test]
    fn extracts_fabric_and_quilt() {
        let fabric = r#"{"libraries":[{"name":"net.fabricmc:fabric-loader:0.15.11"}]}"#;
        let choice = loader(fabric).unwrap();
        assert_eq!(choice.kind, LoaderKind::Fabric);
        assert_eq!(choice.version, "0.15.11");

        let quilt = r#"{"libraries":[{"name":"org.quiltmc:quilt-loader:0.23.1"}]}"#;
        assert_eq!(loader(quilt).unwrap().kind, LoaderKind::Quilt);
    }

    #[test]
    fn strips_mc_prefix_for_forge_and_neoforge() {
        let forge = r#"{"libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.2.0"}]}"#;
        let choice = loader(forge).unwrap();
        assert_eq!(choice.kind, LoaderKind::Forge);
        assert_eq!(choice.version, "47.2.0");

        let neo = r#"{"libraries":[{"name":"net.neoforged:neoforge:21.1.95"}]}"#;
        let choice = loader(neo).unwrap();
        assert_eq!(choice.kind, LoaderKind::NeoForge);
        assert_eq!(choice.version, "21.1.95");
    }

    #[test]
    fn reads_fml_arguments_from_nested_patches() {
        let json = r#"{"patches":[{"arguments":{"game":["--fml.neoForgeVersion","20.4.237"]}}]}"#;
        let choice = loader(json).unwrap();
        assert_eq!(choice.kind, LoaderKind::NeoForge);
        assert_eq!(choice.version, "20.4.237");
    }

    #[test]
    fn quilt_wins_over_fabric_bridge() {
        let json = r#"{"libraries":[
            {"name":"net.fabricmc:fabric-loader:0.15.11"},
            {"name":"org.quiltmc:quilt-loader:0.23.1"}
        ]}"#;
        let choice = loader(json).unwrap();
        assert_eq!(choice.kind, LoaderKind::Quilt);
        assert_eq!(choice.version, "0.23.1");
    }

    #[test]
    fn parses_optifine_versions() {
        assert_eq!(
            parse_optifine_version("1.20.1", "1.20.1_HD_U_I6"),
            ("HD_U".into(), "I6".into())
        );
        assert_eq!(
            parse_optifine_version("1.20.1", "HD_U_I6"),
            ("HD_U".into(), "I6".into())
        );
        assert_eq!(
            parse_optifine_version("1.20.1", "I6"),
            ("HD_U".into(), "I6".into())
        );
    }

    #[test]
    fn vanilla_has_no_loader() {
        assert!(loader(r#"{"libraries":[{"name":"com.mojang:brigadier:1.0.18"}]}"#).is_none());
    }
}
