use crate::util::default_game_dir_base;
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

/// 默认目录：title 固定「当前文件夹」，指向平台感知的默认基准下的 .minecraft
/// （Windows 便携：exe 旁；macOS/Linux：应用数据目录，见 util::default_game_dir_base）
pub fn default_dir() -> GameDir {
    let path = default_game_dir_base()
        .join(".minecraft")
        .to_string_lossy()
        .to_string();
    GameDir {
        name: "当前文件夹".to_string(),
        path,
    }
}
