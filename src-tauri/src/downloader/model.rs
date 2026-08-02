use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
    pub natives: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<ExtractRules>,
}

/// 库下载
#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<std::collections::HashMap<String, Artifact>>,
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

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub phase: String,
    pub file: String,
    pub current: u64,
    pub total: u64,
    pub speed: u64,
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
