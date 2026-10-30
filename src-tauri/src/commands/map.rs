//! 世界预览命令：版本/存档路径由后端解析，只读已加载 region。
//! 瓦片为俯视 PNG，`max_y` 指定向下扫描的最高 Y（降低可看洞穴/地下）。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::game::version::dir::version_dir_path;
use crate::map::folder::{discover_worlds, MapFolderEntry};
use crate::map::region_io::{
    dimension_dir, list_region_files, region_file_for, surface_info, MapRegionInfo,
};
use crate::map::render::{render_region_tile, RenderStyle};
use crate::map::search::{find_biome, list_biomes, MapBiomeHit, MapBiomeSummary};
use crate::map::tile_cache::{read_cached_tile, tile_cache_key, write_tile};
use crate::map::world_info::{world_info, MapWorldInfo};
use crate::seedmap;
use crate::util::is_simple_name;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapBlockInfo {
    pub biome: Option<String>,
}

fn world_root(version_name: &str, world_name: &str) -> Result<PathBuf, String> {
    if !is_simple_name(version_name) {
        bail_key!(
            "errors.map.invalid_version",
            "版本名非法\n仅允许不含路径分隔符与 `..` 的名称"
        );
    }
    if !is_simple_name(world_name) {
        bail_key!(
            "errors.map.invalid_world",
            "存档名非法\n仅允许不含路径分隔符与 `..` 的名称"
        );
    }
    let vdir = version_dir_path(version_name)?;
    let root = vdir.join("saves").join(world_name);
    if !root.is_dir() {
        bail_key!("errors.map.missing_world", "存档不存在\n{}", world_name);
    }
    Ok(root)
}

fn validate_dimension(world: &Path, dimension: &str) -> Result<(), String> {
    if dimension_dir(world, dimension).join("region").is_dir() {
        Ok(())
    } else {
        bail_key!("errors.map.missing_dimension", "维度不存在\n请刷新维度列表")
    }
}

fn folder_root(path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(path);
    if !root.is_dir() {
        bail_key!("errors.map.missing_world", "文件夹不存在\n{}", path);
    }
    Ok(root)
}

fn map_regions(world: &Path, dimension: &str) -> Result<Vec<MapRegionInfo>, String> {
    validate_dimension(world, dimension)?;
    Ok(list_region_files(&dimension_dir(world, dimension)))
}

fn map_block(
    world: &Path,
    dimension: &str,
    x: i32,
    z: i32,
    max_y: i32,
) -> Result<MapBlockInfo, String> {
    validate_dimension(world, dimension)?;
    let region_file = region_file_for(world, dimension, x, z);
    let max_y = max_y.clamp(-512, 512) as isize;
    let biome = surface_info(&region_file, x, z, max_y).and_then(|(_, biome, _)| biome);
    Ok(MapBlockInfo { biome })
}

/// 渲染/读取单块 region 的 512×512 俯视 PNG（自 max_y 向下）。
fn map_tile(
    world: &Path,
    dimension: &str,
    rx: i32,
    rz: i32,
    max_y: i32,
    mod_colors: bool,
) -> Result<Vec<u8>, String> {
    validate_dimension(world, dimension)?;
    let max_y = max_y.clamp(-512, 512) as isize;
    let render_style = RenderStyle::TopDown { max_y };
    let cache_style = format!("topdown{max_y}|mod{}", if mod_colors { 1 } else { 0 });

    let Some(cache_path) = tile_cache_key(world, dimension, rx, rz, &cache_style) else {
        bail_key!(
            "errors.map.missing_region",
            "region 文件不存在\n前端应按 region 列表请求"
        );
    };
    if let Some(png) = read_cached_tile(&cache_path) {
        return Ok(png);
    }

    let region_file = dimension_dir(world, dimension)
        .join("region")
        .join(format!("r.{rx}.{rz}.mca"));
    let overlay = mod_colors
        .then(|| crate::map::mod_color::overlay_for_world(world))
        .flatten();
    let png = render_region_tile(&region_file, render_style, overlay.as_deref())?;
    write_tile(&cache_path, &png)?;
    Ok(png)
}

#[tauri::command]
pub async fn get_map_world_info(
    version_name: String,
    world_name: String,
) -> Result<MapWorldInfo, String> {
    let info = tauri::async_runtime::spawn_blocking(move || -> Result<MapWorldInfo, String> {
        let world = world_root(&version_name, &world_name)?;
        Ok(world_info(&world))
    })
    .await
    .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(info)
}

#[tauri::command]
pub async fn list_map_regions(
    version_name: String,
    world_name: String,
    dimension: String,
) -> Result<Vec<MapRegionInfo>, String> {
    let regions = tauri::async_runtime::spawn_blocking(move || {
        let world = world_root(&version_name, &world_name)?;
        map_regions(&world, &dimension)
    })
    .await
    .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(regions)
}

#[tauri::command]
pub async fn get_map_tile(
    version_name: String,
    world_name: String,
    dimension: String,
    rx: i32,
    rz: i32,
    max_y: i32,
    mod_colors: bool,
) -> Result<tauri::ipc::Response, String> {
    let png = tauri::async_runtime::spawn_blocking(move || {
        let world = world_root(&version_name, &world_name)?;
        map_tile(&world, &dimension, rx, rz, max_y, mod_colors)
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.tile_failed",
        "瓦片渲染任务异常"
    ))??;
    Ok(tauri::ipc::Response::new(png))
}

/// 悬停识别：自 max_y 向下取首个非空气方块与所在群系。
#[tauri::command]
pub async fn get_map_block(
    version_name: String,
    world_name: String,
    dimension: String,
    x: i32,
    z: i32,
    max_y: i32,
) -> Result<MapBlockInfo, String> {
    let info = tauri::async_runtime::spawn_blocking(move || {
        let world = world_root(&version_name, &world_name)?;
        map_block(&world, &dimension, x, z, max_y)
    })
    .await
    .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(info)
}

/// 递归查找用户所选文件夹内的世界根目录（目录本身是世界时只返回它）。
#[tauri::command]
pub async fn resolve_map_folders(path: String) -> Result<Vec<MapFolderEntry>, String> {
    let folders = tauri::async_runtime::spawn_blocking(move || discover_worlds(Path::new(&path)))
        .await
        .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(folders)
}

#[tauri::command]
pub async fn get_map_folder_info(root: String) -> Result<MapWorldInfo, String> {
    let info = tauri::async_runtime::spawn_blocking(move || -> Result<MapWorldInfo, String> {
        let world = folder_root(&root)?;
        Ok(world_info(&world))
    })
    .await
    .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(info)
}

#[tauri::command]
pub async fn list_map_folder_regions(
    root: String,
    dimension: String,
) -> Result<Vec<MapRegionInfo>, String> {
    let regions = tauri::async_runtime::spawn_blocking(move || {
        let world = folder_root(&root)?;
        map_regions(&world, &dimension)
    })
    .await
    .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(regions)
}

#[tauri::command]
pub async fn get_map_folder_tile(
    root: String,
    dimension: String,
    rx: i32,
    rz: i32,
    max_y: i32,
    mod_colors: bool,
) -> Result<tauri::ipc::Response, String> {
    let png = tauri::async_runtime::spawn_blocking(move || {
        let world = folder_root(&root)?;
        map_tile(&world, &dimension, rx, rz, max_y, mod_colors)
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.tile_failed",
        "瓦片渲染任务异常"
    ))??;
    Ok(tauri::ipc::Response::new(png))
}

#[tauri::command]
pub async fn get_map_folder_block(
    root: String,
    dimension: String,
    x: i32,
    z: i32,
    max_y: i32,
) -> Result<MapBlockInfo, String> {
    let info = tauri::async_runtime::spawn_blocking(move || {
        let world = folder_root(&root)?;
        map_block(&world, &dimension, x, z, max_y)
    })
    .await
    .map_err(common_err_ctx_key!("errors.map.failed", "地图操作失败"))??;
    Ok(info)
}

/// 世界预览史莱姆覆盖层：单个 region 的史莱姆区块（与版本无关）。
#[tauri::command]
pub fn get_map_slime_chunks(
    seed: String,
    rx: i32,
    rz: i32,
) -> Result<Vec<seedmap::SeedMapSlimeChunk>, String> {
    let seed = seedmap::parse_seed(&seed)?;
    Ok(seedmap::slime_chunks_seed(seed, rx, rz))
}

/// 世界预览史莱姆搜索：返回中心附近最近的 32 个史莱姆区块。
#[tauri::command]
pub async fn find_map_slime_chunks(
    seed: String,
    center_x: i32,
    center_z: i32,
    radius: i32,
) -> Result<Vec<seedmap::SeedMapSlimeChunk>, String> {
    let seed = seedmap::parse_seed(&seed)?;
    tauri::async_runtime::spawn_blocking(move || {
        seedmap::find_slime_chunks_seed(seed, center_x, center_z, radius)
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.slimeSearchFailed",
        "史莱姆区块搜索任务异常"
    ))
}

/// 已加载地图的群系列表（按区块数降序）。
#[tauri::command]
pub async fn list_map_biomes(
    version_name: String,
    world_name: String,
    dimension: String,
    max_y: i32,
) -> Result<Vec<MapBiomeSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<MapBiomeSummary>, String> {
        let world = world_root(&version_name, &world_name)?;
        validate_dimension(&world, &dimension)?;
        Ok(list_biomes(&world, &dimension, max_y))
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.biomeListFailed",
        "群系列表任务异常"
    ))?
}

#[tauri::command]
pub async fn list_map_folder_biomes(
    root: String,
    dimension: String,
    max_y: i32,
) -> Result<Vec<MapBiomeSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<MapBiomeSummary>, String> {
        let world = folder_root(&root)?;
        validate_dimension(&world, &dimension)?;
        Ok(list_biomes(&world, &dimension, max_y))
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.biomeListFailed",
        "群系列表任务异常"
    ))?
}

/// 已加载地图内离中心最近的指定群系。
#[tauri::command]
pub async fn find_map_biome(
    version_name: String,
    world_name: String,
    dimension: String,
    max_y: i32,
    biome: String,
    center_x: i32,
    center_z: i32,
) -> Result<Option<MapBiomeHit>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<MapBiomeHit>, String> {
        let world = world_root(&version_name, &world_name)?;
        validate_dimension(&world, &dimension)?;
        Ok(find_biome(
            &world, &dimension, max_y, &biome, center_x, center_z,
        ))
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.biomeSearchFailed",
        "群系搜索任务异常"
    ))?
}

#[tauri::command]
pub async fn find_map_folder_biome(
    root: String,
    dimension: String,
    max_y: i32,
    biome: String,
    center_x: i32,
    center_z: i32,
) -> Result<Option<MapBiomeHit>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<MapBiomeHit>, String> {
        let world = folder_root(&root)?;
        validate_dimension(&world, &dimension)?;
        Ok(find_biome(
            &world, &dimension, max_y, &biome, center_x, center_z,
        ))
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.biomeSearchFailed",
        "群系搜索任务异常"
    ))?
}
