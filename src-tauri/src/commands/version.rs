//! 版本内容命令：存档/Mod 扫描、Mod 更新、整合包导出；目录 = 激活游戏目录 + versions/{name}。

use serde::Serialize;

use crate::game::modpack::{pack_modpack, resolve_hosted_mods, resolve_version_meta};
use crate::game::updates::{self, ModFileDetail, ModListResult, ModUpdate, ResourceListResult};
use crate::game::version::dir::{version_dir_path, version_server_section};
use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct WorldInfo {
    pub name: String,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceFile {
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间（unix 秒）
    pub last_modified: u64,
}

#[tauri::command]
pub fn list_worlds(version_name: String) -> Vec<WorldInfo> {
    let Ok(vdir) = version_dir_path(&version_name) else {
        return Vec::new();
    };
    let saves = vdir.join("saves");
    let mut worlds: Vec<WorldInfo> = util::dir_entries(&saves, true)
        .into_iter()
        .map(|(name, _, last_modified)| WorldInfo {
            name,
            last_modified,
        })
        .collect();
    worlds.sort_by_key(|w| std::cmp::Reverse(w.last_modified));
    worlds
}

/// 世界 icon 文件大小上限（128×128 PNG 正常 <100KB；超限视为异常，前端回退占位）。
const MAX_WORLD_ICON_BYTES: u64 = 512 * 1024;

/// 世界缩略图（saves/<world>/icon.png）：缺失/超限/损坏返回 None；`world_name` 须为简单目录名（防路径穿越）。
#[tauri::command]
pub fn get_world_icon(version_name: String, world_name: String) -> Option<util::IconData> {
    if !util::is_simple_name(&world_name) {
        return None;
    }
    let vdir = version_dir_path(&version_name).ok()?;
    util::icon_payload(
        &vdir.join("saves").join(&world_name).join("icon.png"),
        MAX_WORLD_ICON_BYTES,
    )
}

/// 列出版本下资源目录（材质包/数据包/光影包/mod）中的文件，按名称排序；`kind` 白名单：resourcepacks / datapacks / shaderpacks / mods。
#[tauri::command]
pub fn list_resource_files(version_name: String, kind: String) -> Vec<ResourceFile> {
    if !matches!(
        kind.as_str(),
        "resourcepacks" | "datapacks" | "shaderpacks" | "mods"
    ) {
        return Vec::new();
    }
    let Ok(vdir) = version_dir_path(&version_name) else {
        return Vec::new();
    };
    let dir = vdir.join(&kind);
    let mut files: Vec<ResourceFile> = util::dir_entries(&dir, false)
        .into_iter()
        .map(|(name, size, last_modified)| ResourceFile {
            name,
            size,
            last_modified,
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// 资源目录（材质包/数据包/光影包）的 .zip 详细列表：sha1 → Modrinth 识别，未命中/网络失败仅本地信息（network_error 置位）；`kind` 白名单：resourcepacks / datapacks / shaderpacks。
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
    let vdir = version_dir_path(&version_name)?;
    Ok(updates::list_dir_detailed(&vdir.join(&kind), updates::ScanFilter::Zip).await)
}

#[tauri::command]
pub async fn delete_resource_file(
    version_name: String,
    kind: String,
    file_name: String,
) -> Result<(), String> {
    let vdir = version_dir_path(&version_name)?;
    updates::delete_resource_file(&vdir, &kind, &file_name).await
}

/// 删除整个存档（saves/<world> 目录，递归）；`world_name` 须为简单目录名（防路径穿越），大目录走 spawn_blocking。
#[tauri::command]
pub async fn delete_world(version_name: String, world_name: String) -> Result<(), String> {
    if !util::is_simple_name(&world_name) {
        bail_key!("errors.version.invalidDirName", "非法目录名");
    }
    let vdir = version_dir_path(&version_name)?;
    let path = vdir.join("saves").join(&world_name);
    if !path.is_dir() {
        bail_key!("errors.version.worldMissing", "存档不存在\n{}", world_name);
    }
    tokio::fs::remove_dir_all(&path)
        .await
        .map_err(common_err_ctx_key!("errors.file.delete", "删除失败"))
}

#[tauri::command]
pub async fn list_mods_detailed(version_name: String) -> Result<ModListResult, String> {
    let vdir = version_dir_path(&version_name)?;
    Ok(updates::list_mods_detailed(&vdir.join("mods")).await)
}

/// 单个 mod 文件识别：sha1 → Modrinth 元数据；None = 未命中/文件缺失/网络失败；`file_name` 须为纯文件名（防路径穿越）。
#[tauri::command]
pub async fn identify_mod_file(
    version_name: String,
    file_name: String,
) -> Result<Option<ModFileDetail>, String> {
    if !crate::util::is_simple_name(&file_name) {
        bail_key!("errors.version.invalidFileName", "非法文件名");
    }
    let vdir = version_dir_path(&version_name)?;
    let f = updates::identify_one_file(&vdir.join("mods"), &file_name).await;
    Ok(f)
}

/// 检查 mods 更新（仅 Modrinth 源，未命中跳过）；`mc_version`/`loader` 由后端从版本目录 json 权威解析（目录名仅兜底）。
#[tauri::command]
pub async fn check_mod_updates(version_name: String) -> Result<Vec<ModUpdate>, String> {
    let vdir = version_dir_path(&version_name)?;
    let files = updates::scan_mod_files(&vdir.join("mods"));
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let (mc_version, loader, _) = resolve_version_meta(&vdir, &version_name);
    Ok(updates::check_updates(&vdir.join("mods"), &files, &mc_version, &loader).await)
}

#[tauri::command]
pub async fn update_mod_file(
    version_name: String,
    file_name: String,
    file_url: String,
    file_sha1: String,
) -> Result<(), String> {
    let vdir = version_dir_path(&version_name)?;
    updates::update_mod_file(&vdir.join("mods"), &file_name, &file_url, &file_sha1).await
}

/// 版本级服务器直连配置（快速进服 --server/--port，per-version）；存储于 section `version_server_{version_name}`，为空时启动回退全局 LaunchOptions。
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
pub struct VersionServer {
    pub server: Option<String>,
    pub port: Option<u16>,
}

#[tauri::command]
pub fn get_version_server(version_name: String) -> VersionServer {
    let Ok(section) = version_server_section(&version_name) else {
        return VersionServer::default();
    };
    crate::persist::read(&section).unwrap_or_default()
}

#[tauri::command]
pub fn save_version_server(
    version_name: String,
    server: Option<String>,
    port: Option<u16>,
) -> Result<(), String> {
    let section = version_server_section(&version_name)?;
    let cfg = VersionServer {
        server: server.filter(|s| !s.trim().is_empty()),
        port,
    };
    crate::persist::save(&section, &cfg)
        .map_err(common_err_ctx_key!("errors.config.write", "保存失败"))
}

#[tauri::command]
pub fn delete_mod_file(version_name: String, file_name: String) -> Result<(), String> {
    let vdir = version_dir_path(&version_name)?;
    updates::delete_mod_file(&vdir.join("mods"), &file_name)
}

/// 切换 mods 目录下单个 mod 文件的启用/禁用状态（重命名 .jar → .jar.disabled，扫描自动隐藏）。
#[tauri::command]
pub fn toggle_mod_file(version_name: String, file_name: String) -> Result<bool, String> {
    let vdir = version_dir_path(&version_name)?;
    updates::toggle_mod_file(&vdir.join("mods"), &file_name)
}

/// 导出整合包：mods/config/shaderpacks/resourcepacks/datapacks 打包为标准 MRPACK（modrinth.index.json + overrides/）；目录缺失自动跳过，打包走 spawn_blocking。
#[tauri::command]
pub async fn export_modpack(version_name: String, save_path: String) -> Result<(), String> {
    let vdir = version_dir_path(&version_name)?;
    let name = version_name.clone();
    // 能匹配 Modrinth hash 的 mod 走 files[]（在线分发，包内不物理携带），其余打进 overrides。
    let (hosted, hosted_names) = resolve_hosted_mods(&vdir).await;
    tokio::task::spawn_blocking(move || {
        pack_modpack(&vdir, &name, &save_path, &hosted, &hosted_names)
    })
    .await
    .map_err(common_err_ctx_key!("errors.file.write", "打包任务失败"))?
}

#[cfg(test)]
mod tests {
    use crate::game::modpack::pack_modpack;
    use crate::game::modpack::resolve_version_meta;
    use std::io::Read;

    #[test]
    fn pack_modpack_keeps_existing_dirs_and_manifest() {
        let vdir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(vdir.path().join("mods")).unwrap();
        std::fs::write(vdir.path().join("mods").join("a.jar"), b"jar-content").unwrap();
        std::fs::create_dir_all(vdir.path().join("mods").join("nested")).unwrap();
        std::fs::write(vdir.path().join("mods").join("nested").join("b.jar"), b"b").unwrap();
        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("pack.zip");
        pack_modpack(
            vdir.path(),
            "1.20.1-fabric-1.0",
            zip_path.to_str().unwrap(),
            &[],
            &std::collections::HashSet::new(),
        )
        .unwrap();

        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            names.contains(&"modrinth.index.json".to_string()),
            "缺 modrinth.index.json: {names:?}"
        );
        assert!(
            names.contains(&"overrides/mods/".to_string()),
            "缺 overrides/mods 目录条目: {names:?}"
        );
        assert!(
            names.contains(&"overrides/mods/a.jar".to_string()),
            "缺 overrides/mods/a.jar: {names:?}"
        );
        assert!(
            names.contains(&"overrides/mods/nested/b.jar".to_string()),
            "缺嵌套文件: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.starts_with("overrides/config")),
            "不存在的 config 目录不应入包: {names:?}"
        );

        let mut m = String::new();
        archive
            .by_name("modrinth.index.json")
            .unwrap()
            .read_to_string(&mut m)
            .unwrap();
        let j: serde_json::Value = serde_json::from_str(&m).unwrap();
        assert_eq!(j["formatVersion"], 1);
        assert_eq!(j["game"], "minecraft");
        assert_eq!(j["versionId"], "1.20.1-fabric-1.0");
        assert_eq!(j["dependencies"]["minecraft"], "1.20.1");
        assert_eq!(j["dependencies"]["fabric-loader"], "1.0");
        assert_eq!(j["files"], serde_json::json!([]));
    }

    #[test]
    fn pack_modpack_empty_version_still_writes_manifest() {
        let vdir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let zip_path = out.path().join("pack.zip");
        pack_modpack(
            vdir.path(),
            "1.20.1",
            zip_path.to_str().unwrap(),
            &[],
            &std::collections::HashSet::new(),
        )
        .unwrap();

        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names, vec!["modrinth.index.json"]);
    }

    /// json 权威解析：改名 vanilla 目录（[abcd] 里是官方 json，id 即真版本）。
    #[test]
    fn resolve_version_meta_prefers_vanilla_json_id_over_dir_name() {
        let vdir = tempfile::tempdir().unwrap();
        std::fs::write(
            vdir.path().join("[abcd].json"),
            r#"{"id":"1.21.1","mainClass":"net.minecraft.client.main.Main","type":"release"}"#,
        )
        .unwrap();
        let (mc, kind, ver) = resolve_version_meta(vdir.path(), "[abcd]");
        assert_eq!(mc, "1.21.1", "json.id 应纠正目录名垃圾");
        assert_eq!(kind, "vanilla");
        assert_eq!(ver, None);
    }

    /// 大写 loader 标记目录（PCL2 风格）+ fabric profile json：版本号经 json 同 kind 确认。
    #[test]
    fn resolve_version_meta_uppercase_dir_marker_with_fabric_json() {
        let vdir = tempfile::tempdir().unwrap();
        std::fs::write(
            vdir.path().join("1.20.1-Fabric-0.14.22.json"),
            r#"{"id":"1.20.1-fabric-0.14.22","inheritsFrom":"1.20.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
        )
        .unwrap();
        let (mc, kind, ver) = resolve_version_meta(vdir.path(), "1.20.1-Fabric-0.14.22");
        assert_eq!(mc, "1.20.1", "inheritsFrom 优先");
        assert_eq!(kind, "fabric");
        assert_eq!(ver.as_deref(), Some("0.14.22"));
    }

    /// 目录名伪标记与 json 冲突：kind 以 json 为准，版本号拒绝目录名（防注入错版本）。
    #[test]
    fn resolve_version_meta_json_wins_over_fake_dir_marker() {
        let vdir = tempfile::tempdir().unwrap();
        std::fs::write(
            vdir.path().join("X-forge-99.json"),
            r#"{"id":"1.21.1","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
        )
        .unwrap();
        let (mc, kind, ver) = resolve_version_meta(vdir.path(), "X-forge-99");
        assert_eq!(mc, "1.21.1");
        assert_eq!(kind, "fabric", "json 特征应压掉目录名伪标记");
        assert_eq!(
            ver, None,
            "forge 标记与 fabric json 冲突 → 版本号省略而非写假值"
        );
    }
}
