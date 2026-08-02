use serde::{Deserialize, Serialize};

use crate::downloader::fetcher::http;

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
    pub liteloader_list_url: String,
    pub optifine_list_url: String,

    pub forge_installer_url: String,
    pub neoforge_installer_url: String,
    pub fabric_loader_url: String,
    pub fabric_profile_url: String,
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

    /// Apply mirror URL rewriting if this is a BMCLAPI source.
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
    ("libraries.minecraft.net", "https://bmclapi2.bangbang93.com/libraries"),
    ("resources.download.minecraft.net", "https://bmclapi2.bangbang93.com/assets"),
];

fn official() -> DownloadSource {
    DownloadSource {
        name: "官方".into(),
        manifest_url: "https://piston-meta.mojang.com".into(),
        resource_url: "https://resources.download.minecraft.net".into(),
        maven_url: "https://libraries.minecraft.net".into(),
        libraries_url: "https://libraries.minecraft.net".into(),

        forge_list_url: "https://bmclapi2.bangbang93.com/forge/minecraft/{mc}".into(),
        fabric_list_url: "https://meta.fabricmc.net/v2/versions/loader/{mc}".into(),
        neoforge_list_url: "https://bmclapi2.bangbang93.com/neoforge/list/{mc}".into(),
        quilt_list_url: "https://meta.quiltmc.org/v3/versions/loader/{mc}".into(),
        liteloader_list_url: "https://bmclapi2.bangbang93.com/liteloader/list".into(),
        optifine_list_url: "https://bmclapi2.bangbang93.com/optifine/{mc}".into(),

        forge_installer_url: "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar".into(),
        neoforge_installer_url: "https://maven.neoforged.net/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar".into(),
        fabric_loader_url: "https://maven.fabricmc.net/net/fabricmc/fabric-loader/{ver}/fabric-loader-{ver}.jar".into(),
        fabric_profile_url: "https://meta.fabricmc.net/v2/versions/loader/{mc}/{loaderVer}/profile/json".into(),
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

        forge_list_url: "https://bmclapi2.bangbang93.com/forge/minecraft/{mc}".into(),
        fabric_list_url: "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{mc}".into(),
        neoforge_list_url: "https://bmclapi2.bangbang93.com/neoforge/list/{mc}".into(),
        quilt_list_url: "https://meta.quiltmc.org/v3/versions/loader/{mc}".into(),
        liteloader_list_url: "https://bmclapi2.bangbang93.com/liteloader/list".into(),
        optifine_list_url: "https://bmclapi2.bangbang93.com/optifine/{mc}".into(),

        forge_installer_url: "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar".into(),
        neoforge_installer_url: "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar".into(),
        fabric_loader_url: "https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/{ver}/fabric-loader-{ver}.jar".into(),
        fabric_profile_url: "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/{mc}/{loaderVer}/profile/json".into(),
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
    /// 下载速度上限（bytes/s），0 = 无限制
    #[serde(default)]
    pub max_speed: u64,
    /// 正版登录时验证 SSL 证书（默认关闭）
    #[serde(default)]
    pub verify_ssl: bool,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            sources: vec![official(), china()],
            concurrency: 64,
            minecraft_dir: ".minecraft".into(),
            file_source_mode: DownloadSourceMode::PreferOfficial,
            manifest_source_mode: DownloadSourceMode::PreferOfficial,
            max_speed: 0,
            verify_ssl: false,
        }
    }
}

impl DownloaderConfig {
    /// 文件下载主源（按 file_source_mode 选择），文件下载失败不会跨源回退。
    pub fn file_primary_source(&self) -> Option<&DownloadSource> {
        match self.file_source_mode {
            DownloadSourceMode::OfficialOnly | DownloadSourceMode::PreferOfficial => {
                self.sources.first()
            }
            DownloadSourceMode::ChinaOnly | DownloadSourceMode::PreferChina => self.sources.last(),
        }
    }

    /// 按模式返回依次尝试的源列表。
    fn sources_in_order(&self, mode: DownloadSourceMode) -> Vec<&DownloadSource> {
        match mode {
            DownloadSourceMode::OfficialOnly => vec![&self.sources[0]],
            DownloadSourceMode::ChinaOnly => vec![&self.sources[1]],
            DownloadSourceMode::PreferOfficial => vec![&self.sources[0], &self.sources[1]],
            DownloadSourceMode::PreferChina => vec![&self.sources[1], &self.sources[0]],
        }
    }

    /// 文件下载按 file_source_mode 的完整源顺序（含备用源，用于过慢降级）。
    pub fn file_sources_list(&self) -> Vec<&DownloadSource> {
        self.sources_in_order(self.file_source_mode)
    }

    /// 优先模式下主源判慢时限（秒）：超时即降级到备用源。
    const SLOW_SOURCE_TIMEOUT_SECS: u64 = 5;

    /// Try fetching from each source in order; returns first success.
    /// 优先模式：主源超过 5s 未完成（过慢）即降级到备用源，不等待失败。
    pub async fn fetch_with_fallback(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let order = self.sources_in_order(mode);
        let mut last_err = String::new();
        for (i, source) in order.iter().enumerate() {
            let url = make_url(source);
            let mut req = http().get(&url);
            if i == 0 && order.len() > 1 {
                req = req.timeout(std::time::Duration::from_secs(Self::SLOW_SOURCE_TIMEOUT_SECS));
            }
            match req.send().await {
                Ok(resp) => match resp.text().await {
                    Ok(text) => return Ok(text),
                    Err(e) => last_err = format!("读取响应失败: {}", e),
                },
                Err(e) => last_err = format!("请求失败: {}", e),
            }
        }
        Err(last_err)
    }

    /// Try fetching JSON from each source; returns first success.
    /// 优先模式：主源超过 5s 未完成（过慢）即降级到备用源，不等待失败。
    pub async fn fetch_json_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        mode: DownloadSourceMode,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<T, String> {
        let order = self.sources_in_order(mode);
        let mut last_err = String::new();
        for (i, source) in order.iter().enumerate() {
            let url = make_url(source);
            let mut req = http().get(&url);
            if i == 0 && order.len() > 1 {
                req = req.timeout(std::time::Duration::from_secs(Self::SLOW_SOURCE_TIMEOUT_SECS));
            }
            match req.send().await {
                Ok(resp) => match resp.json::<T>().await {
                    Ok(data) => return Ok(data),
                    Err(e) => last_err = format!("解析 JSON 失败: {}", e),
                },
                Err(e) => last_err = format!("请求失败: {}", e),
            }
        }
        Err(last_err)
    }
}
