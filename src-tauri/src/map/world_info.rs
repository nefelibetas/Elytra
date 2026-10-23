//! 世界元信息：种子、DataVersion、Y 范围与可预览维度。
//! 26.x 的种子在 `data/minecraft/world_gen_settings.dat`，旧版回退 `level.dat`。

use std::io::Read;
use std::path::{Path, PathBuf};

use fastnbt::Value;
use serde::Serialize;

use super::region_io::list_dimensions;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapWorldInfo {
    /// 十进制种子；读取失败为 None（地图仍可看，只是不能复制种子）。
    pub seed: Option<String>,
    pub data_version: Option<i32>,
    pub min_y: i32,
    pub max_y: i32,
    pub dimensions: Vec<String>,
}

pub fn world_info(world_root: &Path) -> MapWorldInfo {
    let data_version = read_data_version(world_root);
    let (min_y, max_y) = if data_version.is_some_and(|v| v >= 2825) {
        (-64, 319)
    } else {
        (0, 255)
    };
    MapWorldInfo {
        seed: read_seed(world_root),
        data_version,
        min_y,
        max_y,
        dimensions: list_dimensions(world_root),
    }
}

fn world_gen_files(world_root: &Path) -> [PathBuf; 2] {
    [
        world_root.join("data/minecraft/world_gen_settings.dat"),
        world_root.join("level.dat"),
    ]
}

fn read_nbt(path: &Path) -> Option<std::collections::HashMap<String, Value>> {
    let bytes = std::fs::read(path).ok()?;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_end(&mut out)
        .ok()?;
    match fastnbt::from_bytes::<Value>(&out).ok()? {
        Value::Compound(map) => Some(map),
        _ => None,
    }
}

fn read_seed(world_root: &Path) -> Option<String> {
    for path in world_gen_files(world_root) {
        let Some(map) = read_nbt(&path) else {
            continue;
        };
        for container in ["data", "Data"] {
            let Some(Value::Compound(data)) = map.get(container) else {
                continue;
            };
            match data.get("seed") {
                Some(Value::Long(v)) => return Some(v.to_string()),
                Some(Value::Int(v)) => return Some(v.to_string()),
                _ => {}
            }
        }
    }
    None
}

fn read_data_version(world_root: &Path) -> Option<i32> {
    for path in world_gen_files(world_root) {
        let Some(map) = read_nbt(&path) else {
            continue;
        };
        if let Some(Value::Int(v)) = map.get("DataVersion") {
            return Some(*v);
        }
        if let Some(Value::Compound(data)) = map.get("Data") {
            if let Some(Value::Int(v)) = data.get("DataVersion") {
                return Some(*v);
            }
        }
    }
    None
}
