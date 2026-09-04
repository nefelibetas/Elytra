//! 地图编辑器三级树：游戏目录（1 级）→ 版本（2 级）→ 存档（3 级）。
//! 目录列表复用 gamedirs 配置；版本/存档扫描复用 downloader / version 的领域逻辑。
//! 不读"当前激活目录"（编辑器窗口可浏览任意已配置目录）。

use std::path::Path;

use serde::Serialize;

use crate::commands::gamedirs::read_dirs;
use crate::launcher::args::find_version_json;
use crate::util::{dir_entries, infer_loader_from_json};

/// 树第 3 级：单个存档概要。
#[derive(Debug, Clone, Serialize)]
pub struct MapWorldEntry {
    /// 存档文件夹名
    pub name: String,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

/// 树第 2 级：版本（含其下存档）。
#[derive(Debug, Clone, Serialize)]
pub struct MapVersionEntry {
    /// 版本目录名
    pub name: String,
    /// 版本 json 推断的 loader（vanilla/fabric/forge/neoforge/optifine；读取失败为空串）
    pub loader: String,
    /// 存档列表（按最后修改时间倒序）
    pub worlds: Vec<MapWorldEntry>,
}

/// 树第 1 级：游戏目录（.minecraft 文件夹）。
#[derive(Debug, Clone, Serialize)]
pub struct MapDirEntry {
    /// 目录显示名
    pub name: String,
    /// 目录完整路径（子级定位锚点，亦是编辑器内定位存档绝对路径的依据）
    pub path: String,
    /// 该目录下的版本列表（按名称排序）
    pub versions: Vec<MapVersionEntry>,
}

/// 扫描单个版本目录下的存档（saves 子目录，按最后修改时间倒序）。
/// 与 commands::version::list_worlds 同规则；此处以目录树给定的版本路径为准。
fn scan_worlds(version_path: &Path) -> Vec<MapWorldEntry> {
    let saves = version_path.join("saves");
    let mut worlds: Vec<MapWorldEntry> = dir_entries(&saves, true)
        .into_iter()
        .map(|(name, _, last_modified)| MapWorldEntry { name, last_modified })
        .collect();
    worlds.sort_by_key(|w| std::cmp::Reverse(w.last_modified));
    worlds
}

/// 扫描单个游戏目录下的版本列表（json 推断 loader，按名称排序）。
fn scan_versions(dir_path: &str) -> Vec<MapVersionEntry> {
    let versions_dir = Path::new(dir_path).join("versions");
    let mut versions: Vec<MapVersionEntry> = dir_entries(&versions_dir, true)
        .into_iter()
        .map(|(name, _, _)| {
            let version_path = versions_dir.join(&name);
            let loader = find_version_json(&version_path, &name)
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| infer_loader_from_json(&t))
                .unwrap_or_default();
            MapVersionEntry {
                name,
                loader,
                worlds: scan_worlds(&version_path),
            }
        })
        .collect();
    versions.sort_by(|a, b| a.name.cmp(&b.name));
    versions
}

/// 地图编辑器三级树：所有已配置游戏目录 → 各自版本 → 各自存档。
/// 单次调用返回全树（配合单条 IPC 命令，避免编辑器窗口逐级展开时的请求风暴）；
/// 目录/版本/存档任一层为空则该分支自然收窄（前端渲染为空组）。
pub fn build_tree() -> Vec<MapDirEntry> {
    read_dirs()
        .into_iter()
        .map(|dir| {
            let versions = scan_versions(&dir.path);
            MapDirEntry {
                name: dir.name,
                path: dir.path,
                versions,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// scan_versions 按版本名排序（行为不变量）；空目录/无 saves 不 panic。
    #[test]
    fn scan_versions_sorts_by_name() {
        let dir = std::env::temp_dir().join(format!("elytra_map_tree_test_{}", std::process::id()));
        let versions = dir.join("versions");
        for name in ["1.20.1-fabric", "1.21", "1.12.2"] {
            std::fs::create_dir_all(versions.join(name)).unwrap();
        }
        let mut got = scan_versions(&dir.to_string_lossy());
        got.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<&str> = got.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["1.12.2", "1.20.1-fabric", "1.21"], "版本应按名称排序");
        assert!(got.iter().all(|v| v.worlds.is_empty()), "无 saves 目录应得到空存档列表");
        std::fs::remove_dir_all(&dir).ok();
    }
}
