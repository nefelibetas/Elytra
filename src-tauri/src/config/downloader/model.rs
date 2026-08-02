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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloaderConfig {
    pub sources: Vec<DownloadSource>,
    pub active_source: String,
    pub concurrency: u32,
    pub minecraft_dir: String,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            sources: vec![official(), china()],
            active_source: "1".into(),
            concurrency: 8,
            minecraft_dir: ".minecraft".into(),
        }
    }
}

impl DownloaderConfig {
    pub fn active_source(&self) -> Option<&DownloadSource> {
        let idx: usize = self.active_source.parse().ok()?;
        self.sources.get(idx)
    }

    /// Try fetching from each source in order; returns first success.
    pub async fn fetch_with_fallback(
        &self,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<String, String> {
        let mut last_err = String::new();
        for source in &self.sources {
            let url = make_url(source);
            match http().get(&url).send().await {
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
    pub async fn fetch_json_with_fallback<T: serde::de::DeserializeOwned>(
        &self,
        make_url: impl Fn(&DownloadSource) -> String,
    ) -> Result<T, String> {
        let mut last_err = String::new();
        for source in &self.sources {
            let url = make_url(source);
            match http().get(&url).send().await {
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
