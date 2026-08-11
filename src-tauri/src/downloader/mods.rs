//! Mod 搜索 / 版本查询（Modrinth 官方 API + CurseForge 官方 API）。
//! 两个平台 API 完全不同（ID 体系、鉴权、返回结构），分开实现、统一出参。
//! CurseForge 需要 x-api-key（constants.rs 经 .env 注入）；Modrinth 无需 key。

use serde::Serialize;

use crate::constants;
use crate::downloader::fetcher;

const MODRINTH_API: &str = "https://api.modrinth.com";
const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
const CURSEFORGE_GAME_ID: &str = "432";
/// 搜索类交互请求超时（默认 client 300s 太长，搜索卡住不该让前端挂 5 分钟）
const QUERY_TIMEOUT_SECS: u64 = 10;
/// 每页条数（两平台保持一致，前端分页器按此推算总页数）
const PAGE_SIZE: u64 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct ModSearchResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub page_url: String,
}

/// 搜索分页结果（两平台分页语义不同：Modrinth offset / CurseForge index，统一透传页码起点）。
#[derive(Debug, Clone, Serialize)]
pub struct ModSearchPage {
    pub items: Vec<ModSearchResult>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModVersion {
    pub version_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub filename: String,
    pub download_url: String,
    pub file_size: u64,
    pub sha1: Option<String>,
    pub date_published: String,
    pub downloads: u64,
}

/// CurseForge modLoaderType 枚举（部分）：forge=1、fabric=4、quilt=5、neoforge=6。
fn curseforge_loader_id(loader: &str) -> Option<i64> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

/// CurseForge 项目大类 classId（光影包等非 mod 类别走 classId 过滤）。
fn curseforge_class_id(category: &str) -> Option<i64> {
    match category {
        "shader" => Some(6552),
        "resourcepack" => Some(12),
        "datapack" => Some(6945),
        "modpack" => Some(4471),
        _ => None,
    }
}

/// 从 CurseForge gameVersions（如 ["Client","NeoForge","Server","26.2"]）提取已知 loader 名。
fn curseforge_loaders(game_versions: &[String]) -> Vec<String> {
    game_versions
        .iter()
        .filter_map(|v| match v.as_str() {
            "Forge" => Some("forge"),
            "Fabric" => Some("fabric"),
            "NeoForge" => Some("neoforge"),
            "Quilt" => Some("quilt"),
            _ => None,
        })
        .map(str::to_string)
        .collect()
}

/// JSON 交互请求（10s 超时）：直接 GET 解析，不做文件下载层的重试/换源（搜索失败即返回）。
async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let resp = fetcher::http()
        .get(url)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败: {}", e))
}

// ── Modrinth（无需 key）──

async fn search_modrinth(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u64,
) -> Result<ModSearchPage, String> {
    let mut facets: Vec<Vec<String>> = Vec::new();
    if let Some(gv) = game_version {
        facets.push(vec![format!("versions:{}", gv)]);
    }
    if let Some(ld) = loader {
        facets.push(vec![format!("categories:{}", ld)]);
    }
    // 光影包等非 mod 项目类型（Modrinth 的 project_type facet）
    if let Some(cat) = category {
        facets.push(vec![format!("project_type:{}", cat)]);
    }
    let mut params: Vec<(&str, String)> = vec![
        ("query", query.to_string()),
        ("limit", PAGE_SIZE.to_string()),
        ("offset", offset.to_string()),
    ];
    if !facets.is_empty() {
        let facets_json = serde_json::to_string(&facets).map_err(|e| e.to_string())?;
        params.push(("facets", facets_json));
    }
    // 空关键字时按下载量排（对齐 PCL2：不输入名称直接显示热门 Mod）
    if query.is_empty() {
        params.push(("index", "downloads".into()));
    }
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v2/search", MODRINTH_API),
        &params,
    )
    .map_err(|e| format!("URL 构造失败: {}", e))?;
    let data = get_json(url.as_str()).await?;

    let hits = data["hits"]
        .as_array()
        .ok_or_else(|| "搜索响应缺少 hits".to_string())?;
    let items: Vec<ModSearchResult> = hits
        .iter()
        .filter_map(|h| {
            Some(ModSearchResult {
                id: h["project_id"].as_str()?.to_string(),
                title: h["title"].as_str()?.to_string(),
                description: h["description"].as_str().unwrap_or("").to_string(),
                author: h["author"].as_str().unwrap_or("").to_string(),
                downloads: h["downloads"].as_u64().unwrap_or(0),
                icon_url: h["icon_url"].as_str().map(str::to_string),
                page_url: format!(
                    "https://modrinth.com/mod/{}",
                    h["slug"].as_str().unwrap_or("")
                ),
            })
        })
        .collect();
    let total = data["total_hits"].as_u64().unwrap_or(items.len() as u64);
    Ok(ModSearchPage { items, total })
}

async fn modrinth_versions(
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModVersion>, String> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(gv) = game_version {
        let arr = serde_json::to_string(&vec![gv]).map_err(|e| e.to_string())?;
        params.push(("game_versions", arr));
    }
    if let Some(ld) = loader {
        let arr = serde_json::to_string(&vec![ld]).map_err(|e| e.to_string())?;
        params.push(("loaders", arr));
    }
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v2/project/{}/version", MODRINTH_API, project_id),
        &params,
    )
    .map_err(|e| format!("URL 构造失败: {}", e))?;
    let data = get_json(url.as_str()).await?;

    let arr = data.as_array().ok_or_else(|| "版本响应格式错误".to_string())?;
    Ok(arr
        .iter()
        .filter_map(|v| {
            let files = v["files"].as_array()?;
            let file = files.iter().find(|f| f["primary"].as_bool() == Some(true)).or_else(|| files.first())?;
            let hashes = file["hashes"].as_object();
            Some(ModVersion {
                version_id: v["id"].as_str()?.to_string(),
                version_number: v["version_number"].as_str()?.to_string(),
                game_versions: v["game_versions"]
                    .as_array()?
                    .iter()
                    .filter_map(|g| g.as_str().map(str::to_string))
                    .collect(),
                loaders: v["loaders"]
                    .as_array()?
                    .iter()
                    .filter_map(|l| l.as_str().map(str::to_string))
                    .collect(),
                filename: file["filename"].as_str()?.to_string(),
                download_url: file["url"].as_str()?.to_string(),
                file_size: file["size"].as_u64().unwrap_or(0),
                sha1: hashes.and_then(|h| h.get("sha1")).and_then(|s| s.as_str()).map(str::to_string),
                date_published: v["date_published"].as_str().unwrap_or("").to_string(),
                downloads: v["downloads"].as_u64().unwrap_or(0),
            })
        })
        .collect())
}

// ── CurseForge（需要 x-api-key）──

/// 带 x-api-key 发 GET 并解析 JSON；key 未配置时直接报错（不 panic、不暴露 key）。
async fn curseforge_get(path: &str, params: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let key = constants::CURSEFORGE_API_KEY
        .ok_or_else(|| "未配置 CurseForge API Key（项目根 .env 的 ELYTRA_CURSEFORGE_API_KEY），无法使用 CurseForge 来源".to_string())?;
    let mut all: Vec<(&str, String)> = vec![("gameId", CURSEFORGE_GAME_ID.to_string())];
    all.extend_from_slice(params);
    let url = reqwest::Url::parse_with_params(&format!("{}{}", CURSEFORGE_API, path), &all)
        .map_err(|e| format!("URL 构造失败: {}", e))?;
    let resp = fetcher::http()
        .get(url)
        .header("x-api-key", key)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败: {}", e))
}

async fn search_curseforge(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u64,
) -> Result<ModSearchPage, String> {
    let mut params: Vec<(&str, String)> = vec![
        ("pageSize", PAGE_SIZE.to_string()),
        ("index", offset.to_string()),
    ];
    // 空关键字：不传 searchFilter（API 无热门概念，返回按名称序列表——API 限制）
    if !query.is_empty() {
        params.push(("searchFilter", query.to_string()));
    }
    if let Some(gv) = game_version {
        params.push(("gameVersion", gv.to_string()));
    }
    if let Some(ld) = loader {
        if let Some(id) = curseforge_loader_id(ld) {
            params.push(("modLoaderType", id.to_string()));
        }
    }
    if let Some(cat) = category {
        if let Some(id) = curseforge_class_id(cat) {
            params.push(("classId", id.to_string()));
        }
    }
    let data = curseforge_get("/mods/search", &params).await?;

    let arr = data["data"].as_array().ok_or_else(|| "搜索响应缺少 data".to_string())?;
    let items: Vec<ModSearchResult> = arr
        .iter()
        .filter_map(|m| {
            Some(ModSearchResult {
                id: m["id"].as_u64()?.to_string(),
                title: m["name"].as_str()?.to_string(),
                description: m["summary"].as_str().unwrap_or("").to_string(),
                author: m["authors"]
                    .as_array()?
                    .first()?
                    .get("name")?
                    .as_str()?
                    .to_string(),
                downloads: m["downloadCount"].as_u64().unwrap_or(0),
                icon_url: m["logo"]["url"].as_str().map(str::to_string),
                page_url: format!(
                    "https://www.curseforge.com/minecraft/mc-mods/{}",
                    m["slug"].as_str().unwrap_or("")
                ),
            })
        })
        .collect();
    let total = data["pagination"]["totalCount"]
        .as_u64()
        .unwrap_or(items.len() as u64);
    Ok(ModSearchPage { items, total })
}

async fn curseforge_versions(
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModVersion>, String> {
    let mut params: Vec<(&str, String)> = vec![("pageSize", "30".into())];
    if let Some(gv) = game_version {
        params.push(("gameVersion", gv.to_string()));
    }
    if let Some(ld) = loader {
        if let Some(id) = curseforge_loader_id(ld) {
            params.push(("modLoaderType", id.to_string()));
        }
    }
    let data = curseforge_get(&format!("/mods/{}/files", project_id), &params).await?;

    let arr = data["data"].as_array().ok_or_else(|| "文件响应缺少 data".to_string())?;
    Ok(arr
        .iter()
        .filter_map(|f| {
            let game_versions: Vec<String> = f["gameVersions"]
                .as_array()?
                .iter()
                .filter_map(|g| g.as_str().map(str::to_string))
                .collect();
            let loaders = curseforge_loaders(&game_versions);
            let sha1 = f["hashes"].as_array()?.iter().find(|h| h["algo"].as_u64() == Some(1))?;
            Some(ModVersion {
                version_id: f["id"].as_u64()?.to_string(),
                version_number: f["fileName"].as_str()?.to_string(),
                game_versions,
                loaders,
                filename: f["fileName"].as_str()?.to_string(),
                download_url: f["downloadUrl"].as_str().unwrap_or("").to_string(),
                file_size: f["fileLength"].as_u64().unwrap_or(0),
                sha1: sha1.get("value").and_then(|s| s.as_str()).map(str::to_string),
                date_published: f["fileDate"].as_str().unwrap_or("").to_string(),
                downloads: f["downloadCount"].as_u64().unwrap_or(0),
            })
        })
        .collect())
}

/// 搜索（按来源分派）。`category` 为项目类型（如 shader）或 None（普通 mod）。
pub async fn search_mods(
    source: &str,
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u64,
) -> Result<ModSearchPage, String> {
    match source {
        "modrinth" => search_modrinth(query, game_version, loader, category, offset).await,
        "curseforge" => search_curseforge(query, game_version, loader, category, offset).await,
        other => Err(format!("未知 Mod 来源: {}", other)),
    }
}

/// 版本列表（按来源分派）。
pub async fn get_mod_versions(
    source: &str,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModVersion>, String> {
    match source {
        "modrinth" => modrinth_versions(project_id, game_version, loader).await,
        "curseforge" => curseforge_versions(project_id, game_version, loader).await,
        other => Err(format!("未知 Mod 来源: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CurseForge API 对并发请求敏感（并发测试会偶发连接失败），串行化。
    static CF_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Modrinth 无需 key，任何环境可跑（单发小请求）。
    #[tokio::test]
    async fn search_modrinth_works() {
        let page = search_modrinth("jei", Some("1.21.1"), Some("fabric"), None, 0).await;
        assert!(page.is_ok(), "Modrinth 搜索应成功: {:?}", page.err());
        let page = page.unwrap();
        assert!(!page.items.is_empty(), "搜 jei 应返回结果");
        assert!(
            page.items.iter().any(|h| h.title.contains("JEI")),
            "结果应包含 JEI，实际: {:?}",
            page.items.iter().map(|h| &h.title).collect::<Vec<_>>()
        );
        assert!(page.total > 0, "应返回总数");
    }

    /// 空关键字：对齐 PCL2 行为，返回按下载量排序的热门列表。
    #[tokio::test]
    async fn modrinth_empty_query_returns_popular() {
        let page = search_modrinth("", None, None, None, 0).await.unwrap();
        assert!(!page.items.is_empty(), "空关键字应返回热门 Mod");
        let first = &page.items[0];
        assert!(
            page.items
                .windows(2)
                .all(|w| w[0].downloads >= w[1].downloads),
            "热门列表应按下载量降序，首个: {} ({}), 但存在逆序",
            first.title,
            first.downloads
        );
    }

    /// CurseForge 需 key：无 key 的环境跳过，本机（.env 已配置）走真实请求。
    #[tokio::test]
    async fn search_curseforge_works() {
        let _g = CF_LOCK.lock().await;
        if constants::CURSEFORGE_API_KEY.is_none() {
            eprintln!("跳过: 未配置 CurseForge API Key");
            return;
        }
        let page = search_curseforge("jei", Some("1.21.1"), None, None, 0).await;
        assert!(page.is_ok(), "CurseForge 搜索应成功: {:?}", page.err());
        let page = page.unwrap();
        assert!(!page.items.is_empty(), "搜 jei 应返回结果");
        let first = &page.items[0];
        assert!(
            !first.title.is_empty() && !first.author.is_empty() && !first.id.is_empty(),
            "结果结构应完整: {:?}",
            first
        );
    }

    /// CurseForge 搜索不匹配名称括号内的缩写（"JEI" 在 "Just Enough Items (JEI)" 中搜不到），
    /// 搜全名应命中 JEI（id 238222）。
    #[tokio::test]
    async fn curseforge_full_name_finds_jei() {
        let _g = CF_LOCK.lock().await;
        if constants::CURSEFORGE_API_KEY.is_none() {
            eprintln!("跳过: 未配置 CurseForge API Key");
            return;
        }
        let page = search_curseforge("just enough items", Some("1.21.1"), None, None, 0)
            .await
            .unwrap();
        assert!(
            page.items.iter().any(|h| h.id == "238222" || h.title.contains("Just Enough Items (JEI)")),
            "搜全名应命中 JEI"
        );
    }

    /// loader 提取：gameVersions 中已知 loader 名转小写。
    #[test]
    fn curseforge_loaders_extracts_known_names() {
        let gv = vec![
            "Client".to_string(),
            "NeoForge".to_string(),
            "Server".to_string(),
            "26.2".to_string(),
        ];
        assert_eq!(curseforge_loaders(&gv), vec!["neoforge"]);
    }
}
