//! 社区资源的对外 API：公共出参类型定义 + 按来源分派入口。
//! 平台实现分别在 `curseforge` / `modrinth`，本文件只做归一与分发。

use serde::Serialize;

use super::shared::post_json_multi;
use super::{curseforge, modrinth, wiki_entry};
use super::translation::fetch_translations;
use crate::downloader::endpoints::CommunityEndpoints;
use crate::downloader::fetcher;

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

/// 社区资源搜索条件（统一入参：前端表单 + 分页）。
/// 命令层直接收该结构体（前端 invoke 传命名对象，字段 camelCase），
/// 领域层解包分派给两平台实现。
/// 例外说明：本结构体刻意 `rename_all = "camelCase"`，与全仓「字段全 snake_case」相反——
/// 它是 Tauri 命令 `search_community` 的入参，字段名即前端 invoke 传入 JS 对象的键
/// （前端固定写 `gameVersion` / `pageSize`），改回 snake_case 会破坏 IPC 契约，故保留。
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
///
/// 中文查询会先经内置百科表反查英文查询词再搜：Modrinth/CurseForge 都只认英文，
/// 原样把中文发过去只会得到「按热度排的无关结果」（实测搜「实体」返回 3D Maps）。
pub async fn search_community(
    source: &str,
    q: &SearchQuery,
    translate: bool,
) -> Result<CommunitySearchPage, String> {
    let raw = q.query.trim();
    // 反查失败（非中文/未命中）时保持原样交给平台，不额外报错
    let query = wiki_entry::WIKI
        .query_for_chinese(source, raw)
        .unwrap_or_else(|| raw.to_string());
    let mut page = match source {
        "modrinth" => {
            modrinth::search_modrinth(
                &query,
                q.game_version.as_deref(),
                q.loader.as_deref(),
                q.category.as_deref(),
                q.offset,
                q.page_size,
            )
            .await?
        }
        "curseforge" => {
            curseforge::search_curseforge(
                &query,
                q.game_version.as_deref(),
                q.loader.as_deref(),
                q.category.as_deref(),
                q.offset,
                q.page_size,
            )
            .await?
        }
        other => bail!("未知 Mod 来源\n{other}"),
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
        "modrinth" => modrinth::modrinth_versions(project_id, game_version, loader).await,
        "curseforge" => curseforge::curseforge_versions(project_id, game_version, loader).await,
        other => Err(format!("未知 Mod 来源\n{other}")),
    }
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
        "modrinth" => modrinth::modrinth_details(ids).await?,
        "curseforge" => curseforge::curseforge_details(ids).await?,
        other => bail!("未知 Mod 来源\n{other}"),
    };
    if translate {
        let tr = fetch_translations(source, ids).await;
        for it in &mut items {
            it.translated = tr.get(&it.id).cloned();
        }
    }
    Ok(items)
}

/// 按 sha1 批量查询 Modrinth 文件（POST /v2/version_files，mrpack 导出 files[] 用）。
/// 响应：`{sha1: {files: [{url, hashes:{sha1,sha512}, size}]}}`；官方源优先，失败降级 MCIM 镜像。
/// 全部失败返回 Err（调用方降级为全量物理打包）。
pub async fn modrinth_version_files(hashes: &[String]) -> Result<serde_json::Value, String> {
    if hashes.is_empty() {
        return Ok(serde_json::json!({}));
    }
    let body = serde_json::json!({"hashes": hashes, "algorithm": "sha1"});
    let urls = [
        format!("{}/v2/version_files", CommunityEndpoints::MODRINTH_API_BASE),
        format!("{}/v2/version_files", CommunityEndpoints::MODRINTH_MIRROR_BASE),
    ];
    post_json_multi(&urls, &body, |u| fetcher::http().post(u), 5, 10).await
}
