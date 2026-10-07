//! 已安装版本目录域：定位 / 枚举 / 建目录 / 重命名 / 删除。
//! 命令层（commands/version.rs / downloader.rs / gamedirs.rs / version_icon.rs）只做参数透传。
//! 基准 = downloader section 的 minecraft_dir（相对值按平台默认基准解析）。

use std::path::{Path, PathBuf};

use crate::config::downloader::DownloaderConfig;
use crate::launcher::args::find_version_json;
use crate::persist;
use crate::util::infer_loader_from_json;

/// 当前激活游戏目录（versions/ 父目录）绝对路径。
pub fn active_minecraft_dir() -> PathBuf {
    let config: DownloaderConfig =
        persist::read(crate::constants::SECT_DOWNLOADER).unwrap_or_default();
    config.minecraft_dir_abs()
}

/// 版本目录绝对路径（versions/{name}）。目录不存在时返回错误（读操作不该静默创建）。
pub fn version_dir_path(version_name: &str) -> Result<PathBuf, String> {
    let dir = active_minecraft_dir().join("versions").join(version_name);
    if !dir.exists() {
        return Err(format!("版本目录不存在\n{}", dir.display()));
    }
    Ok(dir)
}

/// 已安装版本信息（get_installed_versions 返回项）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledVersionInfo {
    pub name: String,
    /// 从版本 json 推断的 loader（fabric/forge/neoforge/optifine/vanilla）；
    /// json 读取失败（非版本目录）时为空串
    pub loader: String,
}

/// 枚举已安装版本（versions/ 下子目录），loader 从目录内 json 特征推断
/// （PCL2 实例目录名可能无 loader 标记）。按名称排序。
pub fn list_installed() -> Result<Vec<InstalledVersionInfo>, String> {
    let versions_dir = active_minecraft_dir().join("versions");
    if !versions_dir.exists() {
        return Ok(vec![]);
    }
    let mut items: Vec<InstalledVersionInfo> = std::fs::read_dir(&versions_dir)
        .map_err(|e| format!("读取版本目录失败\n{}", e))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // 从版本 json 推断 loader（PCL2 实例目录名可能无 loader 标记）
            let loader = find_version_json(&entry.path(), &name)
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|t| infer_loader_from_json(&t))
                .unwrap_or_default();
            Some(InstalledVersionInfo { name, loader })
        })
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(items)
}

/// 确保版本对应文件夹存在（不存在则创建），返回其绝对路径（快捷方式按钮用）。
/// 版本隔离：游戏运行时 gameDir = versions/{name}，存档/mod 实际都位于版本目录下。
pub fn ensure_kind_folder(kind: &str, version_name: &str) -> Result<String, String> {
    let base = active_minecraft_dir();
    let sub = match kind {
        "version" => format!("versions/{version_name}"),
        "saves" => format!("versions/{version_name}/saves"),
        "mods" => format!("versions/{version_name}/mods"),
        "resourcepacks" => format!("versions/{version_name}/resourcepacks"),
        "datapacks" => format!("versions/{version_name}/datapacks"),
        "shaderpacks" => format!("versions/{version_name}/shaderpacks"),
        _ => return Err("未知文件夹类型".to_string()),
    };
    let dir = base.join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败\n{e}"))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 重命名版本文件夹（versions/{old} → versions/{new}）。
pub fn rename_version_dir(version_name: &str, new_name: &str) -> Result<(), String> {
    let versions_dir = active_minecraft_dir().join("versions");
    let from = versions_dir.join(version_name);
    let to = versions_dir.join(new_name);
    if !from.exists() {
        return Err(format!("版本「{version_name}」不存在"));
    }
    if to.exists() {
        return Err(format!("版本「{new_name}」已存在"));
    }
    std::fs::rename(&from, &to).map_err(|e| format!("重命名失败\n{e}"))
}

/// 删除版本文件夹（递归删除 versions/{name}）。
/// 大目录删除耗时，命令层用 tokio::fs（内部 spawn_blocking）避免阻塞主线程。
pub async fn delete_version_dir(version_name: &str) -> Result<(), String> {
    let dir = active_minecraft_dir().join("versions").join(version_name);
    if !dir.exists() {
        return Err(format!("版本「{version_name}」不存在"));
    }
    tokio::fs::remove_dir_all(&dir)
        .await
        .map_err(|e| format!("删除失败\n{e}"))
}

/// 解析版本目录名：`versions/{id}` 冲突时追加 `_{n}`。
/// 目录存在但缺 client jar（上次下载中断）→ 复用目录重新下载，避免残留残缺版本。
pub fn resolve_version_dir(mc: &Path, version_id: &str) -> (String, PathBuf) {
    let base = mc.join("versions").join(version_id);
    if !base.exists() {
        return (version_id.to_string(), base);
    }
    if !base.join(format!("{version_id}.jar")).exists() {
        return (version_id.to_string(), base);
    }
    for n in 1.. {
        let name = format!("{}_{}", version_id, n);
        let dir = mc.join("versions").join(&name);
        if !dir.exists() {
            return (name, dir);
        }
    }
    // 循环为无限 RangeFrom（命中即 return），此处仅为满足类型检查
    unreachable!()
}
