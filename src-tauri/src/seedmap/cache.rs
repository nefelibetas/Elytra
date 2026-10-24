use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::map::tile_cache::{mapcache_dir, read_cached_tile, write_tile};

const RENDER_VERSION: &str = "seedmap-v1-cubiomes-d934de1";

pub fn tile_cache_path(version: &str, seed: i64, rx: i32, rz: i32) -> PathBuf {
    let key = format!("{RENDER_VERSION}|{version}|{seed}|{rx}|{rz}");
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    mapcache_dir().join(format!("{:016x}.png", hasher.finish()))
}

pub fn read_tile(path: &Path) -> Option<Vec<u8>> {
    read_cached_tile(path)
}

pub fn write_tile_png(path: &Path, png: &[u8]) -> Result<(), String> {
    write_tile(path, png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_varies_by_seed_and_version() {
        let a = tile_cache_path("26.3", 1, 0, 0);
        let b = tile_cache_path("26.3", 2, 0, 0);
        let c = tile_cache_path("1.20.6", 1, 0, 0);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with(mapcache_dir()));
    }
}
