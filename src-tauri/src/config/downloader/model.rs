use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

fn official() -> DownloadSource {
    DownloadSource {
        name: "官方".into(),
        manifest_url: "https://piston-meta.mojang.com".into(),
        resource_url: "https://resources.download.minecraft.net".into(),
        maven_url: "https://libraries.minecraft.net".into(),
        libraries_url: "https://libraries.minecraft.net".into(),

        forge_list_url: "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml".into(),
        fabric_list_url: "https://meta.fabricmc.net/v2/versions/loader/{mc}".into(),
        neoforge_list_url: "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml".into(),
        quilt_list_url: "https://meta.quiltmc.org/v3/versions/loader/{mc}".into(),
        optifine_list_url: "https://bmclapi2.bangbang93.com/optifine/{mc}".into(),
        forge_installer_url: "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar".into(),
        neoforge_installer_url: "https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar".into(),
        fabric_loader_url: "https://maven.fabricmc.net/net/fabricmc/fabric-loader/{ver}/fabric-loader-{ver}.jar".into(),
        fabric_profile_url: "https://meta.fabricmc.net/v2/versions/loader/{mc}/{loaderVer}/profile/json".into(),
        quilt_profile_url: "https://meta.quiltmc.org/v3/versions/loader/{mc}/{loaderVer}/profile/json".into(),
        optifine_download_url: "https://bmclapi2.bangbang93.com/optifine/{mc}/{type}/{patch}".into(),
    }
}

fn china() -> DownloadSource {
    DownloadSource {
        name: "国内镜像".into(),
        manifest_url: "https://bmclapi2.bangbang93.com".into(),
        resource_url: "https://bmclapi2.bangbang93.com/assets".into(),
        maven_url: "https://bmclapi2.bangbang93.com/maven".into(),
        libraries_url: "https://bmclapi2.bangbang93.com/libraries".into(),

        // ponytail: BMCLAPI 的 forge maven-metadata 停更于 1.18.2-40.1.0(缺 26.2+ 全部新版本),
        // 列表直连官方 maven.minecraftforge.net(几十 KB 小文件,官方直连稳定);
        // 文件层(installer/库)BMCLAPI 镜像 302 实测可用,不受影响
        forge_list_url: "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml".into(),
        fabric_list_url: "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{mc}".into(),
        neoforge_list_url: "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/maven-metadata.xml".into(),
        quilt_list_url: "https://meta.quiltmc.org/v3/versions/loader/{mc}".into(),
        optifine_list_url: "https://bmclapi2.bangbang93.com/optifine/{mc}".into(),

        forge_installer_url: "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar".into(),
        neoforge_installer_url: "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar".into(),
        fabric_loader_url: "https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/{ver}/fabric-loader-{ver}.jar".into(),
        fabric_profile_url: "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{mc}/{loaderVer}/profile/json".into(),
        // ponytail: BMCLAPI 无 quilt-meta 镜像，与 quilt_list_url 一致直连官方
        quilt_profile_url: "https://meta.quiltmc.org/v3/versions/loader/{mc}/{loaderVer}/profile/json".into(),
        optifine_download_url: "https://bmclapi2.bangbang93.com/optifine/{mc}/{type}/{patch}".into(),
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

/// 按 URL host 推断资源类（BMCLAPI/MCIM 镜像 URL 按映射前域名推断）。
/// 未知 → Other（不做镜像候选生成，保持原始 URL 列表）。
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
///   检测到即走代理；无代理时直连（TUN 模式由网络层接管，无需应用配置）
/// - system：强制使用系统代理（检测不到时回退直连并告警）
/// - direct：强制直连（不走应用层代理；TUN 模式仍由网络层生效）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NetworkMode {
    #[default]
    Auto,
    System,
    Direct,
}

/// 下载源选择模式：
/// - auto（默认）：文件下载按 URL 自动分类，镜像候选即时生成并按健康度排序，
///   元数据/清单请求官方优先、超时（4s）自动降级镜像——无需人工指定优先级
/// - officialOnly / chinaOnly：单选，只用一个源，不自动回退
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DownloadSourceMode {
    /// 仅官方源（版本清单/文件/社区 API 全走官方）
    OfficialOnly,
    /// 仅国内镜像源（BMCLAPI / MCIM）
    ChinaOnly,
    /// 自适应：程序自动判断更快源（健康度路由 + 过慢降级）
    #[default]
    Auto,
}

/// 自定义反序列化（含旧配置迁移）：历史上 preferOfficial/preferChina 两态并入 Auto——
/// preferOfficial 语义（官方优先+慢降级）与 Auto 完全一致；
/// preferChina（镜像优先）在 Auto 下由健康度自适应（官方失败/过慢 → 镜像接管）等效实现。
impl<'de> serde::Deserialize<'de> for DownloadSourceMode {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "officialOnly" => Self::OfficialOnly,
            "chinaOnly" => Self::ChinaOnly,
            // preferOfficial / preferChina / auto / 未知值 → 自适应
            _ => Self::Auto,
        })
    }
}

fn default_sources() -> Vec<DownloadSource> {
    vec![official(), china()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderConfig {
    /// 前端保存时可省略（不提供自定义源功能），缺省回默认官方+镜像源
    #[serde(default = "default_sources")]
    pub sources: Vec<DownloadSource>,
    pub concurrency: u32,
    pub minecraft_dir: String,
    /// 文件下载源模式（client JAR / 支持库 / 资源 / ModLoader）
    #[serde(default)]
    pub file_source_mode: DownloadSourceMode,
    /// 版本列表源模式（版本清单 manifest）
    #[serde(default)]
    pub manifest_source_mode: DownloadSourceMode,
    /// 社区下载源模式（Modrinth 官方 API / CDN vs MCIM 国内镜像）。
    /// 管理 Fabric API 等社区资源：official = Modrinth API + cdn.modrinth.com，
    /// china = MCIM（mod.mcimirror.top，镜像 Modrinth/CurseForge API 与文件 CDN）。
    #[serde(default)]
    pub community_source_mode: DownloadSourceMode,
    /// 网络代理模式：auto（自动检测系统代理）/ system（强制系统代理）/ direct（强制直连）。
    #[serde(default)]
    pub network_mode: NetworkMode,
    /// 社区搜索结果翻译显示模式：zh = 仅中文、both = 中英对照、en = 仅英文。
    /// 仅前端展示使用（后端仅存储透传；en 模式下搜索命令跳过翻译请求）。
    #[serde(default)]
    pub translate_display: String,
    /// 社区资源版本列表是否只显示正式版（过滤快照/预发布/候选等非正式版本）。
    /// 仅前端展示使用，后端仅存储透传。
    #[serde(default = "default_show_release_only")]
    pub show_release_only: bool,
}

fn default_show_release_only() -> bool {
    true
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            sources: vec![official(), china()],
            concurrency: 64,
            minecraft_dir: ".minecraft".into(),
            file_source_mode: DownloadSourceMode::Auto,
            manifest_source_mode: DownloadSourceMode::Auto,
            community_source_mode: DownloadSourceMode::Auto,
            network_mode: NetworkMode::Auto,
            translate_display: "zh".into(),
            show_release_only: true,
        }
    }
}

impl DownloaderConfig {
    /// 绝对化的游戏目录（相对路径基于启动器进程 cwd 解析）。
    /// 启动参数（classpath/natives）与文件操作统一走绝对路径，
    /// 避免基于不同 cwd 解析出错。
    pub fn minecraft_dir_abs(&self) -> std::path::PathBuf {
        std::path::absolute(std::path::PathBuf::from(&self.minecraft_dir))
            .unwrap_or_else(|_| std::path::PathBuf::from(&self.minecraft_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧配置（preferOfficial/preferChina）反序列化迁移到 Auto；
    /// 三态正常解析；未知值兜底 Auto。
    #[test]
    fn source_mode_legacy_values_migrate_to_auto() {
        fn parse(s: &str) -> DownloadSourceMode {
            serde_json::from_str(&format!("\"{s}\"")).unwrap()
        }
        assert_eq!(parse("preferOfficial"), DownloadSourceMode::Auto);
        assert_eq!(parse("preferChina"), DownloadSourceMode::Auto);
        assert_eq!(parse("officialOnly"), DownloadSourceMode::OfficialOnly);
        assert_eq!(parse("chinaOnly"), DownloadSourceMode::ChinaOnly);
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

    /// 镜像源必须把 ModLoader 官方 maven 重写到 BMCLAPI 镜像，
    /// 官方源保持原样；未知域名不动。
    #[test]
    fn mirror_url_rewrites_modloader_maven_on_china_source() {
        let china = china();
        assert_eq!(
            china.mirror_url(
                "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
            ),
            "https://bmclapi2.bangbang93.com/maven/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
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
        let official = official();
        assert_eq!(
            official.mirror_url(
                "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
            ),
            "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
        );
    }
}
