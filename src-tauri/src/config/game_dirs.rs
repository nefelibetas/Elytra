use crate::config::downloader::DownloaderConfig;
use crate::constants;
use crate::persist;
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

/// 读取已配置的游戏目录列表（纯读，不产生写盘副作用）。
pub fn read_dirs() -> Vec<GameDir> {
    persist::read::<GameDirsConfig>(constants::SECT_GAME_DIRS)
        .unwrap_or_default()
        .dirs
}

/// 确保目录列表已初始化：为空时生成默认目录（exe 同级 .minecraft）并持久化，返回当前列表。
/// 写盘失败时仍返回内存目录，但下次启动会重复生成 —— 需可诊断。
pub fn ensure_initialized() -> Vec<GameDir> {
    let cfg: GameDirsConfig = persist::read(constants::SECT_GAME_DIRS).unwrap_or_default();
    if !cfg.dirs.is_empty() {
        return cfg.dirs;
    }
    let d = default_dir();
    if let Err(e) = persist::save(
        constants::SECT_GAME_DIRS,
        &GameDirsConfig {
            dirs: vec![d.clone()],
        },
    ) {
        eprintln!("[gamedirs] 默认游戏目录持久化失败: {e}");
    }
    vec![d]
}

/// 保存目录列表到配置。
pub fn save_dirs(dirs: Vec<GameDir>) -> Result<Vec<GameDir>, String> {
    persist::save(
        constants::SECT_GAME_DIRS,
        &GameDirsConfig { dirs: dirs.clone() },
    )
    .map_err(|e| format!("保存配置失败\n{e}"))?;
    Ok(dirs)
}

/// 设置当前激活的游戏目录：写入 downloader 配置的 minecraft_dir（版本列表跟随切换）。
pub fn set_active(path: &str) -> Result<(), String> {
    let mut cfg: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    cfg.minecraft_dir = path.to_string();
    persist::save(constants::SECT_DOWNLOADER, &cfg).map_err(|e| format!("保存配置失败\n{e}"))
}

/// 当前激活的游戏目录（同步于 downloader 配置的 minecraft_dir）；无匹配时回退默认目录。
pub fn active_dir() -> GameDir {
    let cfg: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    read_dirs()
        .into_iter()
        .find(|d| d.path == cfg.minecraft_dir)
        .unwrap_or_else(default_dir)
}
