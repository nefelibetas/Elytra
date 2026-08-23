//! 版本内容命令（薄胶水）：存档/Mod 本地扫描、Mod 更新检查与更新下载、整合包导出。
//! 目录定位复用 gamedirs 的"当前激活游戏目录 + versions/{name}"约定；
//! 更新检查/下载的领域逻辑在 downloader/mod_updates.rs。

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::commands::gamedirs::get_active_game_dir;
use crate::downloader::mod_updates::{self, ModListResult, ModUpdate, ResourceListResult};

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

/// 文件最后修改时间（unix 秒），读取失败回退 0。
fn dir_modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 版本目录绝对路径（versions/{name}）。目录不存在时返回错误（与 ensure_game_folder 语义区分：
/// 内容是"读"操作，不该静默创建）。
fn version_dir(version_name: &str) -> Result<PathBuf, String> {
    let active = get_active_game_dir();
    let dir = Path::new(&active.path)
        .join("versions")
        .join(version_name);
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
                last_modified: dir_modified_secs(&meta),
            })
        })
        .collect();
    worlds.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    worlds
}

/// 世界 icon 文件大小上限（128×128 PNG 正常 <100KB；超限视为异常，前端回退占位）。
const MAX_WORLD_ICON_BYTES: u64 = 512 * 1024;

/// 读取世界缩略图（saves/<world>/icon.png，即游戏世界选择界面的缩略图来源）。
/// 缺失/超限/损坏返回 None（不报错，前端回退 Map 占位）。
/// `world_name` 仅接受简单目录名，防路径穿越。
#[tauri::command]
pub fn get_world_icon(version_name: String, world_name: String) -> Option<Vec<u8>> {
    if world_name.contains('/') || world_name.contains('\\') || world_name.contains("..") {
        return None;
    }
    let vdir = version_dir(&version_name).ok()?;
    let path = vdir.join("saves").join(&world_name).join("icon.png");
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_WORLD_ICON_BYTES {
        return None;
    }
    std::fs::read(&path).ok()
}

/// 版本图标读取上限（平台 logo / 用户图片一般 <256KB，留余量）。
const MAX_PACK_ICON_BYTES: u64 = 1024 * 1024;

/// 魔数嗅探是否为受支持的图片格式（png/jpeg/webp/gif）。
fn is_supported_image(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return true; // png
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return true; // jpeg
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return true; // webp
    }
    bytes.starts_with(b"GIF8") // gif
}

/// 图标优先级：用户自定义（version-icon.png）> 整合包项目图（pack-icon.png）。
fn version_icon_path(vdir: &Path) -> Option<PathBuf> {
    for name in ["version-icon.png", "pack-icon.png"] {
        let p = vdir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 校验版本目录名：仅接受简单目录名，防路径穿越。
fn ensure_simple_dir_name(name: &str) -> Result<(), String> {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("版本目录名不合法\n{name}"));
    }
    Ok(())
}

/// 读取版本图标字节（自定义 > 整合包；都没有返回 None，UI 回退 loader 图标）。
#[tauri::command]
pub fn get_version_icon(version_dir_name: String) -> Option<Vec<u8>> {
    ensure_simple_dir_name(&version_dir_name).ok()?;
    let vdir = version_dir(&version_dir_name).ok()?;
    let path = version_icon_path(&vdir)?;
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_PACK_ICON_BYTES {
        return None;
    }
    std::fs::read(&path).ok()
}

/// 图标来源："custom"=用户自定义 / "pack"=整合包项目图 / "default"=无（loader 图标）。
#[derive(Debug, Clone, Serialize)]
pub struct VersionIconInfo {
    pub source: String,
}

/// 查询当前图标来源（设置页展示状态用；目录不存在返回 default）。
#[tauri::command]
pub fn get_version_icon_source(version_dir_name: String) -> VersionIconInfo {
    let source = ensure_simple_dir_name(&version_dir_name)
        .ok()
        .and_then(|_| version_dir(&version_dir_name).ok())
        .and_then(|vdir| version_icon_path(&vdir))
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .map(|n| {
            if n == "version-icon.png" {
                "custom"
            } else {
                "pack"
            }
        })
        .unwrap_or("default");
    VersionIconInfo {
        source: source.to_string(),
    }
}

/// 设置自定义版本图标：把选中的本地图片复制为 versions/{dir}/version-icon.png。
/// 仅接受 png/jpeg/webp/gif（魔数校验），≤1MB；覆盖旧自定义图。
#[tauri::command]
pub async fn set_version_icon(
    version_dir_name: String,
    source_path: String,
) -> Result<(), String> {
    ensure_simple_dir_name(&version_dir_name)?;
    let src = PathBuf::from(&source_path);
    let meta = std::fs::metadata(&src).map_err(|e| format!("读取图片失败\n{e}"))?;
    if !meta.is_file() {
        return Err("所选路径不是文件".into());
    }
    if meta.len() == 0 || meta.len() > MAX_PACK_ICON_BYTES {
        return Err(format!(
            "图片大小需在 1MB 以内\n当前 {} 字节",
            meta.len()
        ));
    }
    let bytes = std::fs::read(&src).map_err(|e| format!("读取图片失败\n{e}"))?;
    if !is_supported_image(&bytes) {
        return Err("不支持的图片格式（仅支持 png / jpg / webp / gif）".into());
    }
    let vdir = version_dir(&version_dir_name)?;
    // 复制为固定文件名（内容格式由魔数决定，前端按嗅探渲染）
    tokio::task::spawn_blocking(move || {
        std::fs::write(vdir.join("version-icon.png"), &bytes).map_err(|e| format!("写入图标失败\n{e}"))
    })
    .await
    .map_err(|e| format!("写入任务失败\n{e}"))?
}

/// 还原为默认图标：删除自定义图标（versions/{dir}/version-icon.png）。
/// 整合包项目图（pack-icon.png）不受影响——"默认"即安装时的样子。
#[tauri::command]
pub async fn reset_version_icon(version_dir_name: String) -> Result<(), String> {
    ensure_simple_dir_name(&version_dir_name)?;
    let vdir = version_dir(&version_dir_name)?;
    let target = vdir.join("version-icon.png");
    if !target.exists() {
        return Ok(()); // 幂等：本就无自定义图
    }
    tokio::task::spawn_blocking(move || {
        std::fs::remove_file(target).map_err(|e| format!("删除图标失败\n{e}"))
    })
    .await
    .map_err(|e| format!("删除任务失败\n{e}"))?
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
                last_modified: dir_modified_secs(&meta),
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
    Ok(mod_updates::list_dir_detailed(
        &vdir.join(&kind),
        mod_updates::ScanFilter::Zip,
    )
    .await)
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
#[tauri::command]
pub async fn list_mods_detailed(version_name: String) -> Result<ModListResult, String> {
    let vdir = version_dir(&version_name)?;
    Ok(mod_updates::list_mods_detailed(&vdir.join("mods")).await)
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
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);

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
    writer
        .finish()
        .map_err(|e| format!("写入 zip 失败\n{e}"))?;
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
            let mut f = std::fs::File::open(&path)
                .map_err(|err| format!("打开文件失败\n{err}"))?;
            writer
                .start_file(name, opts)
                .map_err(|err| format!("写入文件失败\n{err}"))?;
            std::io::copy(&mut f, writer)
                .map_err(|err| format!("写入文件失败\n{err}"))?;
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
    fn is_supported_image_sniffs_magic_bytes() {
        assert!(is_supported_image(b"\x89PNG\r\n\x1a\nrest"));
        assert!(is_supported_image(&[0xff, 0xd8, 0xff, 0xe0]));
        let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        assert!(is_supported_image(&webp));
        webp[8] = b'X'; // 破坏 WEBP 标记
        assert!(!is_supported_image(&webp));
        assert!(is_supported_image(b"GIF89a"));
        assert!(!is_supported_image(b"<html>"));
        assert!(!is_supported_image(b""));
    }

    #[test]
    fn ensure_simple_dir_name_blocks_traversal() {
        assert!(ensure_simple_dir_name("1.20.1-fabric-0.16.9").is_ok());
        assert!(ensure_simple_dir_name("Pack_1").is_ok());
        assert!(ensure_simple_dir_name("a..b").is_err(), "含 .. 子串一律拒绝（对齐 get_world_icon 先例）");
        assert!(ensure_simple_dir_name("a\\b").is_err());
        assert!(ensure_simple_dir_name("..").is_err());
    }

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
        assert!(names.contains(&"manifest.json".to_string()), "缺 manifest: {names:?}");
        assert!(names.contains(&"mods/".to_string()), "缺 mods 目录条目: {names:?}");
        assert!(names.contains(&"mods/a.jar".to_string()), "缺 mods/a.jar: {names:?}");
        assert!(names.contains(&"mods/nested/b.jar".to_string()), "缺嵌套文件: {names:?}");
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
