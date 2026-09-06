//! 地图渲染命令（薄胶水）：领域逻辑在 map/（region_io/render/tile_cache/chunk_pos）。

use std::path::Path;

use crate::map::region_io::{dimension_dir, list_region_files, MapRegionInfo};
use crate::map::render::{render_region_tile, RenderStyle};
use crate::map::tile_cache::{read_cached_tile, tile_cache_key, write_tile};

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
    Ok(list_region_files(&dimension_dir(Path::new(&world_path), &dimension)))
}

/// 渲染单个 region 的 512×512 地图瓦片 PNG（渲染/缓存逻辑在 map/ 域模块）。
/// 渲染在 spawn_blocking 工作线程执行——半生成 region（部分 chunk 已参与渲染）
/// 单瓦片耗时可达秒级，同步跑主线程会整窗卡死。
#[tauri::command]
pub async fn get_map_tile(
    world_path: String,
    dimension: String,
    rx: i32,
    rz: i32,
    style: String,
) -> Result<tauri::ipc::Response, String> {
    validate_world_path(&world_path)?;
    let (render_style, style_key) = parse_style(&style)?;
    let world_root = Path::new(&world_path).to_path_buf();
    let region_file = dimension_dir(&world_root, &dimension)
        .join("region")
        .join(format!("r.{rx}.{rz}.mca"));
    let Some(cache_path) = tile_cache_key(&world_root, &dimension, rx, rz, &style_key) else {
        return Err("region 文件不存在\n前端应按 list_map_regions 请求".into());
    };
    // 缓存键含 region mtime：存档未变时直接命中磁盘缓存
    if let Some(png) = read_cached_tile(&cache_path) {
        return Ok(tauri::ipc::Response::new(png));
    }
    let png = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        // Mod 色覆盖层（实例动态指纹；None = 纯原版）
        let overlay = crate::map::mod_color::overlay_for_world(&world_root);
        let png = render_region_tile(&region_file, render_style, overlay.as_deref())?;
        write_tile(&cache_path, &png)?;
        Ok(png)
    })
    .await
    .map_err(|e| format!("瓦片渲染任务异常\n{e}"))??;
    Ok(tauri::ipc::Response::new(png))
}

/// 悬停识别载荷：表面方块名 + 所在群系原始 id（26.x/Mod 群系同样支持）。
#[derive(serde::Serialize)]
pub struct MapBlockInfo {
    pub block: Option<String>,
    pub biome: Option<String>,
}

/// 取指定方块坐标的表面方块与群系（悬停识别用；与 top-down 同语义）。
/// 传 `y`（切片模式）时改为精确探测该层：方块为该层实际方块（含空气），
/// 群系为该层覆盖的 4×4×4 群系单元（地下不报地表群系）。未生成/损坏 → 双 null。
/// 主世界未生成区（region 缺失）且种子可读时，`biome` 回退为预测群系名。
#[tauri::command]
pub fn get_map_block(
    world_path: String,
    dimension: String,
    x: i32,
    z: i32,
    y: Option<i32>,
) -> Result<MapBlockInfo, String> {
    validate_world_path(&world_path)?;
    let world_root = Path::new(&world_path);
    let region_file = crate::map::region_io::region_file_for(world_root, &dimension, x, z);
    let found = match y {
        Some(y) => crate::map::region_io::probe_at(&region_file, x, z, y as isize),
        None => crate::map::region_io::surface_info(&region_file, x, z),
    };
    let Some((block, biome)) = found else {
        // 未生成区：主世界走预测回退（只有 vanilla 主世界语义成立）
        if dimension.is_empty() {
            if let Some(seed) = crate::map::biomegen::world::world_seed(world_root) {
                let large = crate::map::biomegen::world::world_large_biomes(world_root);
                let pred = crate::map::biomegen::world::shared_predictor(seed, large);
                let qx = x.div_euclid(4);
                let qz = z.div_euclid(4);
                let id = pred.predict_cell(qx, qz, 16);
                let table = crate::map::biomegen::overworld_26();
                if let Some(name) = ((id as usize) < table.names.len())
                    .then(|| table.names[id as usize])
                {
                    return Ok(MapBlockInfo {
                        block: None,
                        biome: Some(name.to_string()),
                    });
                }
            }
        }
        return Ok(MapBlockInfo { block: None, biome: None });
    };
    Ok(MapBlockInfo {
        block: Some(block),
        biome,
    })
}

/// 预测瓦片：未探索 region 的生物群系预测底图（512×512 PNG，磁盘缓存）。
/// 仅支持主世界 vanilla 语义（26.x/1.20.1+ 参数族；1.18 前存档无预测）。
/// 渲染在 spawn_blocking 工作线程（单瓦片毫秒~秒级，不能占主线程卡 UI）。
///
/// 26.x 存档（DataVersion ≥ MIN_TERRAIN_DATA_VERSION）叠加地形水域分层；
/// 旧参数族（如 1.20.1）仅输出群系色底图——26.x final_density 语义未对其校准。
/// 缓存键含 terrain 开关，两路径互不串用。
#[tauri::command]
pub async fn get_prediction_tile(
    world_path: String,
    dimension: String,
    rx: i32,
    rz: i32,
) -> Result<tauri::ipc::Response, String> {
    validate_world_path(&world_path)?;
    if !dimension.is_empty() {
        return Err("预测仅支持主世界\n未探索区预测基于主世界生物群系采样".into());
    }
    let world_root = Path::new(&world_path);
    let Some(seed) = crate::map::biomegen::world::world_seed(world_root) else {
        return Err("无法读取世界种子\n未找到 data/minecraft/world_gen_settings.dat".into());
    };
    let terrain_ok = crate::map::biomegen::world::world_data_version(world_root)
        .is_some_and(|v| v >= MIN_TERRAIN_DATA_VERSION);
    let cache_path =
        crate::map::tile_cache::prediction_cache_key(world_root, seed, rx, rz, terrain_ok);
    if let Some(png) = crate::map::tile_cache::read_cached_tile(&cache_path) {
        return Ok(tauri::ipc::Response::new(png));
    }
    let large = crate::map::biomegen::world::world_large_biomes(world_root);
    let png = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let pred = crate::map::biomegen::world::shared_predictor(seed, large);
        let png = if terrain_ok {
            let eng = crate::map::biomegen::terrain::solver::shared_terrain(seed)?;
            crate::map::biomegen::png_tile::render_region_prediction_terrain(&pred, Some(&eng), rx, rz)?
        } else {
            crate::map::biomegen::png_tile::render_region_prediction(&pred, rx, rz)?
        };
        crate::map::tile_cache::write_tile(&cache_path, &png)?;
        Ok(png)
    })
    .await
    .map_err(|e| format!("预测任务异常\n{e}"))??;
    Ok(tauri::ipc::Response::new(png))
}

/// 种子模式预测瓦片：无存档依赖，直接以种子渲染（26.x 参数族水域语义）。
/// 种子为十进制 i64 字符串（数值种子可超 2^53，禁止经 JS Number 中转丢精度）。
/// 缓存键含种子；渲染在 spawn_blocking（与存档版同管线）。
#[tauri::command]
pub async fn get_prediction_tile_seed(
    seed: String,
    rx: i32,
    rz: i32,
) -> Result<tauri::ipc::Response, String> {
    let seed = parse_i64_seed(&seed)?;
    let cache_path = crate::map::tile_cache::seed_prediction_cache_key(seed, rx, rz);
    if let Some(png) = crate::map::tile_cache::read_cached_tile(&cache_path) {
        return Ok(tauri::ipc::Response::new(png));
    }
    let png = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let png = render_seed_prediction_png(seed, rx, rz)?;
        crate::map::tile_cache::write_tile(&cache_path, &png)?;
        Ok(png)
    })
    .await
    .map_err(|e| format!("预测任务异常\n{e}"))??;
    Ok(tauri::ipc::Response::new(png))
}

/// 种子悬停载荷：群系预测 + 水面标记（block 恒 null → 前端走"预测区"语义）。
#[derive(serde::Serialize)]
pub struct SeedBlockInfo {
    pub block: Option<String>,
    pub biome: Option<String>,
    pub water: bool,
}

/// 种子模式悬停：直接以种子查询（不依赖存档/region）。
#[tauri::command]
pub fn get_seed_map_block(seed: String, x: i32, z: i32) -> Result<SeedBlockInfo, String> {
    let seed = parse_i64_seed(&seed)?;
    let pred = crate::map::biomegen::world::shared_predictor(seed, false);
    let qx = x.div_euclid(4);
    let qz = z.div_euclid(4);
    let id = pred.predict_cell(qx, qz, 16);
    let table = crate::map::biomegen::overworld_26();
    let biome = ((id as usize) < table.names.len()).then(|| table.names[id as usize].to_string());
    // 水标依赖地形引擎；引擎缺陷（内嵌数据）时仅丢水标不丢群系
    let water = crate::map::biomegen::terrain::solver::shared_terrain(seed)
        .map(|eng| eng.water_class(x as f64, z as f64).is_some())
        .unwrap_or(false);
    Ok(SeedBlockInfo {
        block: None,
        biome,
        water,
    })
}

/// 十进制 i64 种子解析（前端保证十进制形态；非数字种子在前端转 Java 哈希）。
fn parse_i64_seed(seed: &str) -> Result<i64, String> {
    seed.trim()
        .parse::<i64>()
        .map_err(|_| "种子无效\n仅支持十进制整数种子（i64 范围）".to_string())
}

/// 种子渲染共用：26.x 参数族（DataVersion ≥ 4790 校准）固定走水域分层管线。
fn render_seed_prediction_png(seed: i64, rx: i32, rz: i32) -> Result<Vec<u8>, String> {
    let pred = crate::map::biomegen::world::shared_predictor(seed, false);
    let eng = crate::map::biomegen::terrain::solver::shared_terrain(seed)?;
    crate::map::biomegen::png_tile::render_region_prediction_terrain(&pred, Some(&eng), rx, rz)
}

/// 地形 final_density 语义按 26.1.2（DataVersion 4790，实测校准存档）参数族校准。
/// 旧参数族（如 1.20.1 = 3465）仅输出群系色底图，避免 26.x 密度树误画水域；
/// 未来更高版本沿用同族参数时继续生效（若 Mojang 改地形结构需重校准此值）。
const MIN_TERRAIN_DATA_VERSION: i32 = 4790;

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
}
