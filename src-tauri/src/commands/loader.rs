use serde::Deserialize;

use crate::config::downloader::model::{DownloadSourceMode, DownloaderConfig};
use crate::constants;
use crate::persist;

#[derive(Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
}
#[derive(Deserialize)]
struct FabricLoader {
    version: String,
}

#[derive(Deserialize)]
struct QuiltEntry {
    loader: QuiltLoader,
}
#[derive(Deserialize)]
struct QuiltLoader {
    version: String,
}

#[derive(Deserialize)]
struct OptiFineEntry {
    patch: String,
    filename: String,
}

fn config() -> DownloaderConfig {
    persist::read(constants::SECT_DOWNLOADER).unwrap_or_default()
}

async fn fetch_versions<T: serde::de::DeserializeOwned>(
    make_url: impl Fn(&crate::config::downloader::model::DownloadSource) -> String,
) -> Result<Vec<T>, String> {
    let cfg = config();
    cfg.fetch_json_with_fallback::<Vec<T>>(cfg.file_source_mode, make_url)
        .await
}

/// 从 maven-metadata.xml 提取全部 `<version>...</version>`。
/// 官方 maven 与 BMCLAPI /maven/ 代理返回同格式，解析共用一套。
fn parse_maven_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|s| s.split("</version>").next().map(|v| v.to_string()))
        .collect()
}

/// 拉取 maven-metadata.xml 版本列表（多源回退）。
async fn fetch_maven_versions(
    make_url: impl Fn(&crate::config::downloader::model::DownloadSource) -> String,
) -> Result<Vec<String>, String> {
    let cfg = config();
    let xml = cfg
        .fetch_with_fallback(cfg.file_source_mode, make_url)
        .await?;
    Ok(parse_maven_versions(&xml))
}

#[tauri::command]
pub async fn get_forge_versions(mc_version: String) -> Result<Vec<String>, String> {
    // maven-metadata 版本号是 "{mc}-{ver}" 全名（如 "1.21.11-61.2.0"），
    // 过滤出当前 MC 并剥掉前缀，返回纯 ver；列表老在前，反转为新在前
    let prefix = format!("{}-", mc_version);
    let mut versions: Vec<String> = fetch_maven_versions(|s| s.forge_list_url.clone())
        .await?
        .into_iter()
        .filter_map(|v| v.strip_prefix(&prefix).map(|s| s.to_string()))
        .collect();
    versions.reverse();
    Ok(versions)
}

#[tauri::command]
pub async fn get_fabric_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<FabricEntry>(|s| {
        s.url(&s.fabric_list_url, &[("mc", &mc_version)])
    })
    .await?;
    Ok(data.into_iter().map(|e| e.loader.version).collect())
}

#[tauri::command]
pub async fn get_quilt_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<QuiltEntry>(|s| {
        s.url(&s.quilt_list_url, &[("mc", &mc_version)])
    })
    .await?;
    Ok(data.into_iter().map(|e| e.loader.version).collect())
}

#[tauri::command]
pub async fn get_neoforge_versions(mc_version: String) -> Result<Vec<String>, String> {
    // NeoForge 版本号内嵌 MC 版本："1.21.11" → "21.11.x"，"1.20.4" → "20.4.x"；
    // maven-metadata 里的快照（如 0.25w14craftmine.x）不匹配前缀自然排除
    let prefix = format!("{}.", mc_version.strip_prefix("1.").unwrap_or(mc_version.as_str()));
    let mut versions: Vec<String> = fetch_maven_versions(|s| s.neoforge_list_url.clone())
        .await?
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .collect();
    versions.reverse();
    Ok(versions)
}

#[tauri::command]
pub async fn get_optifine_versions(mc_version: String) -> Result<Vec<String>, String> {
    let data = fetch_versions::<OptiFineEntry>(|s| {
        s.url(&s.optifine_list_url, &[("mc", &mc_version)])
    })
    .await?;
    Ok(data
        .into_iter()
        .filter(|e| !e.filename.starts_with("preview_"))
        .map(|e| e.patch)
        .collect())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FabricApiVersion {
    pub version: String,
    pub download_url: String,
    pub filename: String,
}

#[tauri::command]
pub async fn get_fabric_api_versions(mc_version: String) -> Result<Vec<FabricApiVersion>, String> {
    let path = format!(
        "/v2/project/fabric-api/version?loaders=[\"fabric\"]&game_versions=[\"{}\"]",
        mc_version
    );
    // 社区源模式（配置 community_source_mode）：Modrinth 官方 API vs MCIM 镜像
    let cfg = config();
    let official = format!("https://api.modrinth.com{path}");
    let china = format!("https://mod.mcimirror.top/modrinth{path}");
    let urls = match cfg.community_source_mode {
        DownloadSourceMode::OfficialOnly => vec![official],
        DownloadSourceMode::ChinaOnly => vec![china],
        DownloadSourceMode::PreferChina => vec![china, official],
        DownloadSourceMode::PreferOfficial => vec![official, china],
    };
    let mut last_err = String::new();
    for (i, url) in urls.iter().enumerate() {
        let mut req = crate::downloader::fetcher::http().get(url);
        // 主源过慢（5s）即降级备用源，不等待失败
        if i == 0 {
            req = req.timeout(std::time::Duration::from_secs(5));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("请求 Fabric API 版本失败: {}", e);
                continue;
            }
        };
        let data: serde_json::Value = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                last_err = format!("解析版本列表失败: {}", e);
                continue;
            }
        };
        let arr = match data.as_array() {
            Some(a) => a,
            None => {
                last_err = "版本列表格式错误".into();
                continue;
            }
        };
        let versions: Vec<FabricApiVersion> = arr
            .iter()
            .filter_map(|v| {
                let version = v["version_number"].as_str()?.to_string();
                let file = v["files"].as_array()?.first()?;
                let download_url = file["url"].as_str()?.to_string();
                let filename = file["filename"].as_str()?.to_string();
                Some(FabricApiVersion {
                    version,
                    download_url,
                    filename,
                })
            })
            .collect();
        return Ok(versions);
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_maven_versions_extracts_all_versions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.minecraftforge</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <latest>1.21.11-61.2.0</latest>
    <release>1.21.11-61.2.0</release>
    <versions>
      <version>1.20.1-47.1.0</version>
      <version>1.21-51.0.0</version>
      <version>1.21.11-61.2.0</version>
    </versions>
  </versioning>
</metadata>"#;
        assert_eq!(
            parse_maven_versions(xml),
            vec![
                "1.20.1-47.1.0".to_string(),
                "1.21-51.0.0".to_string(),
                "1.21.11-61.2.0".to_string()
            ]
        );
    }

    #[test]
    fn parse_maven_versions_empty_when_no_versions() {
        assert!(parse_maven_versions("<metadata></metadata>").is_empty());
    }
}
