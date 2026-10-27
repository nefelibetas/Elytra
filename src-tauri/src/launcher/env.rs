//! 启动前置（commands/launch.rs 调用）：加载配置、解析版本 JSON、完整性检查、JDK 匹配、游戏语言。

use std::path::PathBuf;

use crate::commands::version::VersionServer;
use crate::config::downloader::DownloaderConfig;
use crate::constants;
use crate::downloader::hash::sha1_hex;
use crate::game::loader::expand_inheritance;
use crate::game::resource::assets::AssetsIndex;
use crate::game::resource::library::missing_libraries;
use crate::game::version::dir::version_server_section;
use crate::game::version::model::VersionJson;
use crate::jdk::model::{JdkInfo, JdkOptions, JdkScanCache, JdkSelection};
use crate::launcher::args::find_version_json;
use crate::launcher::compat::{self, JavaCompat};
use crate::launcher::model::LaunchOptions;
use crate::persist::{self, downloader_config, launch_options};
use crate::util::{java_bin_path, parse_java_major, sha1_sub_dir};

/// 按 JDK 选择配置解析实际使用的 JDK 版本字符串；Manual 在扫描缓存中按 path 匹配，Auto/未匹配返回 None。
pub fn selected_jdk_version(options: &JdkOptions, cache: &JdkScanCache) -> Option<String> {
    match &options.selection {
        JdkSelection::Manual { path } => cache
            .jdks
            .iter()
            .find(|e| &e.path == path)
            .map(|e| e.version.clone()),
        JdkSelection::Auto => None,
    }
}

/// 加载全部持久化配置 + 解析版本 JSON，供 launch/spawn 共用。
#[allow(clippy::type_complexity)]
pub fn load_launch_config(
    version_dir_name: &str,
) -> Result<
    (
        PathBuf,     // java_path
        VersionJson, // version_json
        PathBuf,     // mc_dir
        LaunchOptions,
        u32, // java_major
    ),
    String,
> {
    let dl_config: DownloaderConfig = downloader_config();
    let jdk_options: JdkOptions = persist::read(constants::SECT_JDK).unwrap_or_default();
    let jdk_cache: JdkScanCache = persist::read(constants::SECT_JDK_SCANNED).unwrap_or_default();

    // minecraft_dir 可能是相对路径（默认 ".minecraft"），统一转绝对，避免 classpath/natives 基于 java cwd 解析出错
    let mc_dir = dl_config.minecraft_dir_abs();

    // 实例级启动参数（有 [launch] 段则覆盖全局；否则沿用全局）
    let instance_launch = persist::instance::read_launch(&mc_dir, version_dir_name);
    let mut launch_option: LaunchOptions = launch_options();
    if let Some(profile) = instance_launch.as_ref() {
        profile.apply_to(&mut launch_option);
    }

    // 版本级服务器直连（版本设置里配的 --server/--port）优先，覆盖全局/实例 LaunchOptions
    let version_server: VersionServer = version_server_section(version_dir_name)
        .ok()
        .and_then(|s| persist::read(&s).ok())
        .unwrap_or_default();
    let launch_option = if version_server.server.is_some() || version_server.port.is_some() {
        LaunchOptions {
            server: version_server.server.or(launch_option.server),
            port: version_server.port.or(launch_option.port),
            ..launch_option
        }
    } else {
        launch_option
    };

    // 实例级自动任务（有 [auto_tasks] 段则用实例值，否则沿用全局）
    let launch_option = match persist::instance::read_auto_tasks(&mc_dir, version_dir_name) {
        Some(auto_tasks) => LaunchOptions {
            auto_tasks,
            ..launch_option
        },
        None => launch_option,
    };

    // Java 选择随实例启动快照；没有实例快照时沿用全局 JDK 选择
    let java_selection = instance_launch
        .map(|profile| profile.java)
        .unwrap_or(jdk_options.selection);

    // 读取版本 JSON（扫描目录，文件名不一定是目录名），并展开 inheritsFrom 继承
    let vdir = mc_dir.join("versions").join(version_dir_name);
    if !vdir.is_dir() {
        bail_key!(
            "errors.version.missing",
            "版本目录不存在\n{}",
            vdir.display()
        );
    }
    let version_json = load_version_json_with_inheritance(&mc_dir, version_dir_name)?;

    let required = version_json
        .java_version
        .major_version
        .as_u64()
        .unwrap_or(0) as u32;
    let java_env = match java_selection {
        JdkSelection::Manual { path } => jdk_cache
            .jdks
            .iter()
            .find(|e| e.path == path)
            .cloned()
            .ok_or_else(|| keyed_msg!("errors.launch.jdkNotFound", "未找到指定的 JDK"))?,
        JdkSelection::Auto => auto_pick_jdk(&jdk_cache.jdks, required).ok_or_else(|| {
            if required > 0 {
                keyed_msg!(
                    "errors.launch.jdkRequired?required={required}",
                    "需要 Java {required}+，请在设置中选择或刷新 JDK"
                )
            } else {
                keyed_msg!(
                    "errors.launch.jdkNotConfigured",
                    "未配置 JDK，请先在设置中选择 Java 路径"
                )
            }
        })?,
    };
    // java_env.path 是 JAVA_HOME（目录），需要拼上 bin/java.exe
    let java_home = PathBuf::from(&java_env.path);
    if !java_home.exists() {
        bail_key!(
            "errors.jdk.failed",
            "JDK 路径不存在\n{}",
            java_home.display()
        );
    }
    let java_path = java_bin_path(&java_home);
    if !java_path.exists() {
        bail_key!(
            "errors.jdk.failed",
            "未找到 Java 可执行文件\n{}",
            java_path.display()
        );
    }

    let java_major = parse_java_major(&java_env.version).ok_or_else(|| {
        keyed_msg!(
            "errors.launch.jdkVersionInvalid",
            "无法解析 JDK 版本号\n{}",
            java_env.version
        )
    })?;

    // 版本 ↔ Java 兼容判定（下界对所有主版本通用；上界只拦跨 8→9 的老工具链）
    if let JavaCompat::Block(msg) = compat::check(
        &version_json,
        java_major,
        &java_env.path,
        launch_option.disable_launch_wrapper,
    ) {
        return Err(msg);
    }

    Ok((java_path, version_json, mc_dir, launch_option, java_major))
}

/// 读取版本 JSON 并展开 inheritsFrom 继承（ModLoader json 常为继承式，直接解析缺父库；父版本 json 缺失时报错）。
fn load_version_json_with_inheritance(
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> Result<VersionJson, String> {
    let vdir = mc_dir.join("versions").join(version_dir_name);
    let vj_path = find_version_json(&vdir, version_dir_name).ok_or_else(|| {
        keyed_msg!(
            "errors.launch.versionJsonMissing",
            "版本目录下未找到版本 JSON\n{}",
            vdir.display()
        )
    })?;
    let vj_text = std::fs::read_to_string(&vj_path).map_err(common_err_ctx_key!(
        "errors.version.read",
        "读取版本 JSON 失败"
    ))?;
    let mut vj_value: serde_json::Value = serde_json::from_str(&vj_text).map_err(
        common_err_ctx_key!("errors.version.parse", "解析版本 JSON 失败"),
    )?;

    while let Some(parent_id) = vj_value.get("inheritsFrom").and_then(|v| v.as_str()) {
        let parent_path = mc_dir
            .join("versions")
            .join(parent_id)
            .join(format!("{parent_id}.json"));
        let parent_text = std::fs::read_to_string(&parent_path).map_err(|e| {
            keyed_msg!(
                "errors.launch.parentVersionMissing?parent={parent_id}",
                "读取父版本 JSON（{parent_id}）失败——该版本为继承式安装，请先用 Elytra 补装原版 {parent_id}\n{e}"
            )
        })?;
        vj_value = expand_inheritance(&parent_text, vj_value, version_dir_name)?;
    }

    serde_json::from_value(vj_value).map_err(common_err_ctx_key!(
        "errors.version.parse",
        "解析版本 JSON 失败"
    ))
}

/// 启动前完整性检测：校验 client JAR 和所有必需 library 是否已下载。
pub fn pre_launch_check(
    version_json: &VersionJson,
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> Result<(), String> {
    let mut missing: Vec<String> = Vec::new();

    // 1. client JAR（存在但 0 字节视为损坏）
    let vdir = mc_dir.join("versions").join(version_dir_name);
    let jar_ok = |p: &std::path::Path| p.metadata().map(|m| m.len() > 0).unwrap_or(false);
    // 与 classpath 用同一套解析规则（否则"检查通过但启动用错 jar"）
    if super::args::resolve_client_jar(&vdir, version_dir_name)
        .filter(|p| jar_ok(p))
        .is_none()
    {
        missing.push(format!(
            "客户端 JAR: {}",
            vdir.join(format!("{}.jar", version_dir_name)).display()
        ));
    }

    // 2. libraries（复用下载侧过滤 + sha1 校验：缺失/损坏都算）
    let lib_dir = mc_dir.join("libraries");
    let missing_libs = missing_libraries(&version_json.libraries, &lib_dir);
    if !missing_libs.is_empty() {
        let mut msg = format!("缺失支持库 {} 个（含 sha1 校验）", missing_libs.len());
        for p in missing_libs.iter().take(20) {
            msg.push_str(&format!("\n  {}", p));
        }
        if missing_libs.len() > 20 {
            msg.push_str(&format!("\n  … 其余 {} 个", missing_libs.len() - 20));
        }
        missing.push(msg);
    }

    // 3. assets index（存在 + sha1 校验）+ 资源对象扫描（索引损坏或对象缺失/被截断会漏检 → 无声音/无语言/贴图缺失）。
    let assets_dir = mc_dir.join("assets");
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", version_json.asset_index.id));
    let index_sha1 = version_json.asset_index.sha1.as_str();
    match std::fs::read(&index_path) {
        Err(_) => {
            missing.push(format!("资源索引: {}", index_path.display()));
        }
        Ok(index_bytes) => {
            // 索引 sha1 校验（version_json 提供期望值时；空则跳过校验，仅确认可解析）
            if !index_sha1.is_empty() && sha1_hex(&index_bytes) != index_sha1 {
                missing.push(format!(
                    "资源索引校验失败（sha1 不匹配，文件已损坏）: {}",
                    index_path.display()
                ));
            } else {
                // 对象数可达数千，逐个 sha1 每启动多花数秒；改为存在 + 文件大小校验（可捕获缺失/截断/错误页）。
                match serde_json::from_slice::<AssetsIndex>(&index_bytes) {
                    Err(e) => {
                        missing.push(format!("资源索引解析失败（文件已损坏）: {e}"));
                    }
                    Ok(index) => {
                        let objects_dir = assets_dir.join("objects");
                        let mut bad_objects: Vec<String> = Vec::new();
                        for (name, obj) in &index.objects {
                            let sub = sha1_sub_dir(&obj.hash);
                            let path = objects_dir.join(sub).join(&obj.hash);
                            let ok = std::fs::metadata(&path)
                                .map(|m| m.len() == obj.size)
                                .unwrap_or(false);
                            if !ok {
                                bad_objects.push(format!("{} ({}/{})", name, sub, obj.hash));
                            }
                        }
                        if !bad_objects.is_empty() {
                            let mut msg =
                                format!("缺失/损坏资源文件 {} 个（大小校验）", bad_objects.len());
                            for b in bad_objects.iter().take(20) {
                                msg.push_str(&format!("\n  {}", b));
                            }
                            if bad_objects.len() > 20 {
                                msg.push_str(&format!("\n  … 其余 {} 个", bad_objects.len() - 20));
                            }
                            missing.push(msg);
                        }
                    }
                }
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        // 第一行为大类短句（dialog 正文显示），文件列表进复制区
        Err(keyed_msg!(
            "errors.launch.filesIncomplete",
            "游戏文件不完整\n缺少以下文件：\n{}",
            missing.join("\n")
        ))
    }
}

/// options.txt 行 upsert：替换以 `key:` 开头的行，不存在则追加（文件不存在时创建）；与 skin.rs（resourcePacks）共用。
pub fn upsert_option(options_path: &std::path::Path, key: &str, value: &str) {
    let line = format!("{key}:{value}");
    let content = match std::fs::read_to_string(options_path) {
        Ok(c) => c,
        Err(_) => {
            if let Some(parent) = options_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(options_path, format!("{line}\n"));
            return;
        }
    };

    let mut found = false;
    let mut out = String::new();
    for l in content.lines() {
        if l.starts_with(&format!("{key}:")) {
            out.push_str(&line);
            out.push('\n');
            found = true;
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !found {
        out.push_str(&line);
        out.push('\n');
    }
    let _ = std::fs::write(options_path, out);
}

/// 游戏语言：自动中文；1.13+ 启动时读 gameDir/options.txt 的 `lang` 字段，存在则替换否则追加。
pub fn apply_game_language(game_dir: &std::path::Path) {
    upsert_option(&game_dir.join("options.txt"), "lang", "zh_cn");
}

/// 从已扫描 JDK 列表选最合适者：优先精确匹配，否则最低但满足需求，再不行取最高（列表可空 → None）；pub(crate) 与 ModLoader 安装器 processors 共用。
pub(crate) fn auto_pick_jdk(discovered: &[JdkInfo], required: u32) -> Option<JdkInfo> {
    let mut candidates: Vec<(u32, &JdkInfo)> = discovered
        .iter()
        .filter_map(|env| parse_java_major(&env.version).map(|m| (m, env)))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    // 精确匹配
    if let Some(env) = candidates.iter().find(|(m, _)| *m == required) {
        return Some(env.1.clone());
    }
    // 最低但满足需求
    let mut enough: Vec<_> = candidates.iter().filter(|(m, _)| *m >= required).collect();
    if !enough.is_empty() {
        enough.sort_by_key(|(m, _)| *m);
        return Some(enough[0].1.clone());
    }
    // 都不满足 → 用最高版本（列表可能为空，返回 None 由调用方兜底）
    candidates.sort_by_key(|(m, _)| *m);
    candidates.last().map(|(_, v)| (*v).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jdk::config::save_cache as save_jdk_cache;
    use crate::jdk::{read as jdk_options, read_cache as jdk_cache, save as save_jdk};

    /// 假 JDK 目录下要放的 java 可执行文件名（`load_launch_config` 会校验其存在）。
    const JDK_BIN: &str = if cfg!(windows) { "java.exe" } else { "java" };

    #[test]
    fn test_auto_pick_jdk() {
        let list = vec![
            JdkInfo {
                path: "/jdk17".into(),
                version: "17.0.8".into(),
                vendor: "OpenJDK".into(),
                bits: 64,
            },
            JdkInfo {
                path: "/jdk8".into(),
                version: "1.8.0_202".into(),
                vendor: "Oracle".into(),
                bits: 64,
            },
            JdkInfo {
                path: "/jdk21".into(),
                version: "21".into(),
                vendor: "Temurin".into(),
                bits: 64,
            },
        ];
        assert_eq!(auto_pick_jdk(&list, 21).unwrap().path, "/jdk21");
        assert_eq!(auto_pick_jdk(&list, 17).unwrap().path, "/jdk17");
        assert_eq!(auto_pick_jdk(&list, 11).unwrap().path, "/jdk17");
        assert_eq!(auto_pick_jdk(&list, 22).unwrap().path, "/jdk21");
        assert!(auto_pick_jdk(&[], 8).is_none());
    }

    /// 造临时 mc 目录（含指定版本 JSON）+ 只列一个 JDK 的缓存与 mc_dir，跑真实 `load_launch_config` 后
    /// 还原共享 Settings.toml（断言前还原：失败也不给后续用例留污染）。
    fn try_launch_config(name: &str, version_json: &str, jdk_version: &str) -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("elytra_lc_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let vdir = tmp.join("versions").join(name);
        std::fs::create_dir_all(&vdir).unwrap();
        std::fs::write(vdir.join(format!("{name}.json")), version_json).unwrap();
        let jdk = tmp.join("jdk");
        std::fs::create_dir_all(jdk.join("bin")).unwrap();
        std::fs::write(jdk.join("bin").join(JDK_BIN), b"").unwrap();

        let old_dl = persist::downloader_config();
        let old_options = jdk_options();
        let old_cache = jdk_cache();
        let mut dl = old_dl.clone();
        dl.minecraft_dir = tmp.display().to_string();
        persist::save_downloader_config(&dl).unwrap();
        save_jdk(&JdkOptions::default()).unwrap();
        save_jdk_cache(&JdkScanCache {
            jdks: vec![JdkInfo {
                path: jdk.display().to_string(),
                version: jdk_version.into(),
                vendor: "Temurin".into(),
                bits: 64,
            }],
        })
        .unwrap();

        let result = load_launch_config(name).map(|_| ());
        persist::save_downloader_config(&old_dl).unwrap();
        save_jdk(&old_options).unwrap();
        save_jdk_cache(&old_cache).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
        result
    }

    /// 最小版本 JSON（字段形态与真实 json 一致）。
    fn version_json(name: &str, main_class: &str, java_major: u32) -> String {
        format!(
            r#"{{"id":"{name}","mainClass":"{main_class}","assets":"x","assetIndex":{{"id":"x","url":"x","sha1":"y","size":1,"totalSize":1}},"downloads":{{"client":{{"url":"x","sha1":"y","size":1}}}},"arguments":{{"game":[],"jvm":[]}},"complianceLevel":0,"minimumLauncherVersion":21,"javaVersion":{{"majorVersion":{java_major}}},"releaseTime":"2021-11-30T00:00:00Z","time":"2021-11-30T00:00:00Z","type":"release","libraries":[]}}"#
        )
    }

    /// 扫描缓存里只有新 JDK 时：launchwrapper 实例在启动前被拦下并提示下载 Java 8。
    #[test]
    fn launchwrapper_instance_blocked_with_modern_jdk_only() {
        let name = "1.12.2-forge-14.23.5.2864";
        let json = version_json(name, "net.minecraft.launchwrapper.Launch", 8);
        let err = try_launch_config(name, &json, "21.0.10")
            .expect_err("Java 21 拉 launchwrapper 必然失败，应在启动前拦下");
        assert!(
            err.starts_with("@i18n:errors.jdk.legacy_java"),
            "应给出可操作错误: {err}"
        );
        assert!(err.contains("Java 21"), "应带上当前 JDK 主版本: {err}");
    }

    /// 选中的 Java 低于版本要求时拦下（否则必然 UnsupportedClassVersionError）。
    #[test]
    fn too_old_java_blocked_before_launch() {
        let name = "26.3";
        let json = version_json(name, "net.minecraft.client.main.Main", 25);
        let err = try_launch_config(name, &json, "1.8.0_202")
            .expect_err("Java 8 跑要求 Java 25 的版本必然失败，应在启动前拦下");
        assert!(
            err.starts_with("@i18n:errors.jdk.too_old?need=25&cur=8"),
            "错误应带上需求与当前版本（供前端插值）: {err}"
        );
    }

    /// 继承式版本（HMCL/官方安装器风格）：展开后合并父库、子关键字段覆盖、父缺失时报错。
    #[test]
    fn version_json_expands_inheritance() {
        let tmp = std::env::temp_dir().join(format!("elytra_inh_{}", std::process::id()));
        let parent = tmp.join("versions/1.20.1");
        let child = tmp.join("versions/1.20.1-forge-47.1.0");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(
            parent.join("1.20.1.json"),
            r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main","assets":"1.20","assetIndex":{"id":"1.20","url":"x","sha1":"y","size":1,"totalSize":1},"downloads":{"client":{"url":"x","sha1":"y","size":1}},"arguments":{"game":[],"jvm":[]},"complianceLevel":0,"minimumLauncherVersion":21,"javaVersion":{"majorVersion":17},"releaseTime":"2021-11-30T00:00:00Z","time":"2021-11-30T00:00:00Z","type":"release","libraries":[{"name":"a:b:1","downloads":{"artifact":{"path":"a/b/1/a-1.jar","url":"x","sha1":"y","size":1}}}]}"#,
        )
        .unwrap();
        std::fs::write(
            child.join("1.20.1-forge-47.1.0.json"),
            r#"{"id":"1.20.1-forge-47.1.0","inheritsFrom":"1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","complianceLevel":0,"libraries":[{"name":"net.minecraftforge:forge:47.1.0","downloads":{"artifact":{"path":"net/minecraftforge/forge/47.1.0/forge-47.1.0.jar","url":"x","sha1":"y","size":1}}}]}"#,
        )
        .unwrap();

        let vj = load_version_json_with_inheritance(&tmp, "1.20.1-forge-47.1.0").unwrap();
        assert_eq!(vj.libraries.len(), 2, "父库 + 子库应合并");
        assert_eq!(
            vj.libraries[0].name, "net.minecraftforge:forge:47.1.0",
            "子库应在父库之前（classpath 顺序：子版本的补丁/加载器库先加载，OptiFine 预补丁类覆盖依赖）"
        );
        assert_eq!(
            vj.main_class, "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "子 mainClass 应覆盖父"
        );

        std::fs::remove_dir_all(&parent).unwrap();
        let err = load_version_json_with_inheritance(&tmp, "1.20.1-forge-47.1.0").unwrap_err();
        assert!(err.contains("父版本 JSON"), "应提示父版本缺失: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// find_version_json：PCL2 隔离实例目录混有游戏数据 json（字典序在前），应跳过取真正版本 json。
    #[test]
    fn find_version_json_picks_real_version_json() {
        let tmp = std::env::temp_dir().join(format!("elytra_vj_{}", std::process::id()));
        let dir = tmp.join("versions/愚者");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("TrashSlotSaveState.json"), r#"{"save":1}"#).unwrap();
        std::fs::write(dir.join("patchouli_data.json"), r#"{"book":1}"#).unwrap();
        std::fs::write(
            dir.join("愚者.json"),
            r#"{"id":"1.20.1-forge","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher","libraries":[]}"#,
        )
        .unwrap();
        let found = find_version_json(&dir, "愚者");
        let name = found
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(name, "愚者.json", "应跳过游戏数据 json 取到版本 json");

        let dir2 = tmp.join("versions/1.20.1");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(
            dir2.join("1.20.1.json"),
            r#"{"mainClass":"net.minecraft.client.main.Main"}"#,
        )
        .unwrap();
        std::fs::write(dir2.join("extra.json"), r#"{"x":1}"#).unwrap();
        let found2 = find_version_json(&dir2, "1.20.1");
        assert_eq!(found2.unwrap().file_name().unwrap(), "1.20.1.json");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// pre_launch_check 资源完整性：① 索引 + 对象齐全 → 通过；② 对象截断 → 报"缺失/损坏资源文件"；③ 索引 sha1 不匹配 → 报校验失败。
    #[test]
    fn pre_launch_check_detects_asset_corruption() {
        let tmp = std::env::temp_dir().join(format!("elytra_assetchk_{}", std::process::id()));
        let mc = &tmp;
        let vdir = mc.join("versions/1.20.1");
        std::fs::create_dir_all(&vdir).unwrap();
        std::fs::write(vdir.join("1.20.1.jar"), b"CLIENTJAR").unwrap();

        let obj_body = b"hello world";
        let obj_hash = sha1_hex(obj_body);
        let obj_size = obj_body.len() as u64;
        let objects_dir = mc.join("assets/objects").join(&obj_hash[..2]);
        std::fs::create_dir_all(&objects_dir).unwrap();
        std::fs::write(objects_dir.join(&obj_hash), obj_body).unwrap();

        let index_json =
            format!(r#"{{"objects":{{"minecraft/x":{{"hash":"{obj_hash}","size":{obj_size}}}}}}}"#);
        let index_sha1 = sha1_hex(index_json.as_bytes());
        let index_dir = mc.join("assets/indexes");
        std::fs::create_dir_all(&index_dir).unwrap();
        std::fs::write(index_dir.join("5.json"), &index_json).unwrap();

        let vj: VersionJson = serde_json::from_str(&format!(
            r#"{{
                "id":"1.20.1","mainClass":"net.minecraft.client.main.Main","assets":"5",
                "assetIndex":{{"id":"5","url":"x","sha1":"{index_sha1}","size":{},"totalSize":{obj_size}}},
                "downloads":{{"client":{{"url":"x","sha1":"y","size":1}}}},
                "arguments":{{"game":[],"jvm":[]}},"libraries":[],
                "complianceLevel":0,"minimumLauncherVersion":21,"javaVersion":{{"majorVersion":17}},
                "releaseTime":"2023-01-01T00:00:00Z","time":"2023-01-01T00:00:00Z","type":"release"
            }}"#,
            index_json.len()
        ))
        .unwrap();

        assert!(
            pre_launch_check(&vj, mc, "1.20.1").is_ok(),
            "索引 + 对象齐全应通过完整性检查"
        );

        std::fs::write(objects_dir.join(&obj_hash), b"short").unwrap();
        let err = pre_launch_check(&vj, mc, "1.20.1").unwrap_err();
        assert!(
            err.contains("缺失/损坏资源文件"),
            "对象大小不符应被捕获: {err}"
        );
        std::fs::write(objects_dir.join(&obj_hash), obj_body).unwrap();

        std::fs::write(index_dir.join("5.json"), format!("{index_json} ")).unwrap();
        let err = pre_launch_check(&vj, mc, "1.20.1").unwrap_err();
        assert!(
            err.contains("资源索引校验失败"),
            "索引 sha1 不匹配应被捕获: {err}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
