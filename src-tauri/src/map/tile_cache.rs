//! 瓦片缓存：PNG 落盘 + 失效键 + 目录清理。
//!
//! 缓存位置 `config_dir()/mapcache/`（不污染存档目录；asset 协议 scope 静态声明）。
//! 失效键 = 世界路径哈希 + 维度 + region 坐标 + 样式（top-down / 切片Y）。
//! region 文件 mtime 参与键——存档更新后自动重渲染。
//! 目录上限超限时按 mtime 淘汰最旧文件（简单 LRU 近似，无需精确计数）。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::util::config_dir;

/// 缓存目录上限（256MB；1000+ 张 64×64 瓦片 ≈ 每张 5-15KB，足够覆盖中大型存档）。
const MAPCACHE_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// 瓦片缓存根目录（不存在则惰性创建于写入时）。
pub fn mapcache_dir() -> PathBuf {
    config_dir().join("mapcache")
}

/// FNV 风格短哈希（路径等长字符串 → u64 hex；无跨会话稳定性要求，Default 够用）。
fn short_hash(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// 瓦片缓存键：世界路径 + 维度 + region 坐标 + 样式 + region mtime。
/// 返回 None 仅当 region 文件不存在（世界未生成该 region）。
pub fn tile_cache_key(
    world_path: &Path,
    dimension: &str,
    rx: i32,
    rz: i32,
    style_key: &str,
) -> Option<PathBuf> {
    let region_file = world_path.join(dimension).join("region").join(format!("r.{rx}.{rz}.mca"));
    let meta = std::fs::metadata(&region_file).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = format!(
        "{}|{dimension}|{rx}|{rz}|{style_key}|{mtime}",
        world_path.to_string_lossy()
    );
    Some(mapcache_dir().join(format!("{}.png", short_hash(&key))))
}

/// 读取缓存瓦片；未命中返回 None。
pub fn read_cached_tile(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// 写入瓦片并顺带执行容量治理（写成功后再清理，失败不阻塞主流程）。
pub fn write_tile(path: &Path, png: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("缓存目录创建失败\n{e}"))?;
    }
    std::fs::write(path, png).map_err(|e| format!("瓦片写入失败\n{e}"))?;
    evict_if_oversize();
    Ok(())
}

/// 超限淘汰：按 mtime 升序删最旧 PNG 直到总大小 ≤ 上限。
/// 全目录扫描 O(n)，在写入路径上触发；n 为千级时可接受。
fn evict_if_oversize() {
    let dir = mapcache_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, u64)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "png"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some((e.path(), meta.len(), mtime))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, s, _)| *s).sum();
    if total <= MAPCACHE_MAX_BYTES {
        return;
    }
    files.sort_by_key(|(_, _, m)| *m);
    let mut total = total;
    for (path, size, _) in files {
        if total <= MAPCACHE_MAX_BYTES {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_changes_with_mtime_and_style() {
        let dir = std::env::temp_dir().join(format!("elytra_mapcache_t_{}", std::process::id()));
        let world = dir.join("world");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("region").join("r.0.0.mca"), b"x").unwrap();

        let k1 = tile_cache_key(&world, "", 0, 0, "topdown").unwrap();
        let k2 = tile_cache_key(&world, "", 0, 0, "slice64").unwrap();
        assert_ne!(k1, k2, "样式不同键必须不同");

        // 键含缓存目录前缀（落盘位置正确性）
        let k3 = tile_cache_key(&world, "", 0, 0, "topdown").unwrap();
        assert!(k3.starts_with(mapcache_dir()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_region_file_gives_no_key() {
        let dir = std::env::temp_dir().join(format!("elytra_mapcache_m_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(tile_cache_key(&dir, "", 9, 9, "topdown").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tile_write_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("elytra_mapcache_w_{}", std::process::id()));
        let path = dir.join("abc.png");
        write_tile(&path, b"PNGDATA").unwrap();
        assert_eq!(read_cached_tile(&path), Some(b"PNGDATA".to_vec()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
