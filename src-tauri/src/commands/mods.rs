//! Mod 搜索 / 版本查询命令（薄胶水，业务在 downloader/mods.rs）。
//! 注意：同名字段/结构从 downloader::mods 重导出？——不重导出，命令直接引用全限定路径。

use crate::downloader::mods as mods_domain;

/// 搜索 Mod / 光影包等（按来源分派：modrinth / curseforge；category 为项目类型过滤）。
#[tauri::command]
pub async fn search_mods(
    source: String,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    category: Option<String>,
    offset: Option<u64>,
) -> Result<mods_domain::ModSearchPage, String> {
    mods_domain::search_mods(
        &source,
        query.trim(),
        game_version.as_deref(),
        loader.as_deref(),
        category.as_deref(),
        offset.unwrap_or(0),
    )
    .await
}

/// 查询 Mod 版本列表。
#[tauri::command]
pub async fn get_mod_versions(
    source: String,
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<mods_domain::ModVersion>, String> {
    mods_domain::get_mod_versions(
        &source,
        &project_id,
        game_version.as_deref(),
        loader.as_deref(),
    )
    .await
}
