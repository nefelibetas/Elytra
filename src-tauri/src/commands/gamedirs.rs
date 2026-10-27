use tauri_plugin_opener::OpenerExt;

use crate::commands::version::VersionServer;
use crate::config::game_dirs::{self, GameDir};
use crate::game::version::{dir, watch};

/// 游戏目录列表（空时自动生成默认目录）。
#[tauri::command]
pub fn get_game_dirs() -> Vec<GameDir> {
    game_dirs::ensure_initialized()
}

#[tauri::command]
pub fn add_game_dir(name: String, path: String) -> Result<Vec<GameDir>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(keyed_msg!("errors.input.nameEmpty", "名称不能为空"));
    }
    let mut dirs = game_dirs::ensure_initialized();
    if dirs.iter().any(|d| d.name == name) {
        bail_key!(
            "errors.gamedirs.nameExists?name={name}",
            "名称「{name}」已存在"
        );
    }
    if dirs.iter().any(|d| d.path == path) {
        return Err(keyed_msg!(
            "errors.gamedirs.pathDuplicated",
            "该路径已在列表中"
        ));
    }
    dirs.push(GameDir { name, path });
    game_dirs::save_dirs(dirs)
}

#[tauri::command]
pub fn rename_game_dir(path: String, new_name: String) -> Result<Vec<GameDir>, String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err(keyed_msg!("errors.input.nameEmpty", "名称不能为空"));
    }
    let mut dirs = game_dirs::ensure_initialized();
    let idx = dirs
        .iter()
        .position(|d| d.path == path)
        .ok_or_else(|| keyed_msg!("errors.gamedirs.dirMissing", "文件夹不存在"))?;
    if dirs
        .iter()
        .enumerate()
        .any(|(i, d)| d.name == new_name && i != idx)
    {
        bail_key!(
            "errors.gamedirs.nameExists?name={new_name}",
            "名称「{new_name}」已存在"
        );
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
    game_dirs::set_active(&path)?;
    // 监听目标跟随激活目录：`versions/` 与聚焦版本路径整体变化
    watch::retarget();
    Ok(())
}

/// 设置聚焦版本（前端选中版本变化时调用）：决定监听哪些版本子目录，不影响启动。
#[tauri::command]
pub fn set_watched_version(version: Option<String>) -> Result<(), String> {
    watch::set_focus(version);
    Ok(())
}

#[tauri::command]
pub fn get_active_game_dir() -> GameDir {
    game_dirs::active_dir()
}

#[tauri::command]
pub fn rename_version_dir(version_name: String, new_name: String) -> Result<(), String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err(keyed_msg!("errors.input.nameEmpty", "名称不能为空"));
    }
    if new_name == version_name {
        return Err(keyed_msg!(
            "errors.gamedirs.nameUnchanged",
            "新名称与当前名称相同"
        ));
    }
    dir::rename_version_dir(&version_name, &new_name)?;
    // 版本改名后迁移快速进服配置；失败不反转已完成的目录改名，但保留旧 section 便于排查。
    if let (Ok(old_section), Ok(new_section)) = (
        dir::version_server_section(&version_name),
        dir::version_server_section(&new_name),
    ) {
        if let Ok(cfg) = crate::persist::read::<VersionServer>(&old_section) {
            if crate::persist::save(&new_section, &cfg).is_ok() {
                let _ = crate::persist::remove(&old_section);
            }
        }
    }
    Ok(())
}

/// 用系统文件管理器打开指定目录（不受 JS scope 限制）；打开前校验存在，避免 explorer 回退打开"文档"。
#[tauri::command]
pub fn open_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        bail_key!("errors.dir.missing", "目录不存在\n{path}");
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(common_err_ctx_key!("errors.unknown", "打开文件夹失败"))
}

/// 激活第一个含版本的目录并返回其第一个版本（目录按配置顺序，版本按字母序）。
#[tauri::command]
pub fn auto_activate_version() -> Result<dir::AutoActivate, String> {
    let activated = dir::auto_activate_first_version()?;
    // 内部已切换激活目录：监听目标跟随
    watch::retarget();
    Ok(activated)
}

/// 确保版本文件夹存在（不存在则创建），返回绝对路径；版本隔离：gameDir = versions/{name}。
#[tauri::command]
pub fn ensure_game_folder(kind: String, version_name: String) -> Result<String, String> {
    dir::ensure_kind_folder(&kind, &version_name)
}

/// 删除版本文件夹（递归 versions/{name}）；大目录耗时，async 走 tokio 的 spawn_blocking。
#[tauri::command]
pub async fn delete_version_dir(version_name: String) -> Result<(), String> {
    dir::delete_version_dir(&version_name).await
}
