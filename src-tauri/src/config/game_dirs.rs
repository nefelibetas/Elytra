use crate::config::downloader::DownloaderConfig;
use crate::persist::{
    downloader_config, game_dirs_config, save_downloader_config, save_game_dirs_config,
};
use crate::util::default_game_dir_base;
use serde::{Deserialize, Serialize};

/// 一个游戏目录（.minecraft 文件夹）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export_to = "config/")]
pub struct GameDir {
    /// 列表显示名称
    pub name: String,
    /// 完整路径
    pub path: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "config/")]
pub struct GameDirsConfig {
    pub dirs: Vec<GameDir>,
}

/// 默认目录：title 固定「当前文件夹」，指向平台默认基准下的 `.minecraft`。
/// `None` = 该平台无合适默认（见 `util::default_game_dir_base`），此时列表保持为空，
/// 由前端引导用户自行添加游戏目录。
pub fn default_dir() -> Option<GameDir> {
    let base = default_game_dir_base()?;
    Some(GameDir {
        name: "当前文件夹".to_string(),
        path: base.join(".minecraft").to_string_lossy().to_string(),
    })
}

/// 读取已配置的游戏目录列表（纯读，不产生写盘副作用）。
pub fn read_dirs() -> Vec<GameDir> {
    game_dirs_config().dirs
}

/// 确保目录列表已初始化：为空且存在平台默认基准时，生成默认目录并持久化。
/// 无默认基准（macOS/Linux 等）时**保持空列表**，由前端引导用户显式添加——
/// 此时不写盘，下次调用仍返回空。
/// 写盘失败时仍返回内存目录，但下次启动会重复生成 —— 需可诊断。
pub fn ensure_initialized() -> Vec<GameDir> {
    let cfg: GameDirsConfig = game_dirs_config();
    if !cfg.dirs.is_empty() {
        return cfg.dirs;
    }
    let Some(d) = default_dir() else {
        return Vec::new();
    };
    if let Err(e) = save_game_dirs_config(&GameDirsConfig {
        dirs: vec![d.clone()],
    }) {
        eprintln!("[gamedirs] 默认游戏目录持久化失败: {e}");
    }
    vec![d]
}

/// 保存目录列表到配置。
pub fn save_dirs(dirs: Vec<GameDir>) -> Result<Vec<GameDir>, String> {
    save_game_dirs_config(&GameDirsConfig { dirs: dirs.clone() })
        .map_err(common_err_ctx!("保存配置失败"))?;
    Ok(dirs)
}

/// 设置当前激活的游戏目录：写入 downloader 配置的 minecraft_dir（版本列表跟随切换）。
pub fn set_active(path: &str) -> Result<(), String> {
    let mut cfg: DownloaderConfig = downloader_config();
    cfg.minecraft_dir = path.to_string();
    save_downloader_config(&cfg).map_err(common_err_ctx!("保存配置失败"))
}

/// 当前激活的游戏目录（同步于 downloader 配置的 `minecraft_dir`）。
/// 无匹配项时回退到平台默认目录；连默认都没有（macOS/Linux 且用户尚未添加任何目录）
/// 则返回**空条目**（name/path 均空）——前端据此显示「请添加游戏目录」引导，
/// 而不是指向一个并不存在的路径。
pub fn active_dir() -> GameDir {
    let cfg: DownloaderConfig = downloader_config();
    read_dirs()
        .into_iter()
        .find(|d| d.path == cfg.minecraft_dir)
        .or_else(default_dir)
        .unwrap_or(GameDir {
            name: String::new(),
            path: String::new(),
        })
}
