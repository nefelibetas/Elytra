//! CurseForge 平台实现（需要 x-api-key）：搜索 / 版本列表 / 详情 / 批量文件换链。

use serde::Serialize;

use super::shared::{
    api_routes, api_urls, get_json_multi, post_json_multi, version_range, ApiPlatform,
};
use super::wiki_entry;
use super::{
    CommunityDependency, CommunityDependencyItem, CommunitySearchPage, CommunitySearchResult,
    CommunityVersion,
};

use crate::constants;
use crate::downloader::fetcher::http;
use crate::persist::secrets;

const CURSEFORGE_GAME_ID: &str = "432";
const CURSEFORGE_KEY_SECRET: &str = "curseforge_api_key";
const CURSEFORGE_KEY_LEN: usize = 60;

/// CurseForge Key 来源（只暴露状态，不返回 key 本身）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "community/")]
#[serde(rename_all = "camelCase")]
pub enum CurseForgeKeySource {
    /// 用户在设置中填写的 key（优先）。
    User,
    /// 构建期内置 key。
    BuiltIn,
    /// 两者都没有。
    None,
}

/// CurseForge API Key 是 60 字符的 bcrypt 串（`$2a$10$` 前缀）。
fn validate_curseforge_key(key: &str) -> Result<(), String> {
    if key.len() == CURSEFORGE_KEY_LEN && key.starts_with("$2a$10$") && key.is_ascii() {
        Ok(())
    } else {
        Err(keyed_msg!(
            "errors.community.curseforgeKeyInvalid",
            "CurseForge API Key 格式无效\n应为 60 字符、以 $2a$10$ 开头的字符串"
        ))
    }
}

fn read_user_api_key() -> Result<Option<String>, String> {
    let Some(bytes) = secrets::read_secret(CURSEFORGE_KEY_SECRET) else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    let key = String::from_utf8(bytes).map_err(|_| {
        keyed_msg!(
            "errors.community.curseforgeKeyCorrupt",
            "CurseForge API Key 存储损坏\n请在设置中重新填写"
        )
    })?;
    let key = key.trim();
    validate_curseforge_key(key)?;
    Ok(Some(key.to_string()))
}

/// 用户 key 优先，其次构建期内置 key。
pub(crate) fn api_key() -> Result<String, String> {
    if let Some(key) = read_user_api_key()? {
        return Ok(key);
    }
    if let Some(key) = constants::CURSEFORGE_API_KEY {
        return Ok(key.to_string());
    }
    Err(keyed_msg!(
        "errors.community.curseforgeKeyMissing",
        "未配置 CurseForge API Key\n请在「设置 → 下载 → CurseForge Key」填写，或在项目根 .env 配置 ELYTRA_CURSEFORGE_API_KEY"
    ))
}

pub fn curseforge_key_source() -> CurseForgeKeySource {
    if secrets::read_secret(CURSEFORGE_KEY_SECRET).is_some_and(|v| !v.is_empty()) {
        CurseForgeKeySource::User
    } else if constants::CURSEFORGE_API_KEY.is_some() {
        CurseForgeKeySource::BuiltIn
    } else {
        CurseForgeKeySource::None
    }
}

pub fn set_curseforge_key(key: &str) -> Result<(), String> {
    let key = key.trim();
    validate_curseforge_key(key)?;
    secrets::write_secret(CURSEFORGE_KEY_SECRET, key.as_bytes())
}

pub fn clear_curseforge_key() -> Result<(), String> {
    secrets::clear_secret(CURSEFORGE_KEY_SECRET)
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
        "worlds" => Some(17),
        _ => None,
    }
}

/// CF 项目 URL 段：worlds 分类用 /worlds/，其余（mod 等）用 /mc-mods/。
fn curseforge_url_segment(category: Option<&str>) -> &'static str {
    if category == Some("worlds") {
        "worlds"
    } else {
        "mc-mods"
    }
}

/// 从 gameVersions（如 "NeoForge"）提取已知 loader 名。
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

/// 从 latestFilesIndexes 提取 (loaders, 版本范围)：modLoader 数字映射已知 loader，gameVersion 取 min ~ max。
fn curseforge_support_fields(data: &serde_json::Value) -> (Vec<String>, Option<String>) {
    let mut loaders: Vec<String> = Vec::new();
    let mut versions: Vec<String> = Vec::new();
    if let Some(arr) = data["latestFilesIndexes"].as_array() {
        for idx in arr {
            if let Some(ml) = idx["modLoader"].as_u64() {
                let name = match ml {
                    1 => "forge",
                    4 => "fabric",
                    5 => "quilt",
                    6 => "neoforge",
                    _ => "",
                };
                if !name.is_empty() && !loaders.contains(&name.to_string()) {
                    loaders.push(name.to_string());
                }
            }
            if let Some(gv) = idx["gameVersion"].as_str() {
                if !gv.is_empty() {
                    versions.push(gv.to_string());
                }
            }
        }
    }
    (loaders, version_range(&versions))
}

/// 带 x-api-key 发 GET 并解析 JSON；key 未配置直接报错（不 panic、不暴露 key）。pub(crate)：optifine.rs 的 OptiFabric 安装复用。
pub(crate) async fn curseforge_get(
    path: &str,
    params: &[(&str, String)],
) -> Result<serde_json::Value, String> {
    let key = api_key()?;
    let mut all: Vec<(&str, String)> = vec![("gameId", CURSEFORGE_GAME_ID.to_string())];
    all.extend_from_slice(params);
    let urls: Vec<String> = api_urls(ApiPlatform::CurseForge, path, &all)?;
    get_json_multi(&urls, |u| {
        http().get(u.to_string()).header("x-api-key", key.as_str())
    })
    .await
}

/// 批量换链超时：数百个 fileID 一次请求，服务端聚合查询略慢于单查。
const CF_BATCH_TIMEOUT_SECS: u64 = 30;

/// CurseForge 批量换链 `POST /v1/mods/files` → data 数组（id/fileName/downloadUrl/hashes/fileLength）；整合包清单只有 (projectID, fileID) 数字引用，必须经此换直链与 sha1。
pub(crate) async fn curseforge_files(file_ids: &[i64]) -> Result<Vec<serde_json::Value>, String> {
    let key = api_key()?;
    let body = serde_json::json!({ "fileIds": file_ids });
    let urls = api_routes(ApiPlatform::CurseForge, "/mods/files");
    let json = post_json_multi(
        &urls,
        &body,
        |u| http().post(u).header("x-api-key", key.as_str()),
        CF_BATCH_TIMEOUT_SECS,
        CF_BATCH_TIMEOUT_SECS,
    )
    .await?;
    Ok(json["data"].as_array().cloned().unwrap_or_default())
}

/// 指纹精确匹配结果：本地文件指纹 → CF 项目/文件。
#[derive(Debug, Clone, Copy)]
pub(crate) struct CurseForgeFingerprintMatch {
    pub fingerprint: u32,
    pub project_id: i64,
    pub file_id: i64,
}

/// 批量指纹匹配 `POST /v1/fingerprints`；分块请求，返回精确匹配（部分匹配不用于导出引用）。
pub(crate) async fn curseforge_fingerprint_matches(
    fingerprints: &[u32],
) -> Result<Vec<CurseForgeFingerprintMatch>, String> {
    if fingerprints.is_empty() {
        return Ok(Vec::new());
    }
    let key = api_key()?;
    let urls = api_routes(ApiPlatform::CurseForge, "/fingerprints");
    let mut matches = Vec::new();
    for chunk in fingerprints.chunks(500) {
        let body = serde_json::json!({ "fingerprints": chunk });
        let json = post_json_multi(
            &urls,
            &body,
            |u| http().post(u).header("x-api-key", key.as_str()),
            CF_BATCH_TIMEOUT_SECS,
            CF_BATCH_TIMEOUT_SECS,
        )
        .await?;
        matches.extend(parse_fingerprint_matches(&json));
    }
    Ok(matches)
}

/// 从指纹响应解析 `exactMatches`（`file.modId` = projectID，`file.id` = fileID）。
pub(crate) fn parse_fingerprint_matches(
    json: &serde_json::Value,
) -> Vec<CurseForgeFingerprintMatch> {
    json["data"]["exactMatches"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let file = &item["file"];
            Some(CurseForgeFingerprintMatch {
                fingerprint: file["fileFingerprint"].as_u64()? as u32,
                project_id: file["modId"].as_i64()?,
                file_id: file["id"].as_i64()?,
            })
        })
        .collect()
}

pub(super) async fn search_curseforge(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u64,
    page_size: u32,
) -> Result<CommunitySearchPage, String> {
    let mut params: Vec<(&str, String)> = vec![
        ("pageSize", page_size.to_string()),
        ("index", offset.to_string()),
    ];
    // 空关键字不传 searchFilter（API 无热门概念，返回按名称序列表——API 限制）
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
    // 不传 classId 会混入全部类型（整合包/材质包/光影），必须显式过滤：普通 mod（category None）默认 Minecraft Mods(classId=6)，分类按映射，未知类别回落 6
    let class_id = match category {
        None => 6,
        Some(cat) => curseforge_class_id(cat).unwrap_or(6),
    };
    params.push(("classId", class_id.to_string()));
    let data = curseforge_get("/mods/search", &params).await?;

    let arr = data["data"].as_array().ok_or_else(|| {
        keyed_msg!(
            "errors.community.curseforgeSearchDataMissing",
            "搜索响应缺少 data"
        )
    })?;
    let items: Vec<CommunitySearchResult> = arr
        .iter()
        .filter_map(|m| {
            let (loaders, version_range) = curseforge_support_fields(m);
            let title = m["name"].as_str()?.to_string();
            let entry = wiki_entry::WIKI.lookup("curseforge", m["slug"].as_str().unwrap_or(""));
            let title_zh = entry
                .and_then(|e| e.chinese_raw.as_deref())
                .and_then(wiki_entry::chinese_name);
            Some(CommunitySearchResult {
                id: m["id"].as_u64()?.to_string(),
                title,
                title_zh,
                mcmod_id: entry.map(|e| e.mcmod_id),
                description: m["summary"].as_str().unwrap_or("").to_string(),
                translated: None,
                loaders,
                game_version_range: version_range,
                author: m["authors"]
                    .as_array()?
                    .first()?
                    .get("name")?
                    .as_str()?
                    .to_string(),
                downloads: m["downloadCount"].as_u64().unwrap_or(0),
                icon_url: m["logo"]["url"].as_str().map(str::to_string),
                // 官网 URL 以 API websiteUrl 为准（分类段随 classId 变化，自拼 mc-mods 会让整合包等 404）；缺失时按已知分类段兜底拼接
                page_url: m["links"]["websiteUrl"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "https://www.curseforge.com/minecraft/{}/{}",
                            curseforge_url_segment(category),
                            m["slug"].as_str().unwrap_or("")
                        )
                    }),
                category: category.map(str::to_string),
            })
        })
        .collect();
    let total = data["pagination"]["totalCount"]
        .as_u64()
        .unwrap_or(items.len() as u64);
    Ok(CommunitySearchPage { items, total })
}

pub(super) async fn curseforge_versions(
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<CommunityVersion>, String> {
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

    let arr = data["data"].as_array().ok_or_else(|| {
        keyed_msg!(
            "errors.community.curseforgeFilesDataMissing",
            "文件响应缺少 data"
        )
    })?;
    Ok(arr
        .iter()
        .filter_map(|f| {
            let game_versions: Vec<String> = f["gameVersions"]
                .as_array()?
                .iter()
                .filter_map(|g| g.as_str().map(str::to_string))
                .collect();
            let loaders = curseforge_loaders(&game_versions);
            let sha1 = f["hashes"]
                .as_array()?
                .iter()
                .find(|h| h["algo"].as_u64() == Some(1))?;
            // CurseForge releaseType：1=Release、2=Beta、3=Alpha（归一化）
            let version_type = match f["releaseType"].as_u64() {
                Some(1) => "release",
                Some(2) => "beta",
                Some(3) => "alpha",
                _ => "unknown",
            };
            Some(CommunityVersion {
                version_id: f["id"].as_u64()?.to_string(),
                version_number: f["fileName"].as_str()?.to_string(),
                game_versions,
                loaders,
                filename: f["fileName"].as_str()?.to_string(),
                download_url: f["downloadUrl"].as_str().unwrap_or("").to_string(),
                file_size: f["fileLength"].as_u64().unwrap_or(0),
                sha1: sha1
                    .get("value")
                    .and_then(|s| s.as_str())
                    .map(str::to_string),
                date_published: f["fileDate"].as_str().unwrap_or("").to_string(),
                downloads: f["downloadCount"].as_u64().unwrap_or(0),
                version_type: version_type.to_string(),
                dependencies: f["dependencies"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|d| {
                                // CurseForge relationType：1=embedded、2=optional、3=required（归一化）
                                let relation = match d["relationType"].as_u64() {
                                    Some(1) => "embedded",
                                    Some(2) => "optional",
                                    Some(3) => "required",
                                    _ => "unknown",
                                };
                                Some(CommunityDependency {
                                    project_id: d["modId"].as_u64()?.to_string(),
                                    relation: relation.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect())
}

pub(super) async fn curseforge_details(
    ids: &[String],
) -> Result<Vec<CommunityDependencyItem>, String> {
    let joined = ids.join(",");
    let data = curseforge_get("/mods", &[("modIds", joined)]).await?;
    let arr = data["data"].as_array().ok_or_else(|| {
        keyed_msg!(
            "errors.community.curseforgeDetailsDataMissing",
            "项目详情响应缺少 data"
        )
    })?;
    Ok(arr
        .iter()
        .filter_map(|m| {
            Some(CommunityDependencyItem {
                id: m["id"].as_u64()?.to_string(),
                title: m["name"].as_str()?.to_string(),
                description: m["summary"].as_str().unwrap_or("").to_string(),
                icon_url: m["logo"]["url"].as_str().map(str::to_string),
                translated: None,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CurseForge API 对并发请求敏感（并发测试会偶发连接失败），串行化。
    static CF_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[test]
    fn curseforge_key_format_validation() {
        let valid = format!("$2a$10${}", "a".repeat(53));
        assert_eq!(valid.len(), 60);
        assert!(validate_curseforge_key(&valid).is_ok());
        assert!(validate_curseforge_key("").is_err());
        assert!(validate_curseforge_key("$2a$10$short").is_err());
        assert!(validate_curseforge_key(&format!("$2b$10${}", "a".repeat(53))).is_err());
    }

    #[test]
    fn user_key_overrides_builtin_and_clears() {
        let before = curseforge_key_source();
        clear_curseforge_key().expect("清理应成功");
        let valid = format!("$2a$10${}", "b".repeat(53));
        set_curseforge_key(&valid).expect("写入应成功");
        assert_eq!(curseforge_key_source(), CurseForgeKeySource::User);
        assert_eq!(super::api_key().expect("用户 key 应可用"), valid);
        clear_curseforge_key().expect("清除应成功");
        assert_eq!(curseforge_key_source(), before);
    }

    /// CF 需 key：无 key 的环境跳过，有用户或内置 key 时走真实请求。
    #[tokio::test]
    async fn search_curseforge_works() {
        let _g = CF_LOCK.lock().await;
        if curseforge_key_source() == CurseForgeKeySource::None {
            eprintln!("跳过: 未配置 CurseForge API Key");
            return;
        }
        let page = search_curseforge("jei", Some("1.21.1"), None, None, 0, 30).await;
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

    /// CF 搜索不匹配名称括号内缩写（"JEI" 搜不到）；搜全名命中 JEI（id 238222）。
    #[tokio::test]
    async fn curseforge_full_name_finds_jei() {
        let _g = CF_LOCK.lock().await;
        if curseforge_key_source() == CurseForgeKeySource::None {
            eprintln!("跳过: 未配置 CurseForge API Key");
            return;
        }
        let page = search_curseforge("just enough items", Some("1.21.1"), None, None, 0, 30)
            .await
            .unwrap();
        assert!(
            page.items
                .iter()
                .any(|h| h.id == "238222" || h.title.contains("Just Enough Items (JEI)")),
            "搜全名应命中 JEI"
        );
    }

    #[test]
    fn curseforge_class_id_maps_worlds() {
        assert_eq!(curseforge_class_id("worlds"), Some(17));
        assert_eq!(curseforge_class_id("shader"), Some(6552), "既有分类不回归");
        assert_eq!(curseforge_class_id("unknown"), None);
    }

    #[test]
    fn curseforge_url_segment_selects_category() {
        assert_eq!(curseforge_url_segment(Some("worlds")), "worlds");
        assert_eq!(curseforge_url_segment(Some("mod")), "mc-mods");
        assert_eq!(curseforge_url_segment(None), "mc-mods");
    }

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

    #[test]
    fn curseforge_support_fields_maps_loader_ids() {
        let data = serde_json::json!({
            "latestFilesIndexes": [
                { "gameVersion": "1.20.1", "modLoader": 4 },
                { "gameVersion": "1.20.1", "modLoader": 4 },
                { "gameVersion": "1.19.2", "modLoader": 1 },
                { "gameVersion": "1.19.2", "modLoader": 6 },
                { "gameVersion": "1.21.1", "modLoader": 999 },
                { "gameVersion": "1.17.1", "modLoader": 5 }
            ]
        });
        let (loaders, range) = curseforge_support_fields(&data);
        assert_eq!(
            loaders,
            vec!["fabric", "forge", "neoforge", "quilt"],
            "数字映射去重"
        );
        assert_eq!(
            range,
            Some("1.17.1 ~ 1.21.1".to_string()),
            "版本取 min ~ max"
        );
        assert_eq!(
            curseforge_support_fields(&serde_json::json!({})),
            (Vec::<String>::new(), None)
        );
    }
}
