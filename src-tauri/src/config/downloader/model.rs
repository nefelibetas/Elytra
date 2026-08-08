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

    pub fn mirror_url(&self, original: &str) -> String {
        if !self.manifest_url.contains("bmclapi") {
            return original.to_string();
        }
        for (domain, replacement) in REWRITE_RULES {
            if original.contains(domain) {
                let path = original.split(domain).nth(1).unwrap_or("");
                return format!("{}{}", replacement, path);
            }
        }
        original.to_string()
    }
}

const REWRITE_RULES: &[(&str, &str)] = &[
    ("piston-meta.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("piston-data.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("launcher.mojang.com", "https://bmclapi2.bangbang93.com"),
    ("launchermeta.mojang.com", "https://bmclapi2.bangbang93.com"),
    (
        "libraries.minecraft.net",
        "https://bmclapi2.bangbang93.com/libraries",
    ),
    (
        "resources.download.minecraft.net",
        "https://bmclapi2.bangbang93.com/assets",
    ),
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
];

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

        forge_list_url: "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/maven-metadata.xml".into(),
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

/// 下载源选择模式：
/// - officialOnly / chinaOnly：固定单一源，失败不回退
/// - preferOfficial / preferChina：优先主源，失败时回退到另一源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DownloadSourceMode {
    #[default]
    PreferOfficial,
    PreferChina,
    OfficialOnly,
    ChinaOnly,
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
    /// 下载速度上限（bytes/s），0 = 无限制
    #[serde(default)]
    pub max_speed: u64,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            sources: vec![official(), china()],
            concurrency: 64,
            minecraft_dir: ".minecraft".into(),
            file_source_mode: DownloadSourceMode::PreferOfficial,
            manifest_source_mode: DownloadSourceMode::PreferOfficial,
            community_source_mode: DownloadSourceMode::PreferOfficial,
            max_speed: 0,
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

    /// 文件下载主源（按 file_source_mode 选择），文件下载失败不会跨源回退。
    pub fn file_primary_source(&self) -> Option<&DownloadSource> {
        match self.file_source_mode {
            DownloadSourceMode::OfficialOnly | DownloadSourceMode::PreferOfficial => {
                self.sources.first()
            }
            DownloadSourceMode::ChinaOnly | DownloadSourceMode::PreferChina => self.sources.last(),
        }
    }

    /// 文件下载按 file_source_mode 的完整源顺序（含备用源，用于过慢降级）。
    pub fn file_sources_list(&self) -> Vec<&DownloadSource> {
        self.sources_in_order(self.file_source_mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 镜像源必须把 ModLoader 官方 maven 重写到 BMCLAPI 镜像，
    /// 官方源保持原样；未知域名不动。
    #[test]
    fn mirror_url_rewrites_modloader_maven_on_china_source() {
        let china = china();
        assert_eq!(
            china.mirror_url("https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"),
            "https://bmclapi2.bangbang93.com/maven/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
        );
        assert_eq!(
            china.mirror_url("https://maven.minecraftforge.net/net/minecraftforge/forge/1.21/forge-1.21.jar"),
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
            official.mirror_url("https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"),
            "https://maven.neoforged.net/releases/net/neoforged/bus/8.0.5/bus-8.0.5.jar"
        );
    }
}
