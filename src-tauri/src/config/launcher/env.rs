//! 启动环境准备：加载配置、解析版本 JSON、完整性检查、JDK 匹配、游戏语言。
//! 命令层（commands/launch.rs）的启动前置逻辑集中于此。

use std::path::PathBuf;

use crate::config::downloader::model::DownloaderConfig;
use crate::config::jdk::model::{JavaConfig, JavaEnv};
use crate::config::launcher::args::find_file_in_dir;
use crate::config::launcher::model::LauncherConfig;
use crate::downloader::model::VersionJson;
use crate::persist;

/// 加载全部持久化配置 + 解析版本 JSON，供 launch/spawn 共用。
#[allow(clippy::type_complexity)]
pub fn load_launch_config(
    version_dir_name: &str,
) -> Result<
    (
        PathBuf,          // java_path
        VersionJson,      // version_json
        PathBuf,          // mc_dir
        LauncherConfig,
        u32,              // java_major
    ),
    String,
> {
    let dl_config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let jdk_config: JavaConfig = persist::read("jdk").unwrap_or_default();
    let launcher_config: LauncherConfig = persist::read("launcher").unwrap_or_default();

    // minecraft_dir 可能是相对路径（默认 ".minecraft"），统一转绝对，
    // 避免 classpath / natives 路径基于 java 进程 cwd 解析出错
    let mc_dir = dl_config.minecraft_dir_abs();

    // 读取版本 JSON（扫描目录，文件名不一定是目录名）
    let vdir = mc_dir.join("versions").join(version_dir_name);
    if !vdir.is_dir() {
        return Err(format!("版本目录不存在: {}", vdir.display()));
    }
    let vj_path = find_file_in_dir(&vdir, "json")
        .ok_or_else(|| format!("版本目录下未找到 JSON 文件: {}", vdir.display()))?;
    let vj_text = std::fs::read_to_string(&vj_path)
        .map_err(|e| format!("读取版本 JSON 失败: {}", e))?;
    let version_json: VersionJson =
        serde_json::from_str(&vj_text).map_err(|e| format!("解析版本 JSON 失败: {}", e))?;

    // 确定 JDK：优先使用 active_jdk，否则按游戏版本需求自动匹配
    let required = version_json
        .java_version
        .major_version
        .as_u64()
        .unwrap_or(0) as u32;
    let java_env = match jdk_config.active_jdk.clone() {
        Some(env) => env,
        None => auto_pick_jdk(&jdk_config.discovered_jdks, required).ok_or_else(|| {
            if required > 0 {
                format!("需要 Java {}+，请在设置中选择或刷新 JDK", required)
            } else {
                "未配置 JDK，请先在设置中选择 Java 路径".to_string()
            }
        })?,
    };
    // java_env.path 是 JAVA_HOME（目录），需要拼上 bin/java.exe
    let java_home = PathBuf::from(&java_env.path);
    if !java_home.exists() {
        return Err(format!("JDK 路径不存在: {}", java_home.display()));
    }
    let java_exe = if cfg!(windows) { "java.exe" } else { "java" };
    let java_path = java_home.join("bin").join(java_exe);
    if !java_path.exists() {
        return Err(format!("未找到 Java 可执行文件: {}", java_path.display()));
    }

    // 解析 Java 主版本号
    let java_major = parse_java_major(&java_env.version)
        .ok_or_else(|| format!("无法解析 JDK 版本号: {}", java_env.version))?;

    Ok((java_path, version_json, mc_dir, launcher_config, java_major))
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
    let jar_ok = |p: &std::path::Path| {
        p.metadata().map(|m| m.len() > 0).unwrap_or(false)
    };
    if !(client_jar.exists() && jar_ok(&client_jar)) {
        // 目录内扫描 .jar 兜底（文件名可能不等于目录名）
        let found = find_file_in_dir(&vdir, "jar").filter(|p| jar_ok(p));
        if found.is_none() {
            missing.push(format!("客户端 JAR: {}", client_jar.display()));
        }
    }

    // 2. libraries（复用下载侧过滤 + sha1 校验：缺失/损坏都算）
    let lib_dir = mc_dir.join("libraries");
    let lib_pending = crate::downloader::library::count_pending(&version_json.libraries, &lib_dir);
    if lib_pending > 0 {
        missing.push(format!("缺失支持库 {lib_pending} 个（含 sha1 校验）"));
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
        Err(format!("游戏文件不完整\n缺少以下文件：\n{}", missing.join("\n")))
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
fn auto_pick_jdk(discovered: &[JavaEnv], required: u32) -> Option<JavaEnv> {
    let mut candidates: Vec<(u32, &JavaEnv)> = discovered
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
    // 都不满足 → 用最高版本
    candidates.sort_by_key(|(m, _)| *m);
    Some(candidates.last().unwrap().1.clone())
}

/// 从版本字符串中解析主版本号。
/// 支持格式："17.0.8" → 17, "21" → 21, "25-ea" → 25, "1.8.0_202" → 8
fn parse_java_major(version: &str) -> Option<u32> {
    // "1.x" 格式（Java 8 及更早，如 "1.8.0_202" → 8）
    if let Some(v) = version.strip_prefix("1.") {
        return v.split('.').next()?.parse::<u32>().ok();
    }
    // 新格式："17.0.8" → 17
    version
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|s| s.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_java_major() {
        assert_eq!(parse_java_major("1.8.0_202"), Some(8));
        assert_eq!(parse_java_major("1.7.0"), Some(7));
        assert_eq!(parse_java_major("1.6.0"), Some(6));
        assert_eq!(parse_java_major("17.0.8"), Some(17));
        assert_eq!(parse_java_major("21"), Some(21));
        assert_eq!(parse_java_major("25-ea"), Some(25));
        assert_eq!(parse_java_major("11.0.2"), Some(11));
    }
}
