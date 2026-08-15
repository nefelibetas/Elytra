use crate::config::game_dirs::{default_dir, exe_dir_name, GameDir, GameDirsConfig};
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
        let mut dirs = cfg.dirs;
        // 迁移旧默认项：默认路径上仍叫 exe 目录名（用户未重命名过）的项 → 「当前文件夹」
        let default = default_dir();
        let old_name = exe_dir_name();
        if !old_name.is_empty() {
            if let Some(d) = dirs
                .iter_mut()
                .find(|d| d.path == default.path && d.name == old_name)
            {
                d.name = "当前文件夹".to_string();
                let _ = crate::persist::save(constants::SECT_GAME_DIRS, &GameDirsConfig { dirs: dirs.clone() });
            }
        }
        dirs
    }
}

fn save_dirs(dirs: Vec<GameDir>) -> Result<Vec<GameDir>, String> {
    crate::persist::save(constants::SECT_GAME_DIRS, &GameDirsConfig { dirs: dirs.clone() })
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

/// 重命名版本文件夹（versions/{old} → versions/{new}）。
#[tauri::command]
pub fn rename_version_dir(version_name: String, new_name: String) -> Result<(), String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    if new_name == version_name {
        return Err("新名称与当前名称相同".to_string());
    }
    let active = get_active_game_dir();
    let versions_dir = std::path::Path::new(&active.path).join("versions");
    let from = versions_dir.join(&version_name);
    let to = versions_dir.join(&new_name);
    if !from.exists() {
        return Err(format!("版本「{version_name}」不存在"));
    }
    if to.exists() {
        return Err(format!("版本「{new_name}」已存在"));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("重命名失败\n{e}"))
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
#[tauri::command]
pub fn ensure_game_folder(kind: String, version_name: String) -> Result<String, String> {
    let active = get_active_game_dir();
    let base = std::path::Path::new(&active.path);
    let sub = match kind.as_str() {
        "version" => format!("versions/{version_name}"),
        "saves" => format!("versions/{version_name}/saves"),
        "mods" => format!("versions/{version_name}/mods"),
        _ => return Err("未知文件夹类型".to_string()),
    };
    let dir = base.join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败\n{e}"))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 删除版本文件夹（递归删除 versions/{name}）。
/// 大目录删除耗时，用 tokio::fs（内部 spawn_blocking）避免阻塞主线程。
#[tauri::command]
pub async fn delete_version_dir(version_name: String) -> Result<(), String> {
    let active = get_active_game_dir();
    let dir = std::path::Path::new(&active.path)
        .join("versions")
        .join(&version_name);
    if !dir.exists() {
        return Err(format!("版本「{version_name}」不存在"));
    }
    tokio::fs::remove_dir_all(&dir)
        .await
        .map_err(|e| format!("删除失败\n{e}"))
}
