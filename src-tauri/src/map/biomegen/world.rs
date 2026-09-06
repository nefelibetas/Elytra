//! 种子读取 + 预测器实例缓存（命令层复用；域内轻量静态）。

use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use fastnbt::Value;

use super::BiomePredictor;

/// 世界根目录 → 世界种子：
/// 26.x 在 `data/minecraft/world_gen_settings.dat`（gzip compound `data.seed` Long）；
/// 旧版存档回退读 level.dat（gzip 根 `Data.seed` Long）。
pub fn world_seed(world_root: &Path) -> Option<i64> {
    for path in world_gen_files(world_root) {
        let Some(map) = read_gz_nbt(&path) else { continue };
        // 26.x：root.data.seed；旧版：root.Data.seed
        for (container, key) in [("data", "seed"), ("Data", "seed")] {
            if let Some(Value::Compound(data)) = map.get(container) {
                if let Some(Value::Long(s)) = data.get(key) {
                    return Some(*s);
                }
            }
        }
    }
    None
}

/// 世界 DataVersion（26.x 存于 world_gen_settings.dat 根，旧版在 level.dat 根）。
/// 用于引擎能力门控（地形 final_density 语义仅对 26.x 参数族校准）。
pub fn world_data_version(world_root: &Path) -> Option<i32> {
    for path in world_gen_files(world_root) {
        let Some(map) = read_gz_nbt(&path) else { continue };
        if let Some(Value::Int(v)) = map.get("DataVersion") {
            return Some(*v);
        }
    }
    None
}

fn world_gen_files(world_root: &Path) -> [std::path::PathBuf; 2] {
    [
        world_root.join("data/minecraft/world_gen_settings.dat"),
        world_root.join("level.dat"),
    ]
}

fn read_gz_nbt(path: &Path) -> Option<std::collections::HashMap<String, Value>> {
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

/// large_biomes 判定：世界档案里维度设置引用 overworld_large_biomes 时（启发式含字节搜索）。
pub fn world_large_biomes(world_root: &Path) -> bool {
    for path in world_gen_files(world_root) {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let mut out = Vec::new();
        if flate2::read::GzDecoder::new(&bytes[..])
            .read_to_end(&mut out)
            .is_err()
        {
            continue;
        }
        return out.windows(b"large_biomes".len()).any(|w| w == b"large_biomes");
    }
    false
}

type Slot = Option<(i64, bool, Arc<BiomePredictor>)>;
static PREDICTOR_SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();

/// 命令层共享预测器（同种子复用；单槽足够——多存档切换只是重建一次）。
pub fn shared_predictor(seed: i64, large: bool) -> Arc<BiomePredictor> {
    let m = PREDICTOR_SLOT.get_or_init(|| Mutex::new(None));
    let mut guard = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, l, p)) = guard.as_ref() {
        if *s == seed && *l == large {
            return p.clone();
        }
    }
    let p = Arc::new(BiomePredictor::new(seed));
    *guard = Some((seed, large, p.clone()));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_reader_known_world() {
        let w = Path::new(r"C:\tmp\iso2\world");
        if w.join("data/minecraft/world_gen_settings.dat").exists() {
            assert_eq!(world_seed(w), Some(7354360020768317315));
            assert!(!world_large_biomes(w));
        }
    }
}
