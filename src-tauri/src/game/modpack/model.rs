//! 整合包数据模型与持久化计划结构。

use serde::{Deserialize, Serialize};

use crate::task::LoaderChoice;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PackSource {
    Local {
        path: String,
    },
    Online {
        url: String,
        #[serde(default)]
        sha1: String,
        #[serde(default)]
        size: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackRequest {
    pub name: String,
    pub source: PackSource,
    /// 平台项目图标 URL（本地导入为 None）；安装期下载到 versions/{dir}/pack-icon.png，失败静默跳过。
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFile {
    /// 相对版本目录的目标路径（已过穿越防护，`/` 分隔）
    pub path: String,
    pub url: String,
    /// sha1（空串跳过校验，对齐 fetch_verified 语义）
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

/// overrides 目录引用：zip 内前缀 → 解压到版本目录根。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverridesRef {
    /// 包 zip 的本地缓存路径（prepare 阶段落盘）
    pub zip_path: String,
    /// zip 内成员前缀（`overrides/`、`client-overrides/`、`{base}/minecraft/` 之一）
    pub prefix: String,
    /// 相对前缀的排除前缀（如 `libraries/`；命中即不落盘）
    #[serde(default)]
    pub skip: Vec<String>,
}

/// 清单自带的元数据（展示/日志用；不参与安装流程）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackMetaInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
}

/// 安装计划（prepare 阶段产物，持久化为 JSON；content/base 阶段消费）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackPlan {
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader: Option<LoaderChoice>,
    /// 包声明的 OptiFine 版本（原文，如 `1.20.1_HD_U_I6`；None = 无）
    #[serde(default)]
    pub optifine: Option<String>,
    /// 原版 version json URL（base 阶段 DownloadTask.version_url）
    pub version_url: String,
    /// 预定版本目录名 `{包名}-{mc}`（冲突 `_{n}`），三阶段共享保证一致
    pub version_dir_name: String,
    /// 包身份键（重装/更新同名包时据此复用已有版本目录，sha1 幂等跳过才生效）
    #[serde(default)]
    pub cache_key: String,
    #[serde(default)]
    pub files: Vec<PackFile>,
    /// 作者关闭第三方分发的 CF 文件名（无法自动下载，收尾提示手动放置）
    #[serde(default)]
    pub restricted: Vec<String>,
    /// 覆盖目录（按数组顺序解压，后者覆盖前者；空 = 无覆盖内容）
    #[serde(default, deserialize_with = "de_overrides")]
    pub overrides: Vec<OverridesRef>,
    /// 嗅探到的格式（旧快照无此字段）
    #[serde(default)]
    pub kind: Option<PackKind>,
    /// 清单元数据（旧快照无此字段）
    #[serde(default)]
    pub meta: Option<PackMetaInfo>,
}

/// 兼容旧快照：`overrides` 曾是单个对象，现为数组。
fn de_overrides<'de, D>(deserializer: D) -> Result<Vec<OverridesRef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(OverridesRef),
        Many(Vec<OverridesRef>),
    }
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(ov)) => vec![ov],
        Some(OneOrMany::Many(list)) => list,
    })
}

/// 嗅探到的整合包格式（与扩展名无关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackKind {
    Modrinth,
    CurseForge,
    MultiMc,
    /// PCL 带启动器包（内嵌 `modpack.zip` / `modpack.mrpack`）
    PclLauncher,
    /// 完整游戏目录（唯一 `versions/<id>/<id>.json`）
    Directory,
    /// HMCL 原生（`modpack.json` + `minecraft/pack.json`）
    Hmcl,
    /// MCBBS（`mcbbs.packmeta` 或含 `addons` 的 `manifest.json`）
    Mcbbs,
}
