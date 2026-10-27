use std::collections::HashMap;

use crate::game::community;

/// 搜索 Mod / 光影包等（按 `source` 分派 modrinth / curseforge；`query.category` 为项目类型过滤，`page_size` 为每源条数；`translate` 默认开启）。
#[tauri::command]
pub async fn search_community(
    source: String,
    query: community::SearchQuery,
    translate: Option<bool>,
) -> Result<community::CommunitySearchPage, String> {
    community::search_community(&source, &query, translate.unwrap_or(true)).await
}

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

#[tauri::command]
pub async fn get_community_details(
    source: String,
    ids: Vec<String>,
    translate: Option<bool>,
) -> Result<Vec<community::CommunityDependencyItem>, String> {
    community::get_community_details(&source, &ids, translate.unwrap_or(true)).await
}

/// 解析主 Mod 的前置依赖（required + optional），按目标实例的 MC/loader 选兼容版本。
#[tauri::command]
pub async fn resolve_mod_dependencies(
    source: String,
    project_id: String,
    version_id: String,
    version_dir_name: String,
    version_overrides: Option<HashMap<String, String>>,
) -> Result<community::ModDependencyPlan, String> {
    let vdir = crate::game::version::dir::version_dir_path(&version_dir_name)?;
    let (mc_version, kind, _) =
        crate::game::modpack::resolve_version_meta(&vdir, &version_dir_name);
    if kind == "vanilla" || kind == "optifine" {
        bail_key!(
            "errors.community.instanceNoLoader",
            "目标实例没有可用的 ModLoader\n请先在版本设置中安装 Fabric / Forge / NeoForge / Quilt"
        );
    }
    community::resolve_mod_dependencies(
        &source,
        &project_id,
        &version_id,
        &mc_version,
        &kind,
        &version_overrides.unwrap_or_default(),
        &vdir.join("mods"),
    )
    .await
}

#[tauri::command]
pub fn get_curseforge_key_source() -> community::CurseForgeKeySource {
    community::curseforge_key_source()
}

#[tauri::command]
pub fn set_curseforge_key(key: String) -> Result<(), String> {
    community::set_curseforge_key(&key)
}

#[tauri::command]
pub fn clear_curseforge_key() -> Result<(), String> {
    community::clear_curseforge_key()
}
