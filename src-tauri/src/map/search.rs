//! 已加载地图的群系检索：按区块中心表面群系扫描 region，不生成未加载区块。
//! 旧存档缺少 section biome palette 时跳过；结果只覆盖已存在 region。
//! 每个 region 的结果按 (路径, mtime, max_y) 缓存，list_biomes 与 find_biome 共享。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde::Serialize;

use super::lru_cache::{CacheKey, LruCache};
use super::region_io::{dimension_dir, list_region_files, parse_region_with_raw};

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

/// region 内单 chunk 的中心表面群系（世界坐标）。
struct BiomeHit {
    biome: String,
    x: i32,
    z: i32,
}

/// 缓存 key：路径 + mtime 定位文件版本，max_y 区分扫描高度。
#[derive(PartialEq)]
struct BiomeKey {
    path: PathBuf,
    mtime: u64,
    max_y: isize,
}

impl CacheKey for BiomeKey {
    /// 同一 region 的同一高度只留最新文件版本；不同 max_y 各自成条目。
    fn same_slot(&self, other: &Self) -> bool {
        self.path == other.path && self.max_y == other.max_y
    }
}

/// 8MB ≈ 100+ 个满 region（每 region 最多 1024 条，单条约 50B 量级）。
const MAX_BIOME_CACHE_BYTES: usize = 8 * 1024 * 1024;
static BIOME_CACHE: LazyLock<Mutex<LruCache<BiomeKey, Arc<Vec<BiomeHit>>>>> =
    LazyLock::new(|| Mutex::new(LruCache::new(MAX_BIOME_CACHE_BYTES)));

fn biome_cache() -> &'static Mutex<LruCache<BiomeKey, Arc<Vec<BiomeHit>>>> {
    &BIOME_CACHE
}

/// 按 (路径, mtime, max_y) 缓存 region 的群系命中；锁外解析 + 双检。
/// 空结果不缓存，避免临时 IO 错误固化到 mtime 失效为止。
fn region_biome_hits(
    region_file: &Path,
    mtime: u64,
    region_x: i32,
    region_z: i32,
    max_y: isize,
) -> Arc<Vec<BiomeHit>> {
    let key = BiomeKey {
        path: region_file.to_path_buf(),
        mtime,
        max_y,
    };
    if let Some(hits) = biome_cache().lock().get(&key) {
        return hits;
    }

    #[cfg(test)]
    record_parse(region_file);
    let mut hits = Vec::new();
    for (i, chunk) in parse_region_with_raw(region_file).into_iter().enumerate() {
        let Some(chunk) = chunk else {
            continue;
        };
        let Some(biome) = chunk.surface_biome(max_y) else {
            continue;
        };
        let cx = (i % 32) as i32;
        let cz = (i / 32) as i32;
        hits.push(BiomeHit {
            biome,
            x: region_x * 512 + cx * 16 + 8,
            z: region_z * 512 + cz * 16 + 8,
        });
    }
    if hits.is_empty() {
        return Arc::new(hits);
    }

    let hits = Arc::new(hits);
    let bytes = biome_hits_bytes(&hits);
    let mut cache = biome_cache().lock();
    if let Some(existing) = cache.get(&key) {
        return existing;
    }
    cache.insert(key, Arc::clone(&hits), bytes);
    hits
}

/// 估算条目占用：元素本身 + biome 字符串堆分配。
fn biome_hits_bytes(hits: &[BiomeHit]) -> usize {
    std::mem::size_of_val(hits) + hits.iter().map(|h| h.biome.capacity()).sum::<usize>()
}

#[cfg(test)]
static TEST_PARSES: LazyLock<Mutex<HashMap<PathBuf, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
fn record_parse(path: &Path) {
    *TEST_PARSES.lock().entry(path.to_path_buf()).or_insert(0) += 1;
}

#[cfg(test)]
fn parse_count(path: &Path) -> usize {
    TEST_PARSES.lock().get(path).copied().unwrap_or(0)
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
        let hits = region_biome_hits(&path, region.mtime, region.x, region.z, max_y);
        for hit in hits.iter() {
            visit(&hit.biome, hit.x, hit.z);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::test_support::write_synthetic_region;

    #[test]
    fn list_biomes_counts_synthetic_chunk() {
        let dir = tempfile::tempdir().unwrap();
        write_synthetic_region(dir.path());

        let got = list_biomes(dir.path(), "", 319);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].biome, "minecraft:plains");
        assert_eq!(got[0].count, 1);
    }

    #[test]
    fn find_biome_returns_chunk_center() {
        let dir = tempfile::tempdir().unwrap();
        write_synthetic_region(dir.path());

        let hit = find_biome(dir.path(), "", 319, "plains", 0, 0).expect("应找到 plains");
        assert_eq!(hit.biome, "minecraft:plains");
        assert_eq!((hit.x, hit.z), (8, 8), "chunk (0,0) 中心坐标");
    }

    #[test]
    fn find_biome_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        write_synthetic_region(dir.path());
        assert!(find_biome(dir.path(), "", 319, "desert", 0, 0).is_none());
    }

    #[test]
    fn second_scan_reuses_cached_region_hits() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_synthetic_region(dir.path());

        let first = list_biomes(dir.path(), "", 319);
        let second = list_biomes(dir.path(), "", 319);

        assert_eq!(first.len(), second.len());
        assert_eq!(parse_count(&path), 1, "同一 region 不应重复解析");
    }

    #[test]
    fn find_biome_reuses_list_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_synthetic_region(dir.path());

        let _ = list_biomes(dir.path(), "", 319);
        let hit = find_biome(dir.path(), "", 319, "plains", 0, 0);

        assert!(hit.is_some());
        assert_eq!(parse_count(&path), 1, "list_biomes 与 find_biome 共享缓存");
    }

    #[test]
    fn cache_hit_returns_same_arc() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_synthetic_region(dir.path());
        let region = list_region_files(dir.path()).pop().unwrap();

        let a = region_biome_hits(&path, region.mtime, region.x, region.z, 319);
        let b = region_biome_hits(&path, region.mtime, region.x, region.z, 319);

        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(parse_count(&path), 1);
    }

    #[test]
    fn max_y_change_rescans_region() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_synthetic_region(dir.path());

        let _ = list_biomes(dir.path(), "", 319);
        let _ = list_biomes(dir.path(), "", 63);

        assert_eq!(parse_count(&path), 2, "max_y 变化必须重新扫描");
    }

    #[test]
    fn mtime_change_rescans_region() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_synthetic_region(dir.path());
        let _ = list_biomes(dir.path(), "", 319);

        // 显式推后 mtime，避免同秒保存命中旧条目
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(30))
            .unwrap();
        let _ = list_biomes(dir.path(), "", 319);

        assert_eq!(parse_count(&path), 2, "mtime 变化必须重新扫描");
    }

    #[test]
    fn empty_result_is_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let region_dir = dir.path().join("region");
        std::fs::create_dir_all(&region_dir).unwrap();
        let path = region_dir.join("r.0.0.mca");
        std::fs::write(&path, b"not a region").unwrap();

        assert!(list_biomes(dir.path(), "", 319).is_empty());
        assert!(list_biomes(dir.path(), "", 319).is_empty());
        assert_eq!(parse_count(&path), 2, "空结果不缓存，下次仍重试");
    }
}
