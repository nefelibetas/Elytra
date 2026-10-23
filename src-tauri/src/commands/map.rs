//! 世界预览命令：版本/存档路径由后端解析，只读已加载 region。
//! 瓦片返回 PNG 二进制；固定 Y 切面用 `slice@Y`，top-down 用 `topdown`。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::game::version::dir::version_dir_path;
use crate::map::region_io::{
    dimension_dir, list_region_files, probe_at, region_file_for, surface_info, MapRegionInfo,
};
use crate::map::render::{render_region_tile, RenderStyle};
use crate::map::tile_cache::{read_cached_tile, tile_cache_key, write_tile};
use crate::map::world_info::{world_info, MapWorldInfo};
use crate::util::is_simple_name;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapBlockInfo {
    pub block: Option<String>,
    pub biome: Option<String>,
    pub y: Option<i32>,
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

/// 样式参数解析：`topdown` 或 `slice@Y`；同时返回缓存键片段。
fn parse_style(style: &str) -> Result<(RenderStyle, String), String> {
    if style == "topdown" {
        return Ok((RenderStyle::TopDown, "topdown".into()));
    }
    let Some(y) = style.strip_prefix("slice@") else {
        bail_key!(
            "errors.map.invalid_style",
            "样式参数无效\n仅支持 topdown 或 slice@Y"
        );
    };
    let y: isize = y.parse().map_err(|_| {
        "@i18n:errors.map.invalid_style\n样式参数无效\nslice@Y 的 Y 必须是整数".to_string()
    })?;
    Ok((RenderStyle::Slice { y }, format!("slice{y}")))
}

#[tauri::command]
pub fn get_map_world_info(
    version_name: String,
    world_name: String,
) -> Result<MapWorldInfo, String> {
    let world = world_root(&version_name, &world_name)?;
    Ok(world_info(&world))
}

#[tauri::command]
pub fn list_map_regions(
    version_name: String,
    world_name: String,
    dimension: String,
) -> Result<Vec<MapRegionInfo>, String> {
    let world = world_root(&version_name, &world_name)?;
    validate_dimension(&world, &dimension)?;
    Ok(list_region_files(&dimension_dir(&world, &dimension)))
}

/// 渲染/读取单块 region 的 512×512 PNG。
#[tauri::command]
pub async fn get_map_tile(
    version_name: String,
    world_name: String,
    dimension: String,
    rx: i32,
    rz: i32,
    style: String,
    mod_colors: bool,
) -> Result<tauri::ipc::Response, String> {
    let world = world_root(&version_name, &world_name)?;
    validate_dimension(&world, &dimension)?;
    let (render_style, style_key) = parse_style(&style)?;
    let cache_style = format!("{style_key}|mod{}", if mod_colors { 1 } else { 0 });
    let Some(cache_path) = tile_cache_key(&world, &dimension, rx, rz, &cache_style) else {
        bail_key!(
            "errors.map.missing_region",
            "region 文件不存在\n前端应按 region 列表请求"
        );
    };
    if let Some(png) = read_cached_tile(&cache_path) {
        return Ok(tauri::ipc::Response::new(png));
    }
    let region_file = dimension_dir(&world, &dimension)
        .join("region")
        .join(format!("r.{rx}.{rz}.mca"));
    let png = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let overlay = mod_colors
            .then(|| crate::map::mod_color::overlay_for_world(&world))
            .flatten();
        let png = render_region_tile(&region_file, render_style, overlay.as_deref())?;
        write_tile(&cache_path, &png)?;
        Ok(png)
    })
    .await
    .map_err(common_err_ctx_key!(
        "errors.map.tile_failed",
        "瓦片渲染任务异常"
    ))??;
    Ok(tauri::ipc::Response::new(png))
}

/// 悬停识别：top-down 取地表首个非空气方块；传 y 时取该层实际方块。
#[tauri::command]
pub fn get_map_block(
    version_name: String,
    world_name: String,
    dimension: String,
    x: i32,
    z: i32,
    y: Option<i32>,
) -> Result<MapBlockInfo, String> {
    let world = world_root(&version_name, &world_name)?;
    validate_dimension(&world, &dimension)?;
    let region_file = region_file_for(&world, &dimension, x, z);
    let found = match y {
        Some(y) => probe_at(&region_file, x, z, y as isize)
            .map(|(block, biome, y)| (block, biome, Some(y as i32))),
        None => surface_info(&region_file, x, z).map(|(block, biome, y)| (block, biome, Some(y))),
    };
    let Some((block, biome, y)) = found else {
        return Ok(MapBlockInfo {
            block: None,
            biome: None,
            y: None,
        });
    };
    Ok(MapBlockInfo {
        block: Some(block),
        biome,
        y,
    })
}
