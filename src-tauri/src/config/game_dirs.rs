use crate::util::exe_dir;
use serde::{Deserialize, Serialize};

/// 一个游戏目录（.minecraft 文件夹）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameDir {
    /// 列表显示名称
    pub name: String,
    /// 完整路径
    pub path: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GameDirsConfig {
    pub dirs: Vec<GameDir>,
}

/// 启动器 exe 所在文件夹名（旧版默认项命名，用于迁移）。
pub fn exe_dir_name() -> String {
    exe_dir()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 默认目录：title 固定「当前文件夹」，指向启动器 exe 同级 .minecraft
pub fn default_dir() -> GameDir {
    let path = exe_dir().join(".minecraft").to_string_lossy().to_string();
    GameDir {
        name: "当前文件夹".to_string(),
        path,
    }
}
