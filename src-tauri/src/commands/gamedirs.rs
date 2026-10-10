use tauri_plugin_opener::OpenerExt;

use crate::config::game_dirs::{self, GameDir};
use crate::game::version::dir;

/// 游戏目录列表（空时自动生成默认目录：启动器文件夹名 + .minecraft）。
#[tauri::command]
pub fn get_game_dirs() -> Vec<GameDir> {
    game_dirs::ensure_initialized()
}

/// 添加已有文件夹（校验：名称与路径均不可重复）。
#[tauri::command]
pub fn add_game_dir(name: String, path: String) -> Result<Vec<GameDir>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let mut dirs = game_dirs::ensure_initialized();
    if dirs.iter().any(|d| d.name == name) {
        bail!("名称「{name}」已存在");
    }
    if dirs.iter().any(|d| d.path == path) {
        return Err("该路径已在列表中".to_string());
    }
    dirs.push(GameDir { name, path });
    game_dirs::save_dirs(dirs)
}

/// 重命名目录（校验：非空、不与其余项重名）。
#[tauri::command]
pub fn rename_game_dir(path: String, new_name: String) -> Result<Vec<GameDir>, String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let mut dirs = game_dirs::ensure_initialized();
    let idx = dirs
        .iter()
        .position(|d| d.path == path)
        .ok_or_else(|| "文件夹不存在".to_string())?;
    if dirs
        .iter()
        .enumerate()
        .any(|(i, d)| d.name == new_name && i != idx)
    {
        bail!("名称「{new_name}」已存在");
    }
    dirs[idx].name = new_name;
    game_dirs::save_dirs(dirs)
}

/// 从配置中删除目录（不删除实际文件）。
#[tauri::command]
pub fn remove_game_dir(path: String) -> Result<Vec<GameDir>, String> {
    let mut dirs = game_dirs::ensure_initialized();
    dirs.retain(|d| d.path != path);
    game_dirs::save_dirs(dirs)
}

/// 设置当前激活的游戏目录（同步到 downloader 配置的 minecraft_dir，版本列表跟随切换）。
#[tauri::command]
pub fn set_active_game_dir(path: String) -> Result<(), String> {
    game_dirs::set_active(&path)
}

/// 当前激活的游戏目录（同步于 downloader 配置的 minecraft_dir）。
#[tauri::command]
pub fn get_active_game_dir() -> GameDir {
    game_dirs::active_dir()
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
    dir::rename_version_dir(&version_name, &new_name)
}

/// 用系统文件管理器打开指定目录（tauri-plugin-opener Rust 侧 API，不受 JS scope 限制；
/// 打开前校验存在，避免 explorer 对无效路径回退打开"文档"）。
#[tauri::command]
pub fn open_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        bail!("目录不存在\n{path}");
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(common_err_ctx!("打开文件夹失败"))
}

/// 检测所有已配置的游戏目录，激活第一个存在游戏版本的目录，并返回其第一个版本。
/// 版本名按字母序取第一个；目录按配置列表顺序检测。
#[tauri::command]
pub fn auto_activate_version() -> Result<dir::AutoActivate, String> {
    dir::auto_activate_first_version()
}

/// 确保版本对应文件夹存在（不存在则创建），返回其绝对路径（快捷方式按钮用）。
/// 版本隔离：游戏运行时 gameDir = versions/{name}，存档/mod 实际都位于版本目录下。
/// 实际操作在 downloader/version.rs。
#[tauri::command]
pub fn ensure_game_folder(kind: String, version_name: String) -> Result<String, String> {
    dir::ensure_kind_folder(&kind, &version_name)
}

/// 删除版本文件夹（递归删除 versions/{name}；实际操作在 downloader/version.rs，
/// 大目录删除耗时，命令保持 async 走 tokio::fs 的 spawn_blocking）。
#[tauri::command]
pub async fn delete_version_dir(version_name: String) -> Result<(), String> {
    dir::delete_version_dir(&version_name).await
}
