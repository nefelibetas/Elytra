//! 地图渲染命令（薄胶水）：领域逻辑在 map/（region_io/render/tile_cache/chunk_pos）。

use std::path::Path;

use serde::Serialize;

use crate::map::render::{encode_png, render_chunk, RenderStyle};
use crate::map::region_io::{list_region_files, parse_region};
use crate::map::tile_cache::{read_cached_tile, tile_cache_key, write_tile};
use std::path::PathBuf;

/// region 列表项（视口定位：前端算屏幕坐标）。
#[derive(Debug, Clone, Serialize)]
pub struct MapRegionInfo {
    pub x: i32,
    pub z: i32,
}

/// 样式参数解析：`topdown` 或 `slice@Y`。
/// 统一入口防注入（只接受这两种形态），样式键同时作为缓存键成分。
fn parse_style(style: &str) -> Result<(RenderStyle, String), String> {
    if style == "topdown" {
        return Ok((RenderStyle::TopDown, "topdown".into()));
    }
    let Some(y) = style.strip_prefix("slice@") else {
        return Err("样式参数无效\n仅支持 topdown 或 slice@Y".into());
    };
    let y: isize = y
        .parse()
        .map_err(|_| "样式参数无效\nslice@Y 的 Y 必须是整数".to_string())?;
    Ok((RenderStyle::Slice { y }, format!("slice{y}")))
}

/// 列出存档某维度的全部 region（按坐标排序）。
/// `world_path` 为世界根目录（含 level.dat）；`dimension` 为空串（主世界）、
/// "DIM-1"（下界）、"DIM1"（末地）——也接受相对路径（如 dimensions/minecraft/overworld）。
#[tauri::command]
pub fn list_map_regions(world_path: String, dimension: String) -> Result<Vec<MapRegionInfo>, String> {
    validate_world_path(&world_path)?;
    let dir = dimension_dir(&world_path, &dimension);
    Ok(list_region_files(&dir)
        .into_iter()
        .map(|(x, z, _)| MapRegionInfo { x, z })
        .collect())
}

/// 渲染单个 region 的地图瓦片（top-down 或 y 切片），PNG 字节直接返回。
/// 缓存键含 region mtime：存档未变时直接命中磁盘缓存。
/// region 文件不存在 → Err（前端跳过）；region 存在但无有效 chunk → 返回 1×1 空 PNG 由前端处理。
#[tauri::command]
pub fn get_map_tile(
    world_path: String,
    dimension: String,
    rx: i32,
    rz: i32,
    style: String,
) -> Result<tauri::ipc::Response, String> {
    validate_world_path(&world_path)?;
    let (render_style, style_key) = parse_style(&style)?;
    let region_file = dimension_dir(&world_path, &dimension)
        .join("region")
        .join(format!("r.{rx}.{rz}.mca"));

    let Some(cache_path) = tile_cache_key(Path::new(&world_path), &dimension, rx, rz, &style_key)
    else {
        return Err("region 文件不存在\n前端应按 list_map_regions 请求".into());
    };
    if let Some(png) = read_cached_tile(&cache_path) {
        return Ok(tauri::ipc::Response::new(png));
    }

    // 渲染（CPU 活：整 region 1024 chunk 解析；命令为同步 fn，
    // tauri sync command 在专用线程池执行，不阻塞主线程/其他命令）
    let parsed = parse_region(&region_file);
    let mut png: Option<Vec<u8>> = None;
    for p in parsed.iter().flatten() {
        let px = render_chunk(p, render_style);
        png = Some(encode_png(&px)?);
        break;
    }
    let Some(png) = png else {
        // region 存在但全未生成：16×16 全透明 PNG（合法 PNG，前端 <img> 可渲染）
        const EMPTY: [[u8; 4]; 256] = [[0, 0, 0, 0]; 256];
        let empty_png = encode_png(&EMPTY)?;
        return Ok(tauri::ipc::Response::new(empty_png));
    };
    write_tile(&cache_path, &png)?;
    Ok(tauri::ipc::Response::new(png))
}

/// 世界路径校验：level.dat 必须存在（既是合法性检查也是防任意路径探测）。
fn validate_world_path(world_path: &str) -> Result<(), String> {
    if world_path.contains("..") {
        return Err("路径无效\n包含 .. 段".into());
    }
    if !Path::new(world_path).join("level.dat").exists() {
        return Err("存档无效\n未找到 level.dat".into());
    }
    Ok(())
}

/// 维度目录解析：空串=主世界（26.x 存档在 dimensions/minecraft/overworld，
/// 旧布局在世界根）；DIM-1/DIM1 同理带 fallback。
fn dimension_dir(world_path: &str, dimension: &str) -> PathBuf {
    let base = Path::new(world_path);
    match dimension {
        "" => {
            let legacy_overworld = base.join("dimensions/minecraft/overworld");
            if legacy_overworld.join("region").exists() {
                legacy_overworld
            } else {
                base.to_path_buf()
            }
        }
        d @ ("DIM-1" | "DIM1") => {
            let ns = base.join("dimensions/minecraft").join(if d == "DIM-1" { "the_nether" } else { "the_end" });
            if ns.join("region").exists() {
                ns
            } else {
                base.join(d)
            }
        }
        other => base.join(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_parsing() {
        assert!(matches!(parse_style("topdown"), Ok((RenderStyle::TopDown, _))));
        assert!(matches!(
            parse_style("slice@-64"),
            Ok((RenderStyle::Slice { y: -64 }, _))
        ));
        assert!(parse_style("slice@abc").is_err());
        assert!(parse_style("../etc").is_err());
        assert!(parse_style("").is_err());
    }

    #[test]
    fn world_path_validation_rejects_escape() {
        assert!(validate_world_path("C:/some/..").is_err());
        assert!(validate_world_path("D:/nonexistent").is_err());
    }

    #[test]
    fn dimension_dir_prefers_new_layout() {
        // 26.x 布局：dimensions/minecraft/overworld
        let dir = std::env::temp_dir().join(format!("elytra_map_dim_{}", std::process::id()));
        let world = dir.join("w");
        std::fs::create_dir_all(world.join("dimensions/minecraft/overworld/region")).unwrap();
        let got = dimension_dir(world.to_str().unwrap(), "");
        assert_eq!(got, world.join("dimensions/minecraft/overworld"));
        // 旧布局 fallback：世界根
        let got2 = dimension_dir(dir.join("nowhere").to_str().unwrap(), "");
        assert!(got2.ends_with("nowhere"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
