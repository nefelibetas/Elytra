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
            if let Some(v) = data.get("seed").or_else(|| data.get("RandomSeed")) {
                match v {
                    Value::Long(v) => return Some(v.to_string()),
                    Value::Int(v) => return Some(v.to_string()),
                    _ => {}
                }
            }
            // 1.16+ 原版把种子放进 WorldGenSettings（部分工具写 snake_case）
            for key in ["WorldGenSettings", "world_gen_settings"] {
                if let Some(Value::Compound(settings)) = data.get(key) {
                    match settings.get("seed") {
                        Some(Value::Long(v)) => return Some(v.to_string()),
                        Some(Value::Int(v)) => return Some(v.to_string()),
                        _ => {}
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;

    fn write_gzip_nbt(path: &Path, value: &Value) {
        let raw = fastnbt::to_bytes(value).unwrap();
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&raw).unwrap();
        std::fs::write(path, enc.finish().unwrap()).unwrap();
    }

    #[test]
    fn reads_nbt_fields_and_modern_y_range() {
        let dir = tempfile::tempdir().unwrap();
        let mut data = HashMap::new();
        data.insert("seed".to_string(), Value::Long(1234));
        let mut root = HashMap::new();
        root.insert("DataVersion".to_string(), Value::Int(3465));
        root.insert("Data".to_string(), Value::Compound(data));
        write_gzip_nbt(&dir.path().join("level.dat"), &Value::Compound(root));

        let info = world_info(dir.path());
        assert_eq!(info.data_version, Some(3465));
        assert_eq!(info.seed.as_deref(), Some("1234"));
        assert_eq!((info.min_y, info.max_y), (-64, 319), "1.21+ 数据版本");
        assert!(info.dimensions.is_empty());
    }

    #[test]
    fn invalid_level_dat_degrades_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("region")).unwrap();
        std::fs::write(dir.path().join("level.dat"), b"not-nbt").unwrap();

        let info = world_info(dir.path());
        assert_eq!(info.dimensions, vec![String::new()]);
        assert_eq!((info.min_y, info.max_y), (0, 255));
        assert!(info.seed.is_none(), "损坏的 level.dat 不应影响地图可看");
        assert!(info.data_version.is_none());
    }

    /// 26.x 把种子/DataVersion 放在 world_gen_settings.dat；旧 level.dat 只能兜底，
    /// 两份都在时必须以前者为准（否则读到过期数据）。
    #[test]
    fn world_gen_settings_takes_precedence_over_level_dat() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/minecraft")).unwrap();

        let mut old_data = HashMap::new();
        old_data.insert("seed".to_string(), Value::Long(1));
        let mut old_root = HashMap::new();
        old_root.insert("DataVersion".to_string(), Value::Int(100));
        old_root.insert("Data".to_string(), Value::Compound(old_data));
        write_gzip_nbt(&dir.path().join("level.dat"), &Value::Compound(old_root));

        let mut new_data = HashMap::new();
        new_data.insert("seed".to_string(), Value::Long(2));
        let mut new_root = HashMap::new();
        new_root.insert("DataVersion".to_string(), Value::Int(3465));
        new_root.insert("Data".to_string(), Value::Compound(new_data));
        write_gzip_nbt(
            &dir.path().join("data/minecraft/world_gen_settings.dat"),
            &Value::Compound(new_root),
        );

        let info = world_info(dir.path());
        assert_eq!(info.data_version, Some(3465), "26.x 文件优先");
        assert_eq!(info.seed.as_deref(), Some("2"));
        assert_eq!((info.min_y, info.max_y), (-64, 319));
    }
}
