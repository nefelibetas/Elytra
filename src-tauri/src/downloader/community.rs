//! 社区资源搜索 / 版本查询（Modrinth 官方 API + CurseForge 官方 API），
//! 覆盖 Mod / 光影 / 资源包 / 数据包 / 整合包 / 世界等类别（category 参数区分）。
//! 两个平台 API 完全不同（ID 体系、鉴权、返回结构），分开实现、统一出参。
//! CurseForge 需要 x-api-key（constants.rs 经 .env 注入）；Modrinth 无需 key。
//! 搜索结果的简介翻译由 MCIM 镜像提供（英文原文 → 中文简介），失败静默降级为英文。

use std::collections::HashMap;

use serde::Serialize;

use crate::constants;
use crate::downloader::fetcher;

const MODRINTH_API: &str = "https://api.modrinth.com";
const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
const CURSEFORGE_GAME_ID: &str = "432";
/// MCIM 镜像站（Modrinth/CurseForge API 与翻译数据镜像）
const MCIM_API: &str = "https://mod.mcimirror.top";
/// 搜索类交互请求超时（默认 client 300s 太长，搜索卡住不该让前端挂 5 分钟）
const QUERY_TIMEOUT_SECS: u64 = 10;
/// 翻译缓存 TTL（10 分钟，低频变化数据，对齐 downloader/source.rs 缓存模式）
const TRANSLATION_CACHE_TTL_SECS: u64 = 600;

/// 翻译缓存：id → (查询时间, 中文简介)。模块级 Mutex 串行化；防翻页/重复搜索反复请求。
static TRANSLATION_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<HashMap<String, (std::time::Instant, String)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
pub struct CommunitySearchResult {
    pub id: String,
    pub title: String,
    /// MC百科内置条目表提供的中文译名（英文标题 → 中文名；未收录为 None）
    pub title_zh: Option<String>,
    /// MC百科条目 id（内置表 slug 匹配；未收录为 None）→ https://www.mcmod.cn/class/{id}.html
    pub mcmod_id: Option<u32>,
    pub description: String,
    /// MCIM 中文简介翻译（无翻译/翻译失败为 None；仅社区搜索开启翻译时填充）
    pub translated: Option<String>,
    /// 支持的 ModLoader（fabric/forge/neoforge/quilt，去重保序；未知/无则为空）
    pub loaders: Vec<String>,
    /// 支持的 MC 游戏版本范围（"最低 ~ 最高"，如 "1.17.1 ~ 1.21.1"；
    /// 仅一个有效版本时显示单个；无版本/无法解析为 None）
    pub game_version_range: Option<String>,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub page_url: String,
    /// 搜索结果所属分类（shader/worlds 等，由查询条件透传；普通 mod 搜索为 None）。
    /// 供详情页按类别裁剪操作（如光影/地图不显示 MC百科链接）。
    pub category: Option<String>,
}

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

/// 解析 MC 版本号为数值段（"1.21.1" → [1,21,1]）；含非数字段（snapshot 如 "24w14a"）返回 None。
fn parse_game_version(v: &str) -> Option<Vec<u64>> {
    v.split('.').map(|seg| seg.parse::<u64>().ok()).collect()
}

/// 数值段逐段比较；段数不足按 0 补足（"1.20" 与 "1.20.0" 相等）；任一侧无法解析返回 None。
fn compare_game_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let pa = parse_game_version(a)?;
    let pb = parse_game_version(b)?;
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return Some(x.cmp(&y));
        }
    }
    Some(std::cmp::Ordering::Equal)
}

/// 版本列表 → 范围字符串：仅一个有效版本显示单个，多个显示 "最低 ~ 最高"；无有效版本返回 None。
fn version_range(versions: &[String]) -> Option<String> {
    let valid: Vec<&String> = versions
        .iter()
        .filter(|v| parse_game_version(v).is_some())
        .collect();
    if valid.is_empty() {
        return None;
    }
    let min = valid
        .iter()
        .min_by(|a, b| compare_game_versions(a, b).unwrap())
        .copied()?;
    let max = valid
        .iter()
        .max_by(|a, b| compare_game_versions(a, b).unwrap())
        .copied()?;
    if min == max {
        Some(min.clone())
    } else {
        Some(format!("{} ~ {}", min, max))
    }
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

/// 从 CurseForge latestFilesIndexes 提取 (loaders, game_version_range)：modLoader 数字映射
/// 已知 loader；gameVersion 全量收集后取 min ~ max 范围。
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

/// 搜索分页结果（两平台分页语义不同：Modrinth offset / CurseForge index，统一透传页码起点）。
#[derive(Debug, Clone, Serialize)]
pub struct CommunitySearchPage {
    pub items: Vec<CommunitySearchResult>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunityDependency {
    /// 依赖项目 ID（Modrinth project_id / CurseForge modId，统一转字符串）
    pub project_id: String,
    /// 关系类型（两源归一）：required | optional | incompatible | embedded
    pub relation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommunityVersion {
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
    /// 版本类型（Modrinth version_type / CurseForge releaseType 归一）：
    /// release | beta | alpha | unknown
    pub version_type: String,
    /// 该版本文件的依赖列表（Modrinth dependencies / CurseForge dependencies 归一）
    pub dependencies: Vec<CommunityDependency>,
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
        .map_err(|e| format!("请求失败\n{}", e))?;
    if !resp.status().is_success() {
        return Err(format!("请求失败\nHTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败\n{}", e))
}

/// 解析 MCIM 翻译响应（两源字段名不同：modrinth 用 project_id 字符串、curseforge 用 modid 数字），
/// 返回 id → 中文简介；空简介条目跳过。
fn parse_translations(source: &str, data: &serde_json::Value) -> HashMap<String, String> {
    let Some(arr) = data.as_array() else {
        return HashMap::new();
    };
    let key = if source == "modrinth" {
        "project_id"
    } else {
        "modid"
    };
    arr.iter()
        .filter_map(|v| {
            let id = v[key]
                .as_str()
                .map(str::to_string)
                .or_else(|| v[key].as_u64().map(|n| n.to_string()))?;
            let translated = v["translated"].as_str()?.trim();
            if translated.is_empty() {
                return None;
            }
            Some((id, translated.to_string()))
        })
        .collect()
}

/// 构造 MCIM 翻译批量请求体。
/// 注意：两源字段与类型不同——modrinth 的 `project_ids` 是字符串数组；
/// curseforge 的 `modids` 要求**整数数组**（字符串数组会被 MCIM 400 拒绝，
/// 曾导致 CF 侧全部简介翻译静默丢失）。非数字 id 防御性过滤，全过滤返回 None（跳过请求）。
fn translation_body(source: &str, ids: &[String]) -> Option<serde_json::Value> {
    if source == "modrinth" {
        if ids.is_empty() {
            return None;
        }
        return Some(serde_json::json!({ "project_ids": ids }));
    }
    let modids: Vec<i64> = ids.iter().filter_map(|s| s.parse().ok()).collect();
    if modids.is_empty() {
        return None;
    }
    Some(serde_json::json!({ "modids": modids }))
}

/// 批量查 MCIM 中文简介翻译（POST /translate/{source}）。先查内存缓存（10 分钟 TTL），
/// 只请求未命中的 id；网络失败/无翻译静默降级（返回空 map，不阻塞搜索结果）。
/// pub(crate)：本地 Mod 详情（mod_updates.rs）复用同一翻译通道。
pub(crate) async fn fetch_translations(source: &str, ids: &[String]) -> HashMap<String, String> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let now = std::time::Instant::now();
    let mut result: HashMap<String, String> = HashMap::new();
    let missed: Vec<String> = {
        let cache = TRANSLATION_CACHE.lock();
        ids.iter()
            .filter(|id| match cache.get(*id) {
                Some((at, text))
                    if now.duration_since(*at).as_secs() < TRANSLATION_CACHE_TTL_SECS =>
                {
                    result.insert((*id).clone(), text.clone());
                    false
                }
                _ => true,
            })
            .cloned()
            .collect()
    };
    if missed.is_empty() {
        return result;
    }
    let Some(body) = translation_body(source, &missed) else {
        return result;
    };
    let resp = fetcher::http()
        .post(format!("{}/translate/{}", MCIM_API, source))
        .json(&body)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await;
    let data = match resp {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(v) => v,
            Err(_) => return result,
        },
        _ => return result,
    };

    let mut cache = TRANSLATION_CACHE.lock();
    for (id, text) in parse_translations(source, &data) {
        cache.insert(id.clone(), (now, text.clone()));
        result.insert(id, text);
    }
    result
}

// ── Modrinth（无需 key）──

async fn search_modrinth(
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
            serde_json::to_string(&facets).map_err(|e| format!("序列化数据失败\n{e}"))?;
        params.push(("facets", facets_json));
    }
    // 空关键字时按下载量排（对齐 PCL2：不输入名称直接显示热门 Mod）
    if query.is_empty() {
        params.push(("index", "downloads".into()));
    }
    let url = reqwest::Url::parse_with_params(&format!("{}/v2/search", MODRINTH_API), &params)
        .map_err(|e| format!("URL 构造失败\n{}", e))?;
    let data = get_json(url.as_str()).await?;

    let hits = data["hits"]
        .as_array()
        .ok_or_else(|| "搜索响应缺少 hits".to_string())?;
    let items: Vec<CommunitySearchResult> = hits
        .iter()
        .filter_map(|h| {
            let title = h["title"].as_str()?.to_string();
            let entry = crate::downloader::wiki_entry::WIKI
                .lookup("modrinth", h["slug"].as_str().unwrap_or(""));
            let title_zh = entry
                .and_then(|e| e.chinese_raw.as_deref())
                .and_then(crate::downloader::wiki_entry::chinese_name);
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

async fn modrinth_versions(
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<CommunityVersion>, String> {
    let mut params: Vec<(&str, String)> = Vec::new();
    if let Some(gv) = game_version {
        let arr = serde_json::to_string(&vec![gv]).map_err(|e| format!("序列化数据失败\n{e}"))?;
        params.push(("game_versions", arr));
    }
    if let Some(ld) = loader {
        let arr = serde_json::to_string(&vec![ld]).map_err(|e| format!("序列化数据失败\n{e}"))?;
        params.push(("loaders", arr));
    }
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v2/project/{}/version", MODRINTH_API, project_id),
        &params,
    )
    .map_err(|e| format!("URL 构造失败\n{}", e))?;
    let data = get_json(url.as_str()).await?;

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

// ── CurseForge（需要 x-api-key）──

/// 带 x-api-key 发 GET 并解析 JSON；key 未配置时直接报错（不 panic、不暴露 key）。
/// pub(crate)：OptiFabric 安装（optifine.rs）复用同一查询通道。
pub(crate) async fn curseforge_get(
    path: &str,
    params: &[(&str, String)],
) -> Result<serde_json::Value, String> {
    let key = constants::CURSEFORGE_API_KEY
        .ok_or_else(|| "未配置 CurseForge API Key（项目根 .env 的 ELYTRA_CURSEFORGE_API_KEY），无法使用 CurseForge 来源".to_string())?;
    let mut all: Vec<(&str, String)> = vec![("gameId", CURSEFORGE_GAME_ID.to_string())];
    all.extend_from_slice(params);
    let url = reqwest::Url::parse_with_params(&format!("{}{}", CURSEFORGE_API, path), &all)
        .map_err(|e| format!("URL 构造失败\n{}", e))?;
    let resp = fetcher::http()
        .get(url)
        .header("x-api-key", key)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("请求失败\n{}", e))?;
    if !resp.status().is_success() {
        return Err(format!("请求失败\nHTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析 JSON 失败\n{}", e))
}

async fn search_curseforge(
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
    // CF /mods/search 不传 classId 时返回全部项目类型（整合包/材质包/光影混入），
    // 必须显式过滤：普通 mod 搜索（category None）默认 Minecraft Mods(classId=6)，
    // 分类搜索按已知映射（shader=6552 等），未知类别防御性回落 mod
    let class_id = match category {
        None => 6,
        Some(cat) => curseforge_class_id(cat).unwrap_or(6),
    };
    params.push(("classId", class_id.to_string()));
    let data = curseforge_get("/mods/search", &params).await?;

    let arr = data["data"]
        .as_array()
        .ok_or_else(|| "搜索响应缺少 data".to_string())?;
    let items: Vec<CommunitySearchResult> = arr
        .iter()
        .filter_map(|m| {
            let (loaders, version_range) = curseforge_support_fields(m);
            let title = m["name"].as_str()?.to_string();
            let entry = crate::downloader::wiki_entry::WIKI
                .lookup("curseforge", m["slug"].as_str().unwrap_or(""));
            let title_zh = entry
                .and_then(|e| e.chinese_raw.as_deref())
                .and_then(crate::downloader::wiki_entry::chinese_name);
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
                // 官网 URL 以 API 返回的 websiteUrl 为准（分类段随 classId 变化：
                // mods/modpacks/worlds 等，自行拼接 mc-mods 会让整合包等 404）；
                // 缺失时按已知分类段兜底拼接
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

async fn curseforge_versions(
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

    let arr = data["data"]
        .as_array()
        .ok_or_else(|| "文件响应缺少 data".to_string())?;
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

/// 社区资源搜索条件（统一入参：前端表单 + 分页）。
/// 命令层直接收该结构体（前端 invoke 传命名对象，字段 camelCase），
/// 领域层解包分派给两平台实现。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub query: String,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub category: Option<String>,
    pub offset: u64,
    pub page_size: u32,
}

/// 搜索（按来源分派）。`category` 为项目类型（如 shader）或 None（普通 mod）；
/// `page_size` 每源条数（聚合模式前端传减半值，单源默认 30）；
/// `translate` 为 true 时对结果批量查 MCIM 中文简介（失败静默降级为英文）。
pub async fn search_community(
    source: &str,
    q: &SearchQuery,
    translate: bool,
) -> Result<CommunitySearchPage, String> {
    let mut page = match source {
        "modrinth" => {
            search_modrinth(
                q.query.trim(),
                q.game_version.as_deref(),
                q.loader.as_deref(),
                q.category.as_deref(),
                q.offset,
                q.page_size,
            )
            .await?
        }
        "curseforge" => {
            search_curseforge(
                q.query.trim(),
                q.game_version.as_deref(),
                q.loader.as_deref(),
                q.category.as_deref(),
                q.offset,
                q.page_size,
            )
            .await?
        }
        other => return Err(format!("未知 Mod 来源\n{other}")),
    };
    if translate {
        // 标题译名/mcmod_id：搜索构造时已由内置百科表填充（无网络开销）；
        // 此处仅补 MCIM 简介翻译（失败静默降级）
        if !page.items.is_empty() {
            let ids: Vec<String> = page.items.iter().map(|i| i.id.clone()).collect();
            let tr = fetch_translations(source, &ids).await;
            for item in &mut page.items {
                item.translated = tr.get(&item.id).cloned();
            }
        }
    }
    Ok(page)
}

/// 版本列表（按来源分派）。
pub async fn get_community_versions(
    source: &str,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<CommunityVersion>, String> {
    match source {
        "modrinth" => modrinth_versions(project_id, game_version, loader).await,
        "curseforge" => curseforge_versions(project_id, game_version, loader).await,
        other => Err(format!("未知 Mod 来源\n{other}")),
    }
}

/// 依赖项目详情（图标/标题/简介），供详情页前置依赖卡片展示。
/// 与所属项目同源查询（Modrinth 依赖 project_id / CurseForge 依赖 modId）。
#[derive(Debug, Clone, Serialize)]
pub struct CommunityDependencyItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    /// MCIM 中文简介翻译（translate=false 或未命中时为 None，UI 降级显示原文）
    pub translated: Option<String>,
}

/// 批量查询项目详情（按来源分派）。空 ids 直接返回空列表；单条失败跳过。
/// `translate` 为 true 时对结果批量查 MCIM 中文简介（失败静默降级为英文）。
pub async fn get_community_details(
    source: &str,
    ids: &[String],
    translate: bool,
) -> Result<Vec<CommunityDependencyItem>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut items = match source {
        "modrinth" => modrinth_details(ids).await?,
        "curseforge" => curseforge_details(ids).await?,
        other => return Err(format!("未知 Mod 来源\n{other}")),
    };
    if translate {
        let tr = fetch_translations(source, ids).await;
        for it in &mut items {
            it.translated = tr.get(&it.id).cloned();
        }
    }
    Ok(items)
}

async fn modrinth_details(ids: &[String]) -> Result<Vec<CommunityDependencyItem>, String> {
    let ids_json = serde_json::to_string(ids).map_err(|e| format!("序列化数据失败\n{e}"))?;
    let url = reqwest::Url::parse_with_params(
        &format!("{}/v2/projects", MODRINTH_API),
        &[("ids", ids_json.as_str())],
    )
    .map_err(|e| format!("URL 构造失败\n{}", e))?;
    let data = get_json(url.as_str()).await?;
    let arr = data
        .as_array()
        .ok_or_else(|| "项目详情响应格式错误".to_string())?;
    Ok(arr
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

async fn curseforge_details(ids: &[String]) -> Result<Vec<CommunityDependencyItem>, String> {
    let joined = ids.join(",");
    let data = curseforge_get("/mods", &[("modIds", joined)]).await?;
    let arr = data["data"]
        .as_array()
        .ok_or_else(|| "项目详情响应缺少 data".to_string())?;
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

    /// CurseForge 需 key：无 key 的环境跳过，本机（.env 已配置）走真实请求。
    #[tokio::test]
    async fn search_curseforge_works() {
        let _g = CF_LOCK.lock().await;
        if constants::CURSEFORGE_API_KEY.is_none() {
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

    /// CurseForge 搜索不匹配名称括号内的缩写（"JEI" 在 "Just Enough Items (JEI)" 中搜不到），
    /// 搜全名应命中 JEI（id 238222）。
    #[tokio::test]
    async fn curseforge_full_name_finds_jei() {
        let _g = CF_LOCK.lock().await;
        if constants::CURSEFORGE_API_KEY.is_none() {
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

    /// CurseForge 分类映射：worlds 地图存档 classId=17。
    #[test]
    fn curseforge_class_id_maps_worlds() {
        assert_eq!(curseforge_class_id("worlds"), Some(17));
        assert_eq!(curseforge_class_id("shader"), Some(6552), "既有分类不回归");
        assert_eq!(curseforge_class_id("unknown"), None);
    }

    /// CF 项目 URL 段：worlds 用 /worlds/，其余用 /mc-mods/。
    #[test]
    fn curseforge_url_segment_selects_category() {
        assert_eq!(curseforge_url_segment(Some("worlds")), "worlds");
        assert_eq!(curseforge_url_segment(Some("mod")), "mc-mods");
        assert_eq!(curseforge_url_segment(None), "mc-mods");
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

    /// 翻译响应解析：Modrinth 用 project_id（字符串），CurseForge 用 modid（数字）。
    #[test]
    fn parse_translations_two_sources() {
        let mr = serde_json::json!([
            { "project_id": "AANobbMI", "translated": "高性能渲染引擎替代品", "original": "A high-performance..." },
            { "project_id": "P7dR8mSH", "translated": "轻量级模块化 API", "original": "Lightweight..." }
        ]);
        let m = parse_translations("modrinth", &mr);
        assert_eq!(m.len(), 2, "应解析出 2 条翻译");
        assert_eq!(
            m.get("AANobbMI").map(String::as_str),
            Some("高性能渲染引擎替代品")
        );

        let cf = serde_json::json!([
            { "modid": 238222, "translated": "查看物品和配方", "original": "View Items and Recipes" }
        ]);
        let c = parse_translations("curseforge", &cf);
        assert_eq!(
            c.get("238222").map(String::as_str),
            Some("查看物品和配方"),
            "modid 数字应转字符串"
        );
    }

    /// 翻译响应解析容错：非数组 / 空简介 / 缺字段都跳过，不 panic。
    #[test]
    fn parse_translations_tolerates_bad_input() {
        assert!(
            parse_translations("modrinth", &serde_json::json!({})).is_empty(),
            "非数组返回空"
        );
        let with_empty = serde_json::json!([
            { "project_id": "a", "translated": "" },
            { "project_id": "b" }
        ]);
        assert!(
            parse_translations("modrinth", &with_empty).is_empty(),
            "空简介/缺字段跳过"
        );
        let other_source = serde_json::json!([{ "project_id": "x", "translated": "中文" }]);
        assert!(
            parse_translations("curseforge", &other_source).is_empty(),
            "字段名不匹配返回空"
        );
    }

    /// 翻译请求体：Modrinth 用字符串数组；CurseForge 必须整数数组（字符串数组会被 MCIM 400）。
    #[test]
    fn translation_body_source_formats() {
        let mr = translation_body(
            "modrinth",
            &["AANobbMI".to_string(), "P7dR8mSH".to_string()],
        )
        .unwrap();
        assert_eq!(
            mr,
            serde_json::json!({ "project_ids": ["AANobbMI", "P7dR8mSH"] }),
            "Modrinth 用字符串数组"
        );

        let cf =
            translation_body("curseforge", &["238222".to_string(), "394468".to_string()]).unwrap();
        assert_eq!(
            cf,
            serde_json::json!({ "modids": [238222, 394468] }),
            "CurseForge 必须整数数组，字符串数组会 400"
        );
        assert!(
            cf["modids"].as_array().unwrap().iter().all(|v| v.is_i64()),
            "CurseForge modids 元素必须是整数"
        );
    }

    /// 翻译请求体容错：空 ids / 全非数字 id 返回 None（跳过请求）。
    #[test]
    fn translation_body_tolerates_bad_ids() {
        assert!(translation_body("modrinth", &[]).is_none(), "空 ids 不请求");
        assert!(
            translation_body("curseforge", &[]).is_none(),
            "空 ids 不请求"
        );
        assert!(
            translation_body("curseforge", &["abc".to_string()]).is_none(),
            "全非数字 id 不请求"
        );
        let mixed =
            translation_body("curseforge", &["abc".to_string(), "238222".to_string()]).unwrap();
        assert_eq!(
            mixed,
            serde_json::json!({ "modids": [238222] }),
            "非数字 id 过滤，保留有效数字"
        );
    }

    /// MCIM 翻译 API 真实请求（需网络）：sodium 的 Modrinth 内部 ID 应有中文简介。
    #[tokio::test]
    async fn mcim_translation_api_works() {
        let tr = fetch_translations("modrinth", &["AANobbMI".to_string()]).await;
        assert!(
            !tr.is_empty(),
            "MCIM 翻译应返回 sodium 的中文简介，实际: {:?}",
            tr
        );
    }

    /// MCIM 翻译 API 真实请求（需网络）：CurseForge 侧用字符串 id 应正常返回（回归：曾 400）。
    #[tokio::test]
    async fn mcim_translation_curseforge_works() {
        let tr = fetch_translations("curseforge", &["238222".to_string()]).await;
        assert!(
            !tr.is_empty(),
            "CurseForge 翻译应返回 JEI 的中文简介（字符串 id 需转整数数组），实际: {:?}",
            tr
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

    /// 版本比较：数值段逐段比较，段数不足补 0，非法版本（snapshot）返回 None。
    #[test]
    fn compare_game_versions_numeric_semantics() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_game_versions("1.21.1", "1.20.1"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_game_versions("1.9.4", "1.21.1"),
            Some(Ordering::Less),
            "按数值而非字符串比较"
        );
        assert_eq!(
            compare_game_versions("1.20", "1.20.0"),
            Some(Ordering::Equal),
            "段数不足补 0"
        );
        assert_eq!(
            compare_game_versions("24w14a", "1.21.1"),
            None,
            "snapshot 无法解析"
        );
        assert_eq!(
            compare_game_versions("1.21.1", "1.21.1"),
            Some(Ordering::Equal)
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

    /// CurseForge 支持字段提取：modLoader 数字映射已知 loader，版本取 min ~ max 范围。
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
