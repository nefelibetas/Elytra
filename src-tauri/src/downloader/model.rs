use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// BMCLAPI 镜像路由表（双表合一）：官方域 → BMCLAPI 镜像前缀。
///
/// 此前为两张同构表：`fetcher.rs` 的 MIRROR_ROUTES（路由层镜像候选生成，
/// 全量 10 条）与 `source.rs` 的 REWRITE_RULES（`DownloadSource::mirror_url`
/// 重写，前 8 条子集）。现合并为本单一表，两个消费方均引用此处；
/// 行序即匹配优先序（各域互不重叠，first-match 与全量收集语义一致）。
/// 相比原 REWRITE_RULES 的差异行逐条注明：
/// - `maven.fabricmc.net`：REWRITE_RULES 原无。Fabric 库构件此前仅经
///   fetcher 路由层生成镜像候选；合并后 `mirror_url` 同步获得该重写
///   （现有调用方均不传 fabric 库 URL，纯超集保全集）。
/// - `meta.fabricmc.net` → `/fabric-meta`：REWRITE_RULES 原无。fabric 版本
///   清单镜像（quilt 无镜像，保持直连官方）；同上，仅路由层原先可用。
pub(crate) const MIRROR_ROUTES: &[(&str, &str)] = &[
    (
        "resources.download.minecraft.net",
        "https://bmclapi2.bangbang93.com/assets",
    ),
    (
        "libraries.minecraft.net",
        "https://bmclapi2.bangbang93.com/libraries",
    ),
    ("piston-meta.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("piston-data.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("launcher.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("launchermeta.mojang.com", "https://bmclapi2.bangbang93.com"),
    // ModLoader 官方 maven 对高并发不友好（实测 20 并发即有连接失败），
    // BMCLAPI 完整镜像 maven.neoforged.net / maven.minecraftforge.net（302 实测可用）。
    // 仅做备用：官方源仍优先，镜像源兜底，避免整批库下载失败。
    (
        "maven.neoforged.net",
        "https://bmclapi2.bangbang93.com/maven",
    ),
    (
        "maven.minecraftforge.net",
        "https://bmclapi2.bangbang93.com/maven",
    ),
    (
        "maven.fabricmc.net",
        "https://bmclapi2.bangbang93.com/maven",
    ),
    // fabric 版本清单镜像（quilt 无镜像，保持直连官方）
    (
        "meta.fabricmc.net",
        "https://bmclapi2.bangbang93.com/fabric-meta",
    ),
];

/// 版本清单
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub latest: LatestVersion,
    pub versions: Vec<VersionMeta>,
}

/// 最新版本
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersion {
    pub release: String,
    pub snapshot: String,
}

/// 版本元数据
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

/// 游戏版本json
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    #[serde(default)]
    pub arguments: Arguments,
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

/// Java版本
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: serde_json::Value,
}

/// 资源索引
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub total_size: u64,
    pub url: String,
}

/// 库
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

/// 库下载
#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

/// Artifact
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

/// 规则
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
        let c = vj.downloads.client.unwrap();
        assert_eq!(c.size, 1);
        assert_eq!(vj.arguments.jvm.as_ref().map(Vec::len), Some(0));
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
