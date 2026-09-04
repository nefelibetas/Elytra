//! 地图编辑器命令（薄胶水）：领域逻辑在 map/ 域模块。

use std::path::Path;

use crate::map::tree::{build_tree, MapDirEntry};

/// 地图编辑器三级树（游戏目录 → 版本 → 存档），一次性返回全树。
#[tauri::command]
pub fn get_map_tree() -> Vec<MapDirEntry> {
    build_tree()
}

/// 世界缩略图（编辑器按【绝对路径】锚定读取，区别于主窗口按激活目录的 get_world_icon）。
/// 复用 util::icon_payload（魔数嗅探 + base64）；缺失/超限/损坏返回 None。
/// version/world 仅接受简单目录名（防路径穿越）；dir_path 为完整游戏目录（含分隔符）。
#[tauri::command]
pub fn get_map_world_icon(
    dir_path: String,
    version_name: String,
    world_name: String,
) -> Option<crate::util::IconData> {
    let is_simple = |s: &str| !s.is_empty() && !s.contains(['/', '\\', ':']) && !s.contains("..");
    if !is_simple(&version_name) || !is_simple(&world_name) {
        return None;
    }
    let icon_path = Path::new(&dir_path)
        .join("versions")
        .join(&version_name)
        .join("saves")
        .join(&world_name)
        .join("icon.png");
    crate::util::icon_payload(&icon_path, 512 * 1024)
}
