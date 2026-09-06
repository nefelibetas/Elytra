use crate::config::game_dirs::{default_dir, GameDir, GameDirsConfig};
use crate::constants;

fn read_dirs() -> Vec<GameDir> {
    let cfg: GameDirsConfig = crate::persist::read(constants::SECT_GAME_DIRS).unwrap_or_default();
    if cfg.dirs.is_empty() {
        // 首次使用（或全部删除后）：生成默认目录（exe 同级 .minecraft）并持久化
        let d = default_dir();
        let _ = crate::persist::save(
            constants::SECT_GAME_DIRS,
            &GameDirsConfig {
                dirs: vec![d.clone()],
            },
        );
        vec![d]
    } else {
        cfg.dirs
    }
}

fn save_dirs(dirs: Vec<GameDir>) -> Result<Vec<GameDir>, String> {
    crate::persist::save(
        constants::SECT_GAME_DIRS,
        &GameDirsConfig { dirs: dirs.clone() },
    )
    .map_err(|e| format!("保存配置失败\n{e}"))?;
    Ok(dirs)
}

/// 游戏目录列表（空时自动生成默认目录：启动器文件夹名 + .minecraft）。
#[tauri::command]
pub fn get_game_dirs() -> Vec<GameDir> {
    read_dirs()
}

/// 添加已有文件夹（校验：名称与路径均不可重复）。
#[tauri::command]
pub fn add_game_dir(name: String, path: String) -> Result<Vec<GameDir>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let mut dirs = read_dirs();
    if dirs.iter().any(|d| d.name == name) {
        return Err(format!("名称「{name}」已存在"));
    }
    if dirs.iter().any(|d| d.path == path) {
        return Err("该路径已在列表中".to_string());
    }
    dirs.push(GameDir { name, path });
    save_dirs(dirs)
}

/// 重命名目录（校验：非空、不与其余项重名）。
#[tauri::command]
pub fn rename_game_dir(path: String, new_name: String) -> Result<Vec<GameDir>, String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let mut dirs = read_dirs();
    let idx = dirs
        .iter()
        .position(|d| d.path == path)
        .ok_or_else(|| "文件夹不存在".to_string())?;
    if dirs
        .iter()
        .enumerate()
        .any(|(i, d)| d.name == new_name && i != idx)
    {
        return Err(format!("名称「{new_name}」已存在"));
    }
    dirs[idx].name = new_name;
    save_dirs(dirs)
}

/// 从配置中删除目录（不删除实际文件）。
#[tauri::command]
pub fn remove_game_dir(path: String) -> Result<Vec<GameDir>, String> {
    let mut dirs = read_dirs();
    dirs.retain(|d| d.path != path);
    save_dirs(dirs)
}

/// 设置当前激活的游戏目录（同步到 downloader 配置的 minecraft_dir，版本列表跟随切换）。
#[tauri::command]
pub fn set_active_game_dir(path: String) -> Result<(), String> {
    let mut cfg: crate::config::downloader::model::DownloaderConfig =
        crate::persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    cfg.minecraft_dir = path;
    crate::persist::save(constants::SECT_DOWNLOADER, &cfg).map_err(|e| format!("保存配置失败\n{e}"))
}

/// 当前激活的游戏目录（同步于 downloader 配置的 minecraft_dir）。
#[tauri::command]
pub fn get_active_game_dir() -> GameDir {
    let cfg: crate::config::downloader::model::DownloaderConfig =
        crate::persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let dirs = read_dirs();
    dirs.into_iter()
        .find(|d| d.path == cfg.minecraft_dir)
        .unwrap_or_else(default_dir)
}

/// 重命名版本文件夹（versions/{old} → versions/{new}；实际操作在 downloader/version.rs）。
#[tauri::command]
pub fn rename_version_dir(version_name: String, new_name: String) -> Result<(), String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    if new_name == version_name {
        return Err("新名称与当前名称相同".to_string());
    }
    crate::downloader::version::rename_version_dir(&version_name, &new_name)
}

/// 用系统文件管理器打开指定目录（tauri-plugin-opener Rust 侧 API，不受 JS scope 限制；
/// 打开前校验存在，避免 explorer 对无效路径回退打开"文档"）。
#[tauri::command]
pub fn open_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err(format!("目录不存在\n{path}"));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("打开文件夹失败\n{e}"))
}

/// 自动激活结果：激活的目录路径 + 该目录下第一个版本名。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoActivate {
    pub dir_path: String,
    pub version: String,
}

/// 检测所有已配置的游戏目录，激活第一个存在游戏版本的目录，并返回其第一个版本。
/// 版本名按字母序取第一个；目录按配置列表顺序检测。
#[tauri::command]
pub fn auto_activate_version() -> Result<AutoActivate, String> {
    let dirs = read_dirs();
    for d in &dirs {
        let vdir = std::path::Path::new(&d.path).join("versions");
        if !vdir.is_dir() {
            continue;
        }
        let mut names: Vec<String> = std::fs::read_dir(&vdir)
            .map_err(|e| format!("读取目录失败\n{e}"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        if names.is_empty() {
            continue;
        }
        names.sort();
        let version = names[0].clone();
        set_active_game_dir(d.path.clone())?;
        return Ok(AutoActivate {
            dir_path: d.path.clone(),
            version,
        });
    }
    Err("所有游戏目录中都没有已安装的版本".to_string())
}

/// 确保版本对应文件夹存在（不存在则创建），返回其绝对路径（快捷方式按钮用）。
/// 版本隔离：游戏运行时 gameDir = versions/{name}，存档/mod 实际都位于版本目录下。
/// 实际操作在 downloader/version.rs。
#[tauri::command]
pub fn ensure_game_folder(kind: String, version_name: String) -> Result<String, String> {
    crate::downloader::version::ensure_kind_folder(&kind, &version_name)
}

/// 删除版本文件夹（递归删除 versions/{name}；实际操作在 downloader/version.rs，
/// 大目录删除耗时，命令保持 async 走 tokio::fs 的 spawn_blocking）。
#[tauri::command]
pub async fn delete_version_dir(version_name: String) -> Result<(), String> {
    crate::downloader::version::delete_version_dir(&version_name).await
}
