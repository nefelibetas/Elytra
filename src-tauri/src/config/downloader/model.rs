use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloaderConfig {
    pub mirror_url: String,
    pub concurrency: u32,
    pub minecraft_dir: String,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            mirror_url: "https://piston-meta.mojang.com".into(),
            concurrency: 8,
            minecraft_dir: ".minecraft".into(),
        }
    }
}
