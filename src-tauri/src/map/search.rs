//! 已加载地图的群系检索：按区块中心表面群系扫描 region，不生成未加载区块。
//! 旧存档缺少 section biome palette 时跳过；结果只覆盖已存在 region。

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::region_io::{dimension_dir, list_region_files, parse_region};

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapBiomeSummary {
    pub biome: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapBiomeHit {
    pub biome: String,
    pub x: i32,
    pub z: i32,
}

fn normalized(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

fn scan<F>(world: &Path, dimension: &str, max_y: i32, mut visit: F)
where
    F: FnMut(&str, i32, i32),
{
    let dir = dimension_dir(world, dimension);
    let max_y = max_y.clamp(-512, 512) as isize;
    for region in list_region_files(&dir) {
        let path = dir
            .join("region")
            .join(format!("r.{}.{}.mca", region.x, region.z));
        for (i, chunk) in parse_region(&path).into_iter().enumerate() {
            let Some(chunk) = chunk else {
                continue;
            };
            let Some(biome) = chunk.surface_biome(max_y) else {
                continue;
            };
            let cx = (i % 32) as i32;
            let cz = (i / 32) as i32;
            visit(
                &biome,
                region.x * 512 + cx * 16 + 8,
                region.z * 512 + cz * 16 + 8,
            );
        }
    }
}

/// 已加载区域内出现过的群系及区块数。
pub fn list_biomes(world: &Path, dimension: &str, max_y: i32) -> Vec<MapBiomeSummary> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    scan(world, dimension, max_y, |biome, _, _| {
        *counts.entry(biome.to_string()).or_insert(0) += 1;
    });
    let mut out: Vec<MapBiomeSummary> = counts
        .into_iter()
        .map(|(biome, count)| MapBiomeSummary { biome, count })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.biome.cmp(&b.biome)));
    out
}

/// 已加载区域内离中心最近的匹配群系区块中心。
pub fn find_biome(
    world: &Path,
    dimension: &str,
    max_y: i32,
    target: &str,
    center_x: i32,
    center_z: i32,
) -> Option<MapBiomeHit> {
    let target = normalized(target);
    let mut best: Option<(i64, MapBiomeHit)> = None;
    scan(world, dimension, max_y, |biome, x, z| {
        if normalized(biome) != target {
            return;
        }
        let dx = (x - center_x) as i64;
        let dz = (z - center_z) as i64;
        let dist = dx * dx + dz * dz;
        if best.as_ref().is_none_or(|(d, _)| dist < *d) {
            best = Some((
                dist,
                MapBiomeHit {
                    biome: biome.to_string(),
                    x,
                    z,
                },
            ));
        }
    });
    best.map(|(_, hit)| hit)
}
