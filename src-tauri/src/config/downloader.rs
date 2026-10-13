use crate::downloader::endpoints;
use crate::util;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "config/")]
pub struct DownloadSource {
    pub name: String,

    pub manifest_url: String,
    pub resource_url: String,
    pub maven_url: String,
    pub libraries_url: String,

    pub forge_list_url: String,
    pub fabric_list_url: String,
    pub neoforge_list_url: String,
    pub quilt_list_url: String,
    pub optifine_list_url: String,

    pub forge_installer_url: String,
    pub neoforge_installer_url: String,
    pub fabric_loader_url: String,
    pub fabric_profile_url: String,
    pub quilt_profile_url: String,
    pub optifine_download_url: String,
}

impl DownloadSource {
    pub fn url(&self, template: &str, args: &[(&str, &str)]) -> String {
        let mut s = template.to_string();
        for (k, v) in args {
            s = s.replace(&format!("{{{}}}", k), v);
        }
        s
    }
}

/// 资源类（按 URL host 自动推断，供下载路由选择镜像候选/排序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    /// 客户端 jar / assets 索引 / version json（piston-meta/piston-data/launcher 域）
    Game,
    /// assets 对象文件（resources.download.minecraft.net）
    Asset,
    /// 支持库（libraries.minecraft.net）
    Library,
    /// ModLoader 构件与清单（maven.minecraftforge.net / maven.neoforged.net / maven.fabricmc.net / meta.fabricmc.net）
    Loader,
    /// Java runtime（piston-meta 且路径含 java-runtime）
    Java,
    /// Modrinth / CurseForge 社区资源（cdn.modrinth.com / forgecdn / MCIM 镜像）
    Community,
    /// 未识别
    Other,
}

/// 按 URL host 推断资源类（BMCLAPI/MCIM 镜像 URL 按映射前域名推断）；未知 → Other（不生成镜像候选）。
pub fn infer_resource_class(url: &str) -> ResourceClass {
    if url.contains("java-runtime") && url.contains("piston") {
        return ResourceClass::Java;
    }
    if url.contains("resources.download.minecraft.net")
        || (url.contains("bmclapi") && url.contains("/assets/"))
    {
        return ResourceClass::Asset;
    }
    if url.contains("libraries.minecraft.net") {
        return ResourceClass::Library;
    }
    if url.contains("piston-meta")
        || url.contains("piston-data")
        || url.contains("launcher.mojang.com")
        || url.contains("launchermeta.mojang.com")
    {
        return ResourceClass::Game;
    }
    if url.contains("minecraftforge.net")
        || url.contains("neoforged.net")
        || url.contains("fabricmc.net")
    {
        return ResourceClass::Loader;
    }
    if url.contains("modrinth.com")
        || url.contains("forgecdn.net")
        || url.contains("curseforge.com")
        || url.contains("mcimirror.top")
    {
        return ResourceClass::Community;
    }
    ResourceClass::Other
}

/// - auto（默认）：自动检测系统代理（Windows 注册表 / Linux 环境变量或 gsettings），
///   检测到即走代理，无代理直连（TUN 模式由网络层接管）
/// - system：强制系统代理（检测不到回退直连并告警）；direct：强制直连（TUN 仍由网络层生效）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ts_rs::TS)]
#[ts(export_to = "config/")]
#[serde(rename_all = "camelCase")]
pub enum NetworkMode {
    #[default]
    Auto,
    System,
    Direct,
}

/// 下载源选择模式：
/// - auto（默认）：按 URL 自动分类，镜像候选即时生成并按健康度排序；元数据/清单请求官方优先、
///   超时（4s）自动降级镜像
/// - officialOnly / mirrorOnly：单选，只用一个源，不自动回退
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, ts_rs::TS)]
#[ts(export_to = "config/")]
#[serde(rename_all = "camelCase")]
pub enum DownloadSourceMode {
    /// 仅官方源（版本清单/文件/社区 API 全走官方）
    OfficialOnly,
    /// 仅镜像源（BMCLAPI / MCIM / TUNA 等国内镜像）
    MirrorOnly,
    /// 自适应：程序自动判断更快源（健康度路由 + 过慢降级）
    #[default]
    Auto,
}

/// 自定义反序列化，供 DownloadSourceMode / JdkSourceMode 复用（消除两个同构 impl）。
macro_rules! deserialize_mode {
    ($t:ty) => {
        impl<'de> serde::Deserialize<'de> for $t {
            fn deserialize<D>(d: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(d)?;
                Ok(match s.as_str() {
                    "officialOnly" => Self::OfficialOnly,
                    "mirrorOnly" => Self::MirrorOnly,
                    _ => Self::Auto,
                })
            }
        }
    };
}

deserialize_mode!(DownloadSourceMode);

/// 缺省源 = 代码内置模板（官方 + 镜像，单一事实源见 `downloader/endpoints.rs`）。
fn default_sources() -> Vec<DownloadSource> {
    vec![endpoints::official_source(), endpoints::mirror_source()]
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "config/")]
pub struct DownloaderConfig {
    /// 文件源模板（官方 + 镜像）。代码内置、单一事实源见 `downloader/endpoints.rs`；
    /// **不持久化**：写时跳过，读时忽略旧文件 URL 快照（防旧地址覆盖新版本内置拓扑）。
    #[serde(default = "default_sources", skip_serializing, skip_deserializing)]
    #[ts(skip)]
    pub sources: Vec<DownloadSource>,
    pub concurrency: u32,
    pub minecraft_dir: String,
    /// 全局限速（bytes/s）；0/None = 不限制。fetcher 全局生效。
    #[serde(default)]
    #[ts(type = "number")]
    pub max_speed: u64,
    /// 游戏下载源模式（版本列表/json、资源索引与对象、支持库、client JAR、ModLoader 构件与安装器、
    /// Mojang JDK runtime 统一走此源）
    #[serde(default)]
    pub game_source_mode: DownloadSourceMode,
    /// 社区下载源模式：official = Modrinth API + cdn.modrinth.com；
    /// china = MCIM（mod.mcimirror.top，镜像 Modrinth/CurseForge API 与文件 CDN）。
    #[serde(default)]
    pub community_source_mode: DownloadSourceMode,
    /// 网络代理模式：auto（自动检测系统代理）/ system（强制系统代理）/ direct（强制直连）。
    #[serde(default)]
    pub network_mode: NetworkMode,
    /// 仅前端展示使用（后端仅存储透传；en 模式下搜索命令跳过翻译请求）。
    #[serde(default)]
    pub translate_display: String,
    /// 社区版本列表是否只显示正式版（过滤快照/预发布）；仅前端使用，后端存储透传。
    #[serde(default = "default_show_release_only")]
    pub show_release_only: bool,
    /// JDK（managed runtime）下载源模式：auto 智能回退 / officialOnly 仅官方 / mirrorOnly 仅镜像
    #[serde(default)]
    pub jdk_source_mode: JdkSourceMode,
}

/// JDK（managed runtime）下载源模式：
/// - auto（默认）：Mojang 官方 runtime 优先，失败依次回退 Adoptium（TUNA 清华镜像优先，404 回退官方 link）与 Azul
/// - officialOnly：仅官方（Mojang runtime → Azul）；mirrorOnly：仅镜像（Adoptium 仅 TUNA 下载候选）
/// - 元数据仍走 Adoptium 官方 API——TUNA 只镜像文件、不提供 checksum，属不可绕过的例外
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, ts_rs::TS)]
#[ts(export_to = "config/")]
#[serde(rename_all = "camelCase")]
pub enum JdkSourceMode {
    /// 仅官方源（Mojang runtime → Azul）
    OfficialOnly,
    /// 仅镜像（Adoptium TUNA 文件；元数据 API 例外走官方）
    MirrorOnly,
    /// 智能回退：官方 → Adoptium（TUNA→官方）→ Azul 全链
    #[default]
    Auto,
}

deserialize_mode!(JdkSourceMode);

fn default_show_release_only() -> bool {
    true
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            sources: default_sources(),
            concurrency: 64,
            max_speed: 0,
            minecraft_dir: ".minecraft".into(),
            game_source_mode: DownloadSourceMode::Auto,
            community_source_mode: DownloadSourceMode::Auto,
            network_mode: NetworkMode::Auto,
            translate_display: "zh".into(),
            show_release_only: true,
            jdk_source_mode: JdkSourceMode::Auto,
        }
    }
}

impl DownloaderConfig {
    /// 绝对化的游戏目录。绝对路径原样返回（`set_active` 写入的总是绝对路径）；
    /// 相对路径基于 `util::default_game_dir_base()` 而非进程 cwd 解析（避免从任意目录启动时错位）；
    /// 无默认基准（macOS/Linux）时退化为配置目录（防御）。
    pub fn minecraft_dir_abs(&self) -> std::path::PathBuf {
        let p = std::path::PathBuf::from(&self.minecraft_dir);
        if p.is_absolute() {
            p
        } else {
            util::default_game_dir_base()
                .unwrap_or_else(util::config_dir)
                .join(p)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader;

    /// 旧配置（preferOfficial/preferChina）迁移到 Auto；三态正常解析；未知值兜底 Auto。
    #[test]
    fn source_mode_legacy_values_migrate_to_auto() {
        fn parse(s: &str) -> DownloadSourceMode {
            serde_json::from_str(&format!("\"{s}\"")).unwrap()
        }
        assert_eq!(parse("preferOfficial"), DownloadSourceMode::Auto);
        assert_eq!(parse("preferChina"), DownloadSourceMode::Auto);
        assert_eq!(parse("officialOnly"), DownloadSourceMode::OfficialOnly);
        assert_eq!(parse("mirrorOnly"), DownloadSourceMode::MirrorOnly);
        assert_eq!(parse("auto"), DownloadSourceMode::Auto);
        assert_eq!(parse("whatever"), DownloadSourceMode::Auto);
        // 默认值 = Auto（智能模式）
        assert_eq!(DownloadSourceMode::default(), DownloadSourceMode::Auto);
        // 序列化回 camelCase 三态
        assert_eq!(
            serde_json::to_string(&DownloadSourceMode::Auto).unwrap(),
            "\"auto\""
        );
    }

    /// 镜像源把 ModLoader 官方 maven 重写到 BMCLAPI；官方源原样；未知域名不动。
    /// NeoForge 的 `/releases` 段必须去掉（BMCLAPI 的 `/maven` 前缀已指向该仓库根，保留即 404）。
    #[test]
    fn mirror_url_rewrites_modloader_maven_on_china_source() {
        let china = downloader::endpoints::mirror_source();
        assert_eq!(
            china.mirror_url(
                "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
        );
        // universal / fancymodloader / modlauncher / mergetool 等 18 条只在去段形态下可达
        assert_eq!(
            china.mirror_url(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.248/neoforge-21.1.248-universal.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/21.1.248/neoforge-21.1.248-universal.jar"
        );
        assert_eq!(
            china.mirror_url(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/1.21/forge-1.21.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.21/forge-1.21.jar"
        );
        // 未知域名保持原样
        assert_eq!(
            china.mirror_url("https://libraries.minecraft.net/com/x/y.jar"),
            "https://bmclapi2.bangbang93.com/libraries/com/x/y.jar"
        );
        // 官方源不重写
        let official = downloader::endpoints::official_source();
        assert_eq!(
            official.mirror_url(
                "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
            ),
            "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
        );
    }

    /// JDK 源模式三态序列化 + 未知值兜底 Auto（防手改 TOML 非法值导致 section 反序列化失败回默认）。
    #[test]
    fn jdk_source_mode_parses_and_defaults_to_auto() {
        fn parse(s: &str) -> JdkSourceMode {
            serde_json::from_str(&format!("\"{s}\"")).unwrap()
        }
        assert_eq!(parse("officialOnly"), JdkSourceMode::OfficialOnly);
        assert_eq!(parse("mirrorOnly"), JdkSourceMode::MirrorOnly);
        assert_eq!(parse("auto"), JdkSourceMode::Auto);
        assert_eq!(parse("whatever"), JdkSourceMode::Auto);
        // 默认值 = Auto（智能回退）
        assert_eq!(JdkSourceMode::default(), JdkSourceMode::Auto);
        // 序列化回 camelCase 三态
        assert_eq!(
            serde_json::to_string(&JdkSourceMode::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(
            serde_json::to_string(&JdkSourceMode::MirrorOnly).unwrap(),
            "\"mirrorOnly\""
        );
        // DownloaderConfig 缺省 jdk_source_mode → Auto（旧配置兼容）
        let cfg: DownloaderConfig = toml::from_str("concurrency = 64\nminecraft_dir = \".minecraft\"\ngame_source_mode = \"auto\"\ncommunity_source_mode = \"auto\"\n").unwrap();
        assert_eq!(cfg.jdk_source_mode, JdkSourceMode::Auto);
    }

    /// sources（URL 模板）永不持久化：旧配置里的 [[sources]] URL 快照读取时忽略
    /// （永远用代码内置模板），序列化也不再落盘。
    #[test]
    fn sources_not_persisted_and_legacy_rows_ignored() {
        let legacy = "concurrency = 64\nminecraft_dir = \".minecraft\"\ngame_source_mode = \"mirrorOnly\"\ncommunity_source_mode = \"mirrorOnly\"\n\n[[sources]]\nname = \"旧快照\"\nmanifest_url = \"https://stale.example.com\"\n";
        let cfg: DownloaderConfig = toml::from_str(legacy).unwrap();
        // 模式正常解析；URL 快照被忽略 → sources 回代码内置模板
        assert_eq!(cfg.game_source_mode, DownloadSourceMode::MirrorOnly);
        assert_eq!(cfg.community_source_mode, DownloadSourceMode::MirrorOnly);
        assert_eq!(cfg.sources.len(), 2);
        assert_eq!(
            cfg.sources[0].manifest_url,
            downloader::endpoints::MOJANG_MANIFEST
        );
        assert_eq!(cfg.sources[1].manifest_url, downloader::endpoints::BMCLAPI);
        // 序列化不含 sources 段（Settings.toml 只落模式，不落 URL）
        let out = toml::to_string(&cfg).unwrap();
        assert!(!out.contains("sources"), "不应再写 sources 段: {out}");
        assert!(!out.contains("https://"), "不应再写任何 URL: {out}");
    }
}
