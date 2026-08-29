//! 版本内容命令（薄胶水）：存档/Mod 本地扫描、Mod 更新检查与更新下载、整合包导出。
//! 目录定位复用 gamedirs 的"当前激活游戏目录 + versions/{name}"约定；
//! 更新检查/下载的领域逻辑在 downloader/mod_updates/；版本图标命令在 commands/version_icon.rs。

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::commands::gamedirs::get_active_game_dir;
use crate::downloader::mod_updates::{
    self, ModFileDetail, ModListResult, ModUpdate, ResourceListResult,
};

/// 本地存档信息（saves 目录子目录扫描结果）。
#[derive(Debug, Clone, Serialize)]
pub struct WorldInfo {
    /// 世界文件夹名
    pub name: String,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

/// 版本下资源目录中的单个文件（resourcepacks/datapacks/shaderpacks 扫描结果）。
#[derive(Debug, Clone, Serialize)]
pub struct ResourceFile {
    /// 文件名
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

/// 版本目录绝对路径（versions/{name}）。目录不存在时返回错误（与 ensure_game_folder 语义区分：
/// 内容是"读"操作，不该静默创建）。
/// pub(crate)：版本图标命令（version_icon.rs）共用同一目录定位约定。
pub(crate) fn version_dir(version_name: &str) -> Result<PathBuf, String> {
    let active = get_active_game_dir();
    let dir = Path::new(&active.path).join("versions").join(version_name);
    if !dir.exists() {
        return Err(format!("版本目录不存在\n{}", dir.display()));
    }
    Ok(dir)
}

/// 列出版本下的本地存档（saves 子目录，按最后修改时间倒序；无 saves 目录返回空列表）。
#[tauri::command]
pub fn list_worlds(version_name: String) -> Vec<WorldInfo> {
    let Ok(vdir) = version_dir(&version_name) else {
        return Vec::new();
    };
    let saves = vdir.join("saves");
    let Ok(entries) = std::fs::read_dir(&saves) else {
        return Vec::new();
    };
    let mut worlds: Vec<WorldInfo> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.is_dir() {
                return None;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let meta = e.metadata().ok()?;
            Some(WorldInfo {
                name,
                last_modified: crate::util::modified_secs(&meta),
            })
        })
        .collect();
    worlds.sort_by_key(|w| std::cmp::Reverse(w.last_modified));
    worlds
}

/// 世界 icon 文件大小上限（128×128 PNG 正常 <100KB；超限视为异常，前端回退占位）。
const MAX_WORLD_ICON_BYTES: u64 = 512 * 1024;

/// 读取世界缩略图（saves/<world>/icon.png，即游戏世界选择界面的缩略图来源）。
/// 缺失/超限/损坏返回 None（不报错，前端回退 Map 占位）。
/// `world_name` 仅接受简单目录名，防路径穿越。
#[tauri::command]
pub fn get_world_icon(version_name: String, world_name: String) -> Option<crate::util::IconData> {
    if world_name.contains('/') || world_name.contains('\\') || world_name.contains("..") {
        return None;
    }
    let vdir = version_dir(&version_name).ok()?;
    crate::util::icon_payload(
        &vdir.join("saves").join(&world_name).join("icon.png"),
        MAX_WORLD_ICON_BYTES,
    )
}

/// 列出版本下资源目录（材质包/数据包/光影包/mod）中的文件，按名称排序；目录不存在返回空列表。
/// `kind` 白名单：resourcepacks / datapacks / shaderpacks / mods（防路径穿越）。
#[tauri::command]
pub fn list_resource_files(version_name: String, kind: String) -> Vec<ResourceFile> {
    if !matches!(
        kind.as_str(),
        "resourcepacks" | "datapacks" | "shaderpacks" | "mods"
    ) {
        return Vec::new();
    }
    let Ok(vdir) = version_dir(&version_name) else {
        return Vec::new();
    };
    let dir = vdir.join(&kind);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<ResourceFile> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(ResourceFile {
                name: e.file_name().to_string_lossy().to_string(),
                size: meta.len(),
                last_modified: crate::util::modified_secs(&meta),
            })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// 列出资源目录（材质包/数据包/光影包）下的 .zip 详细列表：
/// sha1 → Modrinth 识别（resourcepack/shaderpack/datapack 项目），命中带社区元数据，
/// 未命中/文件夹形式仅本地信息（网络失败时 files 仍含本地信息，network_error 置位）。
/// `kind` 白名单：resourcepacks / datapacks / shaderpacks（防路径穿越）。
#[tauri::command]
pub async fn list_resource_detailed(
    version_name: String,
    kind: String,
) -> Result<ResourceListResult, String> {
    if !matches!(kind.as_str(), "resourcepacks" | "datapacks" | "shaderpacks") {
        return Ok(ResourceListResult {
            files: Vec::new(),
            network_error: false,
        });
    }
    let vdir = version_dir(&version_name)?;
    Ok(mod_updates::list_dir_detailed(&vdir.join(&kind), mod_updates::ScanFilter::Zip).await)
}

/// 删除资源目录（材质包/数据包/光影包）下的单个文件。
/// `kind` 白名单 + 纯文件名（防路径穿越），文件不存在报错。
#[tauri::command]
pub fn delete_resource_file(
    version_name: String,
    kind: String,
    file_name: String,
) -> Result<(), String> {
    if !matches!(kind.as_str(), "resourcepacks" | "datapacks" | "shaderpacks") {
        return Err("未知资源类型".to_string());
    }
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err("非法文件名".to_string());
    }
    let vdir = version_dir(&version_name)?;
    let path = vdir.join(&kind).join(&file_name);
    if !path.is_file() {
        return Err(format!("文件不存在\n{}", file_name));
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除失败\n{e}"))
}

/// 删除整个存档（saves/<world> 目录，递归）。
/// `world_name` 仅接受简单目录名（防路径穿越）；大目录删除走 tokio（内部 spawn_blocking）。
#[tauri::command]
pub async fn delete_world(version_name: String, world_name: String) -> Result<(), String> {
    if world_name.contains('/') || world_name.contains('\\') || world_name.contains("..") {
        return Err("非法目录名".to_string());
    }
    let vdir = version_dir(&version_name)?;
    let path = vdir.join("saves").join(&world_name);
    if !path.is_dir() {
        return Err(format!("存档不存在\n{}", world_name));
    }
    tokio::fs::remove_dir_all(&path)
        .await
        .map_err(|e| format!("删除失败\n{e}"))
}

/// 列出 mods 目录下的 mod 详情（.jar 扫描 + Modrinth 元数据识别；网络失败降级本地信息）。
/// 兼容旧入口；前端现走逐文件增量识别（identify_mod_file）。
#[tauri::command]
pub async fn list_mods_detailed(version_name: String) -> Result<ModListResult, String> {
    let vdir = version_dir(&version_name)?;
    Ok(mod_updates::list_mods_detailed(&vdir.join("mods")).await)
}

/// 增量识别单个 mod 文件：sha1 → Modrinth 元数据。
/// 返回 None = 未命中（HTTP 404）/文件缺失/网络失败——前端保持本地信息渲染；
/// 前端逐文件并发调用，每项完成立即刷新列表（不用等全部识别完）。
/// `file_name` 仅接受纯文件名（防路径穿越）。
#[tauri::command]
pub async fn identify_mod_file(
    version_name: String,
    file_name: String,
) -> Result<Option<ModFileDetail>, String> {
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err("非法文件名".into());
    }
    let vdir = version_dir(&version_name)?;
    let f = mod_updates::identify_one_file(&vdir.join("mods"), &file_name).await;
    Ok(f)
}

/// 检查 mods 目录下的 mod 是否有更新（仅 Modrinth 源，命中不到的项目跳过）。
/// `mc_version` / `loader` 由前端从版本目录名解析传入（fabric/forge/neoforge/quilt）。
#[tauri::command]
pub async fn check_mod_updates(
    version_name: String,
    mc_version: String,
    loader: String,
) -> Result<Vec<ModUpdate>, String> {
    let vdir = version_dir(&version_name)?;
    let files = mod_updates::scan_mod_files(&vdir.join("mods"));
    if files.is_empty() {
        return Ok(Vec::new());
    }
    Ok(mod_updates::check_updates(&vdir.join("mods"), &files, &mc_version, &loader).await)
}

/// 更新单个 mod：下载新版本 jar 并替换 mods 目录中的旧文件。
#[tauri::command]
pub async fn update_mod_file(
    version_name: String,
    file_name: String,
    file_url: String,
    file_sha1: String,
) -> Result<(), String> {
    let vdir = version_dir(&version_name)?;
    mod_updates::update_mod_file(&vdir.join("mods"), &file_name, &file_url, &file_sha1).await
}

/// 删除 mods 目录下的单个 mod 文件。
#[tauri::command]
pub fn delete_mod_file(version_name: String, file_name: String) -> Result<(), String> {
    let vdir = version_dir(&version_name)?;
    mod_updates::delete_mod_file(&vdir.join("mods"), &file_name)
}

/// 切换 mods 目录下单个 mod 文件的启用/禁用状态（重命名 .jar → .jar.disabled，扫描自动隐藏）。
#[tauri::command]
pub fn toggle_mod_file(version_name: String, file_name: String) -> Result<bool, String> {
    let vdir = version_dir(&version_name)?;
    mod_updates::toggle_mod_file(&vdir.join("mods"), &file_name)
}

/// 导出整合包：把版本目录下的 mods/config/shaderpacks/resourcepacks 打进 zip
/// （顶层同名目录 + manifest.json 元数据）。目录缺失自动跳过；打包走 spawn_blocking 防阻塞。
/// jar 等已压缩内容用 Stored（不二次压缩，速度优先）。
#[tauri::command]
pub async fn export_modpack(version_name: String, save_path: String) -> Result<(), String> {
    let vdir = version_dir(&version_name)?;
    let name = version_name.clone();
    tokio::task::spawn_blocking(move || pack_modpack(&vdir, &name, &save_path))
        .await
        .map_err(|e| format!("打包任务失败\n{e}"))?
}

fn pack_modpack(vdir: &Path, version_name: &str, save_path: &str) -> Result<(), String> {
    // 收集存在的顶层目录
    let subdirs: Vec<&str> = ["mods", "config", "shaderpacks", "resourcepacks"]
        .iter()
        .copied()
        .filter(|d| vdir.join(d).is_dir())
        .collect();

    let file = std::fs::File::create(save_path).map_err(|e| format!("创建文件失败\n{e}"))?;
    let mut writer = zip::ZipWriter::new(file);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let manifest = serde_json::json!({
        "name": version_name,
        "directories": subdirs,
        "exported_at": unix_now(),
    });
    let content = serde_json::to_vec_pretty(&manifest).map_err(|e| format!("序列化失败\n{e}"))?;
    writer
        .start_file("manifest.json", opts)
        .map_err(|e| format!("写入 manifest 失败\n{e}"))?;
    writer
        .write_all(&content)
        .map_err(|e| format!("写入 manifest 失败\n{e}"))?;

    for sub in subdirs {
        add_dir_to_zip(&mut writer, &vdir.join(sub), sub, opts)?;
    }
    writer.finish().map_err(|e| format!("写入 zip 失败\n{e}"))?;
    Ok(())
}

/// 递归把目录内容写入 zip（保留相对路径；目录本身也记录条目，zip 空目录才可保留）。
fn add_dir_to_zip(
    writer: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    writer
        .add_directory(format!("{prefix}/"), opts)
        .map_err(|err| format!("写入目录失败\n{err}"))?;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = format!("{}/{}", prefix, e.file_name().to_string_lossy());
        if path.is_dir() {
            add_dir_to_zip(writer, &path, &name, opts)?;
        } else if path.is_file() {
            let mut f = std::fs::File::open(&path).map_err(|err| format!("打开文件失败\n{err}"))?;
            writer
                .start_file(name, opts)
                .map_err(|err| format!("写入文件失败\n{err}"))?;
            std::io::copy(&mut f, writer).map_err(|err| format!("写入文件失败\n{err}"))?;
        }
    }
    Ok(())
}

/// 当前 unix 时间戳（秒，String 便于进 JSON）；系统时间异常时退回 0（不影响打包主流程）。
fn unix_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn pack_modpack_keeps_existing_dirs_and_manifest() {
        let vdir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(vdir.path().join("mods")).unwrap();
        std::fs::write(vdir.path().join("mods").join("a.jar"), b"jar-content").unwrap();
        std::fs::create_dir_all(vdir.path().join("mods").join("nested")).unwrap();
        std::fs::write(vdir.path().join("mods").join("nested").join("b.jar"), b"b").unwrap();
        // config 目录不存在 → 打包时跳过
        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("pack.zip");
        pack_modpack(vdir.path(), "1.20.1-fabric-1.0", zip_path.to_str().unwrap()).unwrap();

        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            names.contains(&"manifest.json".to_string()),
            "缺 manifest: {names:?}"
        );
        assert!(
            names.contains(&"mods/".to_string()),
            "缺 mods 目录条目: {names:?}"
        );
        assert!(
            names.contains(&"mods/a.jar".to_string()),
            "缺 mods/a.jar: {names:?}"
        );
        assert!(
            names.contains(&"mods/nested/b.jar".to_string()),
            "缺嵌套文件: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.starts_with("config")),
            "不存在的 config 目录不应入包: {names:?}"
        );

        let mut m = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut m)
            .unwrap();
        let j: serde_json::Value = serde_json::from_str(&m).unwrap();
        assert_eq!(j["name"], "1.20.1-fabric-1.0");
        assert_eq!(j["directories"][0], "mods");
    }

    #[test]
    fn pack_modpack_empty_version_still_writes_manifest() {
        let vdir = tempfile::tempdir().unwrap(); // 无任何内容目录
        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("pack.zip");
        pack_modpack(vdir.path(), "1.20.1", zip_path.to_str().unwrap()).unwrap();

        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names, vec!["manifest.json"]);
    }
}
