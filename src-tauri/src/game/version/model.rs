use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub latest: LatestVersion,
    pub versions: Vec<VersionMeta>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersion {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub id: String,
    pub r#type: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    pub sha1: String,
    pub compliance_level: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    /// 游戏参数：1.13+ 用对象形态；**老版本（≤1.12）无该字段，部分合并/第三方写入 null**，
    /// 故按 Option 解析（`#[serde(default)]` 只兜住缺失，兜不住显式 null）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    pub asset_index: AssetIndex,
    pub assets: serde_json::Value,
    pub compliance_level: serde_json::Value,
    pub downloads: Downloads,
    pub id: String,
    pub java_version: JavaVersion,
    pub libraries: Vec<Library>,
    pub main_class: String,
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    pub minimum_launcher_version: serde_json::Value,
    pub release_time: String,
    pub time: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: u64,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<ExtractRules>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

/// 解压规则（旧式 native 库用）
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractRules {
    pub exclude: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Downloads {
    pub client: Option<ClientDownload>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDownload {
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Arguments {
    pub game: Option<Vec<serde_json::Value>>,
    pub jvm: Option<Vec<serde_json::Value>>,
}

impl VersionJson {
    /// `arguments.jvm`（新式；老版本无 → None）。
    pub fn jvm_args(&self) -> Option<&Vec<serde_json::Value>> {
        self.arguments.as_ref().and_then(|a| a.jvm.as_ref())
    }

    /// `arguments.game`（新式；老版本无 → None，由 `minecraftArguments` 兜底）。
    pub fn game_args(&self) -> Option<&Vec<serde_json::Value>> {
        self.arguments.as_ref().and_then(|a| a.game.as_ref())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssetJson {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    /// 老版本（≤1.12）json 无 arguments 字段、第三方可能写 `"arguments": null`，二者都必须能解析。
    #[test]
    fn legacy_arguments_null_or_missing_both_parse() {
        let base = |args_field: &str| {
            format!(
                r#"{{ {args_field} "assetIndex": {{ "id": "legacy", "sha1": "x", "size": 1, "totalSize": 2, "url": "u" }},
                   "assets": "legacy", "complianceLevel": 1, "downloads": {{}}, "id": "1.7.10",
                   "javaVersion": {{ "component": "jre-legacy", "majorVersion": 8 }},
                   "libraries": [], "mainClass": "net.minecraft.launchwrapper.Launch",
                   "minecraftArguments": "--username ${{auth_player_name}}", "minimumLauncherVersion": 4,
                   "releaseTime": "2014-05-14T00:00:00+00:00", "time": "2014-05-14T00:00:00+00:00", "type": "release" }}"#
            )
        };
        // 字段缺失（原始原版 json 形态）
        let vj: VersionJson = serde_json::from_str(&base("")).expect("缺 arguments 字段应能解析");
        assert!(vj.arguments.is_none());
        assert!(vj.jvm_args().is_none() && vj.game_args().is_none());
        assert!(vj.minecraft_arguments.is_some());
        // 显式 null（第三方启动器写入）
        let vj2: VersionJson =
            serde_json::from_str(&base(r#""arguments": null,"#)).expect("arguments: null 应能解析");
        assert!(vj2.arguments.is_none());
        // 新式对象形态仍照常
        let vj3: VersionJson =
            serde_json::from_str(&base(r#""arguments": { "game": ["--demo"] },"#))
                .expect("新式 arguments 应能解析");
        assert_eq!(vj3.game_args().map(Vec::len), Some(1));
    }

    use super::*;

    /// 最小可用版本 JSON（覆盖全部必填字段）。
    fn minimal_json() -> &'static str {
        r#"{
            "id": "1.18.2",
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "1.18",
            "assetIndex": { "id": "1.18", "url": "x", "sha1": "y", "size": 1, "totalSize": 1 },
            "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
            "arguments": { "game": [], "jvm": [] },
            "libraries": [],
            "complianceLevel": 0,
            "minimumLauncherVersion": 21,
            "javaVersion": { "majorVersion": 17 },
            "releaseTime": "2021-11-30T00:00:00Z",
            "time": "2021-11-30T00:00:00Z",
            "type": "release"
        }"#
    }

    #[test]
    fn test_version_json_parse_minimal() {
        let vj: VersionJson = serde_json::from_str(minimal_json()).unwrap();
        assert_eq!(vj.id, "1.18.2");
        assert_eq!(vj.main_class, "net.minecraft.client.main.Main");
        assert!(vj.libraries.is_empty());
        assert_eq!(vj.jvm_args().map(Vec::len), Some(0));
        let c = vj.downloads.client.unwrap();
        assert_eq!(c.size, 1);
    }

    #[test]
    fn test_version_json_parse_loader() {
        // ModLoader 版本：带 libraries + minecraftArguments + 未知字段（须容忍）
        let json = r#"{
            "id": "1.20.1-forge-47.1.0",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "assets": "1.20",
            "assetIndex": { "id": "1.20", "url": "x", "sha1": "y", "size": 1, "totalSize": 1 },
            "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
            "arguments": { "game": [], "jvm": [] },
            "minecraftArguments": "--username ${auth_player_name}",
            "libraries": [{
                "name": "net.minecraftforge:forge:47.1.0",
                "downloads": { "artifact": { "path": "x.jar", "url": "u", "sha1": "s", "size": 1 } }
            }],
            "complianceLevel": 0,
            "minimumLauncherVersion": 21,
            "javaVersion": { "majorVersion": 17 },
            "releaseTime": "2023-07-01T00:00:00Z",
            "time": "2023-07-01T00:00:00Z",
            "type": "release",
            "inheritsFrom": "1.20.1",
            "unknownFutureField": { "a": 1 }
        }"#;
        let vj: VersionJson = serde_json::from_str(json).unwrap();
        assert_eq!(vj.id, "1.20.1-forge-47.1.0");
        assert_eq!(vj.libraries.len(), 1);
        assert_eq!(vj.libraries[0].name, "net.minecraftforge:forge:47.1.0");
        assert!(vj.minecraft_arguments.is_some());
    }
}
