//! 社区资源（Mod/光影/资源包等）搜索 / 版本查询命令（薄胶水，业务在 downloader/community.rs）。
//! 注意：同名字段/结构从 downloader::community 重导出？——不重导出，命令直接引用全限定路径。

use crate::game::community;

/// 搜索 Mod / 光影包等（按来源分派：modrinth / curseforge；query 内 category 为项目类型过滤、
/// page_size 每源条数，聚合模式前端传减半值，默认 30；translate 默认开启中文简介翻译）。
/// 前端 invoke 传命名对象 `{ source, query: { query, gameVersion, loader, category, offset, pageSize }, translate }`。
#[tauri::command]
pub async fn search_community(
    source: String,
    query: community::SearchQuery,
    translate: Option<bool>,
) -> Result<community::CommunitySearchPage, String> {
    community::search_community(&source, &query, translate.unwrap_or(true)).await
}

/// 查询 Mod 版本列表。
#[tauri::command]
pub async fn get_community_versions(
    source: String,
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<community::CommunityVersion>, String> {
    community::get_community_versions(
        &source,
        &project_id,
        game_version.as_deref(),
        loader.as_deref(),
    )
    .await
}

/// 批量查询项目详情（前置依赖卡片数据源；translate 默认开启中文简介翻译）。
#[tauri::command]
pub async fn get_community_details(
    source: String,
    ids: Vec<String>,
    translate: Option<bool>,
) -> Result<Vec<community::CommunityDependencyItem>, String> {
    community::get_community_details(&source, &ids, translate.unwrap_or(true)).await
}
