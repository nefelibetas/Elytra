//! Modrinth 平台实现（无需 key）：搜索 / 版本列表 / 详情原始 JSON。

use super::shared::{api_urls, get_json_multi, version_range, ApiPlatform};
use super::wiki_entry;
use super::{
    CommunityDependency, CommunityDependencyItem, CommunitySearchPage, CommunitySearchResult,
    CommunityVersion,
};
use crate::downloader::fetcher::http;

/// 已知 ModLoader 名（用于从 Modrinth categories 中过滤 loader 标签）。
const KNOWN_LOADERS: [&str; 4] = ["fabric", "forge", "neoforge", "quilt"];

/// 从 Modrinth 搜索 hit 的 categories 数组提取已知 loader 名（去重保序）。
fn modrinth_loaders(categories: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = categories.as_array() {
        for c in arr {
            let Some(name) = c.as_str() else { continue };
            let lower = name.to_lowercase();
            if KNOWN_LOADERS.contains(&lower.as_str()) && !out.contains(&lower) {
                out.push(lower);
            }
        }
    }
    out
}

/// Modrinth 支持的 MC 版本范围：latest_version 与 versions 合并后取 min ~ max。
fn modrinth_version_range(hit: &serde_json::Value) -> Option<String> {
    let mut versions: Vec<String> = Vec::new();
    if let Some(latest) = hit["latest_version"].as_str() {
        versions.push(latest.to_string());
    }
    if let Some(arr) = hit["versions"].as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                versions.push(s.to_string());
            }
        }
    }
    version_range(&versions)
}

// ── Modrinth（无需 key）──

pub(super) async fn search_modrinth(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u64,
    page_size: u32,
) -> Result<CommunitySearchPage, String> {
    let mut facets: Vec<Vec<String>> = Vec::new();
    if let Some(gv) = game_version {
        facets.push(vec![format!("versions:{}", gv)]);
    }
    if let Some(ld) = loader {
        facets.push(vec![format!("categories:{}", ld)]);
    }
    // Modrinth /v2/search 不带 project_type facet 时返回全部项目类型
    // （modpack/resourcepack 等会混入 mod 搜索结果，实测确认），必须显式过滤：
    // 普通 mod 搜索（category None）固定 project_type:mod；
    // 分类搜索按类型映射——注意 datapack 无 project_type 实体（数据包项目是
    // mod 类型 + datapack 标签，需双 facet：project_type:mod AND categories:datapack）；
    // worlds 无对应类型（前端已锁定 CurseForge 单源，此处防御性返回空）
    match category {
        Some("datapack") => {
            facets.push(vec!["project_type:mod".into()]);
            facets.push(vec!["categories:datapack".into()]);
        }
        Some("worlds") => {
            return Ok(CommunitySearchPage {
                items: Vec::new(),
                total: 0,
            });
        }
        _ => facets.push(vec![format!("project_type:{}", category.unwrap_or("mod"))]),
    }
    let mut params: Vec<(&str, String)> = vec![
        ("query", query.to_string()),
        ("limit", page_size.to_string()),
        ("offset", offset.to_string()),
    ];
    if !facets.is_empty() {
        let facets_json =
            serde_json::to_string(&facets).map_err(common_err_ctx!("序列化数据失败"))?;
        params.push(("facets", facets_json));
    }
    // 空关键字时按下载量排（对齐 PCL2：不输入名称直接显示热门 Mod）
    if query.is_empty() {
        params.push(("index", "downloads".into()));
    }
    let urls: Vec<String> = api_urls(ApiPlatform::Modrinth, "/v2/search", &params)?;
    let data = get_json_multi(&urls, |u| http().get(u.to_string())).await?;

    let hits = data["hits"]
        .as_array()
        .ok_or_else(|| "搜索响应缺少 hits".to_string())?;
    let items: Vec<CommunitySearchResult> = hits
        .iter()
        .filter_map(|h| {
            let title = h["title"].as_str()?.to_string();
            let entry =
                wiki_entry::WIKI.lookup("modrinth", h["slug"].as_str().unwrap_or(""));
            let title_zh = entry
                .and_then(|e| e.chinese_raw.as_deref())
                .and_then(wiki_entry::chinese_name);
            Some(CommunitySearchResult {
                id: h["project_id"].as_str()?.to_string(),
                title,
                title_zh,
                mcmod_id: entry.map(|e| e.mcmod_id),
                description: h["description"].as_str().unwrap_or("").to_string(),
                translated: None,
                loaders: modrinth_loaders(&h["categories"]),
                game_version_range: modrinth_version_range(h),
                author: h["author"].as_str().unwrap_or("").to_string(),
                downloads: h["downloads"].as_u64().unwrap_or(0),
                icon_url: h["icon_url"].as_str().map(str::to_string),
                // 项目 URL 按 project_type 分段（mod/shader/resourcepack/datapack/modpack），
                // 写死 /mod/ 会让光影包等详情页「转到 Modrinth」404
                page_url: format!(
                    "https://modrinth.com/{}/{}",
                    h["project_type"].as_str().unwrap_or("mod"),
                    h["slug"].as_str().unwrap_or("")
                ),
                category: category.map(str::to_string),
            })
        })
        .collect();
    let total = data["total_hits"].as_u64().unwrap_or(items.len() as u64);
    Ok(CommunitySearchPage { items, total })
}

pub(super) async fn modrinth_versions(
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<CommunityVersion>, String> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(gv) = game_version {
        let arr = serde_json::to_string(&vec![gv]).map_err(common_err_ctx!("序列化数据失败"))?;
        params.push(("game_versions", arr));
    }
    if let Some(ld) = loader {
        let arr = serde_json::to_string(&vec![ld]).map_err(common_err_ctx!("序列化数据失败"))?;
        params.push(("loaders", arr));
    }
    let urls: Vec<String> = api_urls(
        ApiPlatform::Modrinth,
        &format!("/v2/project/{project_id}/version"),
        &params,
    )?;
    let data = get_json_multi(&urls, |u| http().get(u.to_string())).await?;

    let arr = data
        .as_array()
        .ok_or_else(|| "版本响应格式错误".to_string())?;
    Ok(arr
        .iter()
        .filter_map(|v| {
            let files = v["files"].as_array()?;
            let file = files
                .iter()
                .find(|f| f["primary"].as_bool() == Some(true))
                .or_else(|| files.first())?;
            let hashes = file["hashes"].as_object();
            Some(CommunityVersion {
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
                sha1: hashes
                    .and_then(|h| h.get("sha1"))
                    .and_then(|s| s.as_str())
                    .map(str::to_string),
                date_published: v["date_published"].as_str().unwrap_or("").to_string(),
                downloads: v["downloads"].as_u64().unwrap_or(0),
                version_type: v["version_type"].as_str().unwrap_or("unknown").to_string(),
                dependencies: v["dependencies"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|d| {
                                // 仅保留带项目 ID 的依赖（部分依赖只有 version_id，无法定位项目）
                                Some(CommunityDependency {
                                    project_id: d["project_id"].as_str()?.to_string(),
                                    relation: d["dependency_type"]
                                        .as_str()
                                        .unwrap_or("unknown")
                                        .to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect())
}

/// 批量拉取 Modrinth 项目原始 JSON（GET /v2/projects?ids=[...]），保持响应顺序。
/// pub(crate)：依赖卡片详情（本模块）与本地 Mod 元数据识别（mod_updates）
/// 共用同一端点与解析入口，消除两套同端点实现。
pub(crate) async fn modrinth_projects_raw(
    ids: &[String],
) -> Result<Vec<serde_json::Value>, String> {
    let ids_json = serde_json::to_string(ids).map_err(common_err_ctx!("序列化数据失败"))?;
    let urls: Vec<String> = api_urls(ApiPlatform::Modrinth, "/v2/projects", &[("ids", ids_json)])?;
    let data = get_json_multi(&urls, |u| http().get(u.to_string())).await?;
    let arr = data
        .as_array()
        .ok_or_else(|| "项目详情响应格式错误".to_string())?;
    Ok(arr.to_vec())
}

pub(super) async fn modrinth_details(
    ids: &[String],
) -> Result<Vec<CommunityDependencyItem>, String> {
    Ok(modrinth_projects_raw(ids)
        .await?
        .iter()
        .filter_map(|p| {
            Some(CommunityDependencyItem {
                id: p["id"].as_str()?.to_string(),
                title: p["title"].as_str()?.to_string(),
                description: p["description"].as_str().unwrap_or("").to_string(),
                icon_url: p["icon_url"].as_str().map(str::to_string),
                translated: None,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modrinth 无需 key，任何环境可跑（单发小请求）。
    #[tokio::test]
    async fn search_modrinth_works() {
        let page = search_modrinth("jei", Some("1.21.1"), Some("fabric"), None, 0, 30).await;
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
        let page = search_modrinth("", None, None, None, 0, 30).await.unwrap();
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

    /// Modrinth loader 提取：categories 中过滤已知 loader，杂项/重复忽略。
    #[test]
    fn modrinth_loaders_filters_known_ones() {
        let cats = serde_json::json!(["Fabric", "Mechanic", "fabric", "forge", "Adventure"]);
        assert_eq!(
            modrinth_loaders(&cats),
            vec!["fabric", "forge"],
            "大小写不敏感且去重保序"
        );
        assert!(
            modrinth_loaders(&serde_json::json!(["Adventure"])).is_empty(),
            "无 loader 返回空"
        );
        assert!(
            modrinth_loaders(&serde_json::json!(null)).is_empty(),
            "非数组返回空"
        );
    }

    /// Modrinth 版本范围：latest_version 与 versions 合并，过滤 snapshot，取 min ~ max。
    #[test]
    fn modrinth_version_range_min_to_max() {
        let hit = serde_json::json!({
            "latest_version": "1.21.1",
            "versions": ["1.20.1", "1.19.4", "24w14a", "1.9.4", "1.21.1"]
        });
        assert_eq!(
            modrinth_version_range(&hit),
            Some("1.9.4 ~ 1.21.1".to_string()),
            "snapshot 过滤，min ~ max"
        );
        let single = serde_json::json!({ "latest_version": "1.20.1", "versions": ["1.20.1"] });
        assert_eq!(
            modrinth_version_range(&single),
            Some("1.20.1".to_string()),
            "仅一个有效版本显示单个"
        );
        assert_eq!(
            modrinth_version_range(&serde_json::json!({})),
            None,
            "无版本返回 None"
        );
        assert_eq!(
            modrinth_version_range(&serde_json::json!({ "latest_version": "24w14a" })),
            None,
            "全部非法返回 None"
        );
    }
}
