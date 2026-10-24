use std::path::{Path, PathBuf};

use serde::Serialize;

/// 递归发现的世界根目录（含 `level.dat` 或 26.x `world_gen_settings.dat`）。
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapFolderEntry {
    pub root: String,
    pub name: String,
}

const MAX_DEPTH: usize = 4;
const MAX_WORLDS: usize = 64;

/// 从用户选择的文件夹递归查找世界根目录；选中目录本身是世界时只返回它。
pub fn discover_worlds(path: &Path) -> Result<Vec<MapFolderEntry>, String> {
    if !path.is_dir() {
        return Err("文件夹不存在\n请重新选择".to_string());
    }
    let mut roots = Vec::new();
    walk(path, path, MAX_DEPTH, &mut roots);
    if roots.is_empty() {
        return Err("未找到地图\n请选择含 level.dat 的存档文件夹".to_string());
    }
    Ok(roots)
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<MapFolderEntry>) {
    if out.len() >= MAX_WORLDS {
        return;
    }
    if is_world_root(dir) {
        out.push(entry(root, dir));
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let ty = entry.file_type().ok()?;
            // 符号链接可能指向存档外或形成环，递归不跟随。
            ty.is_dir().then(|| entry.path())
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        if out.len() >= MAX_WORLDS {
            return;
        }
        if is_noise_dir(&dir) {
            continue;
        }
        walk(root, &dir, depth - 1, out);
    }
}

fn is_world_root(dir: &Path) -> bool {
    dir.join("level.dat").is_file()
        || dir
            .join("data")
            .join("minecraft")
            .join("world_gen_settings.dat")
            .is_file()
}

fn is_noise_dir(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    matches!(
        name,
        "region"
            | "DIM-1"
            | "DIM1"
            | "data"
            | "poi"
            | "entities"
            | "libraries"
            | "assets"
            | "logs"
            | "crash-reports"
            | "screenshots"
            | "resourcepacks"
            | "shaderpacks"
            | "versions"
            | "mods"
            | "config"
            | "downloads"
            | "runtime"
            | "bundles"
            | "defaultconfigs"
    )
}

fn entry(root: &Path, dir: &Path) -> MapFolderEntry {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let name = if rel.as_os_str().is_empty() {
        dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("world")
            .to_string()
    } else {
        rel.to_string_lossy().replace('\\', "/")
    };
    MapFolderEntry {
        root: dir.to_string_lossy().into_owned(),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_nested_worlds_and_skips_noise() {
        let dir = std::env::temp_dir().join(format!("elytra_map_folder_{}", std::process::id()));
        let saves = dir.join("saves");
        let world = saves.join("world");
        let modern = saves.join("modern");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::create_dir_all(modern.join("data/minecraft")).unwrap();
        std::fs::create_dir_all(dir.join("libraries/foo")).unwrap();
        std::fs::write(world.join("level.dat"), b"x").unwrap();
        std::fs::write(modern.join("data/minecraft/world_gen_settings.dat"), b"x").unwrap();
        std::fs::write(dir.join("libraries/foo/level.dat"), b"x").unwrap();

        let roots = discover_worlds(&dir).unwrap();
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().any(|r| r.name == "saves/world"));
        assert!(roots.iter().any(|r| r.name == "saves/modern"));
        assert!(roots.iter().all(|r| !r.root.contains("libraries")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn selected_world_root_returns_itself() {
        let dir = std::env::temp_dir().join(format!("elytra_map_root_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("region")).unwrap();
        std::fs::write(dir.join("level.dat"), b"x").unwrap();
        let roots = discover_worlds(&dir).unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].root, dir.to_string_lossy());
        std::fs::remove_dir_all(&dir).ok();
    }
}
