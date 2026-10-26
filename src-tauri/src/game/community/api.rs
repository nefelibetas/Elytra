//! 社区资源对外 API：公共出参类型 + 按来源分派入口（实现分别在 curseforge / modrinth）。

use serde::Serialize;

use super::shared::post_json_multi;
use super::translation::fetch_translations;
use super::{curseforge, modrinth, wiki_entry};
use crate::downloader::endpoints::CommunityEndpoints;
use crate::downloader::fetcher;

#[derive(Debug, Clone, Serialize)]
pub struct CommunitySearchResult {
    pub id: String,
    pub title: String,
    /// MC百科内置表的中文译名（未收录为 None）
    pub title_zh: Option<String>,
    /// MC百科条目 id（内置表 slug 匹配；未收录为 None）→ https://www.mcmod.cn/class/{id}.html
    pub mcmod_id: Option<u32>,
    pub description: String,
    /// MCIM 中文简介翻译（无/失败为 None；仅开启翻译时填充）
    pub translated: Option<String>,
    /// 支持的 ModLoader（fabric/forge/neoforge/quilt，去重保序；未知/无则为空）
    pub loaders: Vec<String>,
    /// 支持的 MC 版本范围（"最低 ~ 最高"，单版本显示单个；无法解析为 None）
    pub game_version_range: Option<String>,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub page_url: String,
    /// 所属分类（由查询条件透传；普通 mod 为 None）。供详情页按类别裁剪操作。
    pub category: Option<String>,
}

/// 搜索分页结果（Modrinth offset / CurseForge index，统一透传页码起点）。
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
    /// 精确版本提示（Modrinth `version_id`；CurseForge 依赖无此字段）
    #[serde(default)]
    pub version_id: Option<String>,
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
    /// 版本类型（两源归一）：release | beta | alpha | unknown
    pub version_type: String,
    /// 该版本文件的依赖列表（Modrinth dependencies / CurseForge dependencies 归一）
    pub dependencies: Vec<CommunityDependency>,
}

/// 依赖项目详情（图标/标题/简介），与所属项目同源查询（MR project_id / CF modId）。
#[derive(Debug, Clone, Serialize)]
pub struct CommunityDependencyItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    /// MCIM 中文简介翻译（translate=false 或未命中时为 None，UI 降级显示原文）
    pub translated: Option<String>,
}

/// 社区资源搜索条件（前端表单 + 分页）。刻意 `rename_all = "camelCase"`：作 Tauri 命令 `search_community` 入参，
/// 字段名即前端 invoke 的 JS 键（`gameVersion`/`pageSize`），改回 snake_case 破坏 IPC 契约。
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

/// 搜索（按来源分派）。`page_size` 每源条数（聚合模式前端传减半值，单源默认 30）；`translate` 批量查 MCIM 简介（失败静默降级）。
/// 中文查询先经内置百科表反查英文词：两平台只认英文，直发中文只会得到按热度排的无关结果。
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
        other => bail_key!("errors.community.unknownSource", "未知 Mod 来源\n{other}"),
    };
    if translate {
        // 标题译名/mcmod_id 已由内置百科表填充（无网络开销），此处仅补 MCIM 简介翻译
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

pub async fn get_community_versions(
    source: &str,
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<CommunityVersion>, String> {
    match source {
        "modrinth" => modrinth::modrinth_versions(project_id, game_version, loader).await,
        "curseforge" => curseforge::curseforge_versions(project_id, game_version, loader).await,
        other => Err(keyed_msg!(
            "errors.community.unknownSource",
            "未知 Mod 来源\n{other}"
        )),
    }
}

/// 批量查询项目详情（按来源分派）。空 ids 直接返回空列表；单条失败跳过；`translate` 批量查 MCIM 简介（失败静默降级）。
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
        other => bail_key!("errors.community.unknownSource", "未知 Mod 来源\n{other}"),
    };
    if translate {
        let tr = fetch_translations(source, ids).await;
        for it in &mut items {
            it.translated = tr.get(&it.id).cloned();
        }
    }
    Ok(items)
}

/// 按 sha1 批量查 Modrinth 文件（POST /v2/version_files，mrpack 导出 files[] 用）；响应 `{sha1:{files:[{url,hashes:{sha1,sha512},size}]}}`；官方源优先、失败降级 MCIM 镜像、全失败返回 Err（调用方改全量打包）。
pub async fn modrinth_version_files(hashes: &[String]) -> Result<serde_json::Value, String> {
    if hashes.is_empty() {
        return Ok(serde_json::json!({}));
    }
    let body = serde_json::json!({"hashes": hashes, "algorithm": "sha1"});
    let urls = [
        format!("{}/v2/version_files", CommunityEndpoints::MODRINTH_API_BASE),
        format!(
            "{}/v2/version_files",
            CommunityEndpoints::MODRINTH_MIRROR_BASE
        ),
    ];
    post_json_multi(&urls, &body, |u| fetcher::http().post(u), 5, 10).await
}
