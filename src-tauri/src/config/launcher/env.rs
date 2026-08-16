//! 启动环境准备：加载配置、解析版本 JSON、完整性检查、JDK 匹配、游戏语言。
//! 命令层（commands/launch.rs）的启动前置逻辑集中于此。

use std::path::PathBuf;

use crate::config::downloader::model::DownloaderConfig;
use crate::config::launcher::args::{find_file_in_dir, find_version_json};
use crate::config::launcher::model::LauncherConfig;
use crate::constants;
use crate::downloader::model::VersionJson;
use crate::jdk::model::{JdkConfig, JdkInfo, JdkSelection};
use crate::persist;

/// 加载全部持久化配置 + 解析版本 JSON，供 launch/spawn 共用。
#[allow(clippy::type_complexity)]
pub fn load_launch_config(
    version_dir_name: &str,
) -> Result<
    (
        PathBuf,     // java_path
        VersionJson, // version_json
        PathBuf,     // mc_dir
        LauncherConfig,
        u32, // java_major
    ),
    String,
> {
    let dl_config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let jdk_config: JdkConfig = persist::read(constants::SECT_JDK).unwrap_or_default();
    let launcher_config: LauncherConfig =
        persist::read(constants::SECT_LAUNCHER).unwrap_or_default();

    // minecraft_dir 可能是相对路径（默认 ".minecraft"），统一转绝对，
    // 避免 classpath / natives 路径基于 java 进程 cwd 解析出错
    let mc_dir = dl_config.minecraft_dir_abs();

    // 读取版本 JSON（扫描目录，文件名不一定是目录名），并展开 inheritsFrom 继承
    let vdir = mc_dir.join("versions").join(version_dir_name);
    if !vdir.is_dir() {
        return Err(format!("版本目录不存在\n{}", vdir.display()));
    }
    let version_json = load_version_json_with_inheritance(&mc_dir, version_dir_name)?;

    let required = version_json
        .java_version
        .major_version
        .as_u64()
        .unwrap_or(0) as u32;
    let java_env = match jdk_config.jdk_selection {
        JdkSelection::Manual { path } => jdk_config
            .discovered_jdks
            .iter()
            .find(|e| e.path == path)
            .cloned()
            .ok_or_else(|| "未找到指定的 JDK".to_string())?,
        JdkSelection::Auto => {
            auto_pick_jdk(&jdk_config.discovered_jdks, required).ok_or_else(|| {
                if required > 0 {
                    format!("需要 Java {}+，请在设置中选择或刷新 JDK", required)
                } else {
                    "未配置 JDK，请先在设置中选择 Java 路径".to_string()
                }
            })?
        }
    };
    // java_env.path 是 JAVA_HOME（目录），需要拼上 bin/java.exe
    let java_home = PathBuf::from(&java_env.path);
    if !java_home.exists() {
        return Err(format!("JDK 路径不存在\n{}", java_home.display()));
    }
    let java_path = crate::jdk::java_bin_path(&java_home);
    if !java_path.exists() {
        return Err(format!("未找到 Java 可执行文件\n{}", java_path.display()));
    }

    // 解析 Java 主版本号
    let java_major = crate::jdk::parse_java_major(&java_env.version)
        .ok_or_else(|| format!("无法解析 JDK 版本号\n{}", java_env.version))?;

    Ok((java_path, version_json, mc_dir, launcher_config, java_major))
}

/// 读取版本 JSON 并展开 inheritsFrom 继承。
/// HMCL/官方安装器等生成的 ModLoader 版本 json 常为继承式（只含自身库 +
/// inheritsFrom 指向原版，父库在父版本 json 里）——直接解析会缺父库导致
/// 启动失败。Elytra 自装版本（merge_loader_profile）已是完整 json 无继承，
/// 原样返回；父版本 json 缺失时明确报错（引导补装原版）。
fn load_version_json_with_inheritance(
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> Result<VersionJson, String> {
    let vdir = mc_dir.join("versions").join(version_dir_name);
    let vj_path = find_version_json(&vdir, version_dir_name)
        .ok_or_else(|| format!("版本目录下未找到版本 JSON\n{}", vdir.display()))?;
    let vj_text =
        std::fs::read_to_string(&vj_path).map_err(|e| format!("读取版本 JSON 失败\n{}", e))?;
    let mut vj_value: serde_json::Value =
        serde_json::from_str(&vj_text).map_err(|e| format!("解析版本 JSON 失败\n{}", e))?;

    // 展开继承链：子库 + 父库合并、子关键字段覆盖、删 inheritsFrom
    while let Some(parent_id) = vj_value.get("inheritsFrom").and_then(|v| v.as_str()) {
        let parent_path = mc_dir
            .join("versions")
            .join(parent_id)
            .join(format!("{parent_id}.json"));
        let parent_text = std::fs::read_to_string(&parent_path).map_err(|e| {
            format!(
                "读取父版本 JSON（{parent_id}）失败——该版本为继承式安装，请先用 Elytra 补装原版 {parent_id}\n{}",
                e
            )
        })?;
        vj_value = crate::downloader::mod_loader::expand_inheritance(
            &parent_text,
            vj_value,
            version_dir_name,
        )?;
    }

    serde_json::from_value(vj_value).map_err(|e| format!("解析版本 JSON 失败\n{}", e))
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
    let client_jar = vdir.join(format!("{}.jar", version_dir_name));
    let jar_ok = |p: &std::path::Path| p.metadata().map(|m| m.len() > 0).unwrap_or(false);
    if !(client_jar.exists() && jar_ok(&client_jar)) {
        // 目录内扫描 .jar 兜底（文件名可能不等于目录名）
        let found = find_file_in_dir(&vdir, "jar").filter(|p| jar_ok(p));
        if found.is_none() {
            missing.push(format!("客户端 JAR: {}", client_jar.display()));
        }
    }

    // 2. libraries（复用下载侧过滤 + sha1 校验：缺失/损坏都算）
    let lib_dir = mc_dir.join("libraries");
    let missing_libs =
        crate::downloader::library::missing_libraries(&version_json.libraries, &lib_dir);
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

    // 3. assets index
    let index_path = mc_dir
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", version_json.asset_index.id));
    if !index_path.exists() {
        missing.push(format!("资源索引: {}", index_path.display()));
    }

    if missing.is_empty() {
        Ok(())
    } else {
        // 第一行为大类短句（dialog 正文显示），文件列表进复制区
        Err(format!(
            "游戏文件不完整\n缺少以下文件：\n{}",
            missing.join("\n")
        ))
    }
}

/// options.txt 行 upsert：替换以 `key:` 开头的行，不存在则追加（文件不存在时创建）。
/// apply_game_language（lang）与 skin.rs（resourcePacks）共用。
pub fn upsert_option(options_path: &std::path::Path, key: &str, value: &str) {
    let line = format!("{key}:{value}");
    let content = match std::fs::read_to_string(options_path) {
        Ok(c) => c,
        Err(_) => {
            // 文件不存在：预写，游戏首次启动沿用
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

/// 游戏语言：自动设置为中文。
/// 1.13+ 游戏启动时读取 gameDir/options.txt 的 `lang` 字段，
/// 存在则替换，否则追加（文件不存在时创建，游戏启动后保留该值）。
pub fn apply_game_language(game_dir: &std::path::Path) {
    upsert_option(&game_dir.join("options.txt"), "lang", "zh_cn");
}

/// 从已扫描的 JDK 列表中自动选择最合适的版本。
/// 优先精确匹配需求版本；否则选最低但满足需求的版本；再不行选最高版本。
fn auto_pick_jdk(discovered: &[JdkInfo], required: u32) -> Option<JdkInfo> {
    let mut candidates: Vec<(u32, &JdkInfo)> = discovered
        .iter()
        .filter_map(|env| crate::jdk::parse_java_major(&env.version).map(|m| (m, env)))
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
    // 都不满足 → 用最高版本
    candidates.sort_by_key(|(m, _)| *m);
    Some(candidates.last().unwrap().1.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // 精确匹配
        assert_eq!(auto_pick_jdk(&list, 21).unwrap().path, "/jdk21");
        // 无精确匹配时选最低但满足的
        assert_eq!(auto_pick_jdk(&list, 17).unwrap().path, "/jdk17");
        assert_eq!(auto_pick_jdk(&list, 11).unwrap().path, "/jdk17");
        // 都不满足时用最高版本
        assert_eq!(auto_pick_jdk(&list, 22).unwrap().path, "/jdk21");
        // 空列表
        assert!(auto_pick_jdk(&[], 8).is_none());
    }

    /// 继承式版本（HMCL/官方安装器风格）：子 json 只有自身库 + inheritsFrom 父指针，
    /// 展开后应合并父库、子关键字段覆盖、父版本缺失时明确报错。
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
            vj.libraries[0].name, "a:b:1",
            "父库应在子库之前（classpath 顺序）"
        );
        assert_eq!(
            vj.main_class, "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "子 mainClass 应覆盖父"
        );

        // 父版本缺失：明确报错
        std::fs::remove_dir_all(&parent).unwrap();
        let err = load_version_json_with_inheritance(&tmp, "1.20.1-forge-47.1.0").unwrap_err();
        assert!(err.contains("父版本 JSON"), "应提示父版本缺失: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// find_version_json：PCL2 版本隔离实例目录混有游戏数据 json（字典序在前），
    /// 应跳过它们取到真正的版本 json（优先同名；否则含 mainClass 的）。
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
        let found = crate::config::launcher::args::find_version_json(&dir, "愚者");
        let name = found.unwrap().file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(name, "愚者.json", "应跳过游戏数据 json 取到版本 json");

        // 标准布局：同名 json 优先
        let dir2 = tmp.join("versions/1.20.1");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(
            dir2.join("1.20.1.json"),
            r#"{"mainClass":"net.minecraft.client.main.Main"}"#,
        )
        .unwrap();
        std::fs::write(dir2.join("extra.json"), r#"{"x":1}"#).unwrap();
        let found2 = crate::config::launcher::args::find_version_json(&dir2, "1.20.1");
        assert_eq!(found2.unwrap().file_name().unwrap(), "1.20.1.json");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
