use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadSource {
    pub name: String,
    pub manifest_url: String,
    pub resource_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloaderConfig {
    pub sources: Vec<DownloadSource>,
    pub active_source: String,
    pub concurrency: u32,
    pub minecraft_dir: String,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            sources: vec![
                DownloadSource {
                    name: "Mojang 官方".into(),
                    manifest_url: "https://piston-meta.mojang.com".into(),
                    resource_url: "https://resources.download.minecraft.net".into(),
                },
                DownloadSource {
                    name: "BMCLAPI 镜像".into(),
                    manifest_url: "https://bmclapi2.bangbang93.com".into(),
                    resource_url: "https://bmclapi2.bangbang93.com/assets".into(),
                },
            ],
            active_source: "0".into(),
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
}
