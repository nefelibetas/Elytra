use tauri::ipc::Response;

use crate::seedmap;

#[tauri::command]
pub fn get_seed_map_versions() -> Vec<seedmap::SeedMapVersion> {
    seedmap::supported_versions()
}

#[tauri::command]
pub fn resolve_seed_map_version(version: String) -> Result<seedmap::ResolvedVersion, String> {
    seedmap::resolve_version(&version)
}

#[tauri::command]
pub async fn get_seed_map_tile(
    seed: String,
    version: String,
    rx: i32,
    rz: i32,
) -> Result<Response, String> {
    let resolved = seedmap::resolve_version(&version)?;
    let seed = seedmap::parse_seed(&seed)?;
    let cache_path = seedmap::tile_cache_path(&resolved.family, seed, rx, rz);
    if let Some(png) = seedmap::read_tile(&cache_path) {
        return Ok(Response::new(png));
    }
    let family = resolved.family;
    let png = tauri::async_runtime::spawn_blocking(move || {
        let png = seedmap::render_tile(&family, seed, rx, rz)?;
        let _ = seedmap::write_tile_png(&cache_path, &png);
        Ok::<Vec<u8>, String>(png)
    })
    .await
    .map_err(|e| format!("种子地图任务异常\n{e}"))??;
    Ok(Response::new(png))
}

#[tauri::command]
pub fn get_seed_map_block(
    seed: String,
    version: String,
    x: i32,
    z: i32,
) -> Result<seedmap::SeedMapBlock, String> {
    let resolved = seedmap::resolve_version(&version)?;
    let seed = seedmap::parse_seed(&seed)?;
    seedmap::biome_at(&resolved.family, seed, x, z)
}

#[tauri::command]
pub async fn get_seed_map_structures(
    seed: String,
    version: String,
    rx: i32,
    rz: i32,
) -> Result<Vec<seedmap::SeedMapStructure>, String> {
    let resolved = seedmap::resolve_version(&version)?;
    let seed = seedmap::parse_seed(&seed)?;
    tauri::async_runtime::spawn_blocking(move || {
        seedmap::collect_structures(&resolved.family, seed, rx, rz)
    })
    .await
    .map_err(|e| format!("结构查询任务异常\n{e}"))?
}

#[tauri::command]
pub async fn find_seed_map_structures(
    seed: String,
    version: String,
    kind: String,
    center_x: i32,
    center_z: i32,
    radius: i32,
) -> Result<Vec<seedmap::SeedMapStructure>, String> {
    let resolved = seedmap::resolve_version(&version)?;
    let seed = seedmap::parse_seed(&seed)?;
    tauri::async_runtime::spawn_blocking(move || {
        seedmap::find_structures(&resolved.family, seed, &kind, center_x, center_z, radius)
    })
    .await
    .map_err(|e| format!("结构搜索任务异常\n{e}"))?
}

#[tauri::command]
pub async fn find_seed_map_slime_chunks(
    seed: String,
    version: String,
    center_x: i32,
    center_z: i32,
    radius: i32,
) -> Result<Vec<seedmap::SeedMapSlimeChunk>, String> {
    let resolved = seedmap::resolve_version(&version)?;
    let seed = seedmap::parse_seed(&seed)?;
    tauri::async_runtime::spawn_blocking(move || {
        seedmap::find_slime_chunks(&resolved.family, seed, center_x, center_z, radius)
    })
    .await
    .map_err(|e| format!("史莱姆区块搜索任务异常\n{e}"))?
}

#[tauri::command]
pub fn get_seed_map_slime_chunks(
    seed: String,
    version: String,
    rx: i32,
    rz: i32,
) -> Result<Vec<seedmap::SeedMapSlimeChunk>, String> {
    let resolved = seedmap::resolve_version(&version)?;
    let seed = seedmap::parse_seed(&seed)?;
    seedmap::slime_chunks(&resolved.family, seed, rx, rz)
}
