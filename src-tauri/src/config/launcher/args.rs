use std::path::{Path, PathBuf};

use crate::config::launcher::model::{AuthMode, LauncherConfig, MemoryMode, GcType};
use crate::downloader::extract::{native_classifier_matches, os_classifier_key};
use crate::downloader::model::{OsRule, Rule, VersionJson};

/// 启动器标识
const LAUNCHER_NAME: &str = "Elytra";
const LAUNCHER_VERSION: &str = "0.1.0";
const LAUNCHER_CLIENT_ID: &str = "elytra-launcher";

/// 完整的启动命令，按执行顺序排列。
pub struct LaunchCommand {
    pub java_path: PathBuf,
    /// 完整命令行参数（exec 格式，第一个元素是 java 路径）
    pub args: Vec<String>,
}

impl LaunchCommand {
    pub fn to_args(&self) -> Vec<String> {
        self.args.clone()
    }
}

// ── 占位符替换上下文 ──────────────────────────────────

struct TemplateContext<'a> {
    auth_player_name: &'a str,
    version_name: &'a str,
    game_directory: &'a str,
    assets_root: &'a str,
    assets_index_name: &'a str,
    auth_uuid: &'a str,
    auth_access_token: &'a str,
    auth_xuid: &'a str,
    user_type: &'a str,
    version_type: &'a str,
    resolution_width: u32,
    resolution_height: u32,
    launcher_name: &'a str,
    launcher_version: &'a str,
}

/// 替换游戏参数模板中的 `${key}` 占位符。
fn substitute<'a>(template: &str, ctx: &TemplateContext<'a>) -> String {
    let mut s = template.to_string();
    s = s.replace("${auth_player_name}", ctx.auth_player_name);
    s = s.replace("${version_name}", ctx.version_name);
    s = s.replace("${game_directory}", ctx.game_directory);
    s = s.replace("${assets_root}", ctx.assets_root);
    s = s.replace("${assets_index_name}", ctx.assets_index_name);
    s = s.replace("${auth_uuid}", ctx.auth_uuid);
    s = s.replace("${auth_access_token}", ctx.auth_access_token);
    s = s.replace("${auth_xuid}", ctx.auth_xuid);
    s = s.replace("${user_type}", ctx.user_type);
    s = s.replace("${version_type}", ctx.version_type);
    s = s.replace("${resolution_width}", &ctx.resolution_width.to_string());
    s = s.replace("${resolution_height}", &ctx.resolution_height.to_string());
    s = s.replace("${launcher_name}", ctx.launcher_name);
    s = s.replace("${launcher_version}", ctx.launcher_version);
    s = s.replace("${clientid}", LAUNCHER_CLIENT_ID);
    s = s.replace("${quickPlayPath}", "");
    s
}

#[allow(clippy::too_many_arguments)]
/// 替换 JVM 参数模板中的 `${key}` 占位符。
fn substitute_jvm(
    template: &str,
    natives_dir: &str,
    lib_dir: &str,
    classpath: &str,
    classpath_sep: &str,
    version_name: &str,
    launcher_name: &str,
    launcher_version: &str,
) -> String {
    let mut s = template.to_string();
    s = s.replace("${natives_directory}", natives_dir);
    s = s.replace("${library_directory}", lib_dir);
    s = s.replace("${classpath}", classpath);
    s = s.replace("${classpath_separator}", classpath_sep);
    s = s.replace("${version_name}", version_name);
    s = s.replace("${launcher_name}", launcher_name);
    s = s.replace("${launcher_version}", launcher_version);
    s
}

// ── 规则匹配 ──────────────────────────────────────────

/// 检查规则的 features 是否匹配启动器提供的 feature 集。
///
/// 启动器支持的 feature（26.2 起官方 JSON 用 features 而非 os 区分参数）：
/// - `has_custom_resolution`: 由用户「窗口大小」设置决定（默认 false = 不传 --width/--height）
/// - 其余（is_demo_user / has_quick_plays_support / is_quick_play_*）: false
fn feature_match(features: &serde_json::Value, has_custom_resolution: bool) -> bool {
    let Some(obj) = features.as_object() else {
        return true;
    };
    obj.iter().all(|(k, v)| {
        let expected = v.as_bool().unwrap_or(false);
        match k.as_str() {
            "has_custom_resolution" => expected == has_custom_resolution,
            // 不支持的 feature：要求为 false
            _ => !expected,
        }
    })
}

/// 检查规则列表是否匹配当前 OS（Windows）+ features。
/// 规则语义：默认不允许，遇到 allow 且 os/features 匹配则允许，disallow 直接否决。
fn match_rules_json(rules: &[serde_json::Value], has_custom_resolution: bool) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_name = rule
            .get("os")
            .and_then(|o| o.get("name"))
            .and_then(|n| n.as_str());
        let os_match = match os_name {
            Some(n) => n == "windows",
            None => true,
        };
        let feat_match = match rule.get("features") {
            Some(f) => feature_match(f, has_custom_resolution),
            None => true,
        };
        match rule["action"].as_str() {
            Some("allow") if os_match && feat_match => allowed = true,
            Some("disallow") if os_match && feat_match => return false,
            _ => {}
        }
    }
    allowed
}

/// 解析 arguments 数组，返回展开后的参数列表（处理规则对象）。
/// `has_custom_resolution`: 用户是否设置了自定义窗口大小（决定 has_custom_resolution feature）。
fn extract_args(args: &[serde_json::Value], has_custom_resolution: bool) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Object(obj) => {
                let rules = match obj.get("rules").and_then(|r| r.as_array()) {
                    Some(r) => match_rules_json(r, has_custom_resolution),
                    None => true,
                };
                if !rules {
                    continue;
                }
                let value = match obj.get("value") {
                    Some(serde_json::Value::String(s)) => vec![s.clone()],
                    Some(serde_json::Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect(),
                    _ => continue,
                };
                out.extend(value);
            }
            _ => {}
        }
    }
    out
}

// ── 核心构建函数 ──────────────────────────────────────

/// 构建完整的 Minecraft 启动命令。
///
/// 参数说明：
/// - `java_path`: JDK 的 java.exe 路径
/// - `version_json`: 已解析的版本 JSON
/// - `version_dir_name`: 版本目录名（如 "1.20.1-fabric-0.15.11"）
/// - `mc_dir`: .minecraft 目录
/// - `config`: 启动器用户配置
/// - `auth`: 认证模式（离线/在线）
/// - `java_major_version`: JDK 主版本号（用于 JDK 22+ 的特殊 flag）
#[allow(clippy::too_many_arguments)]
pub fn build_launch_command(
    java_path: &Path,
    version_json: &VersionJson,
    version_dir_name: &str,
    mc_dir: &Path,
    config: &LauncherConfig,
    auth: &AuthMode,
    java_major_version: u32,
) -> LaunchCommand {
    // ── 路径 ──
    // natives 目录统一放在 .minecraft/natives/<ver>/（首次启动解压一次，后续复用）
    let natives_dir = mc_dir.join("natives").join(&version_json.id);
    let lib_dir = mc_dir.join("libraries");
    let assets_dir = mc_dir.join("assets");
    let version_dir = mc_dir.join("versions").join(version_dir_name);
    // 版本隔离：每个版本使用独立游戏目录（存档/配置隔离）
    let game_directory = version_dir.clone();
    // 窗口大小：默认不传 --width/--height（has_custom_resolution feature 为 false）
    let (resolution_width, resolution_height, has_custom_resolution) = match config.window_size {
        crate::config::launcher::model::WindowSize::Custom(w, h) => (w, h, true),
        // 占位值仅供模板替换兜底（带 feature 的参数组已被过滤）
        crate::config::launcher::model::WindowSize::Default => (854, 480, false),
    };
    // JAR 文件名不一定等于目录名（目录可能因冲突被重命名）
    let client_jar = find_jar_in_dir(&version_dir)
        .unwrap_or_else(|| version_dir.join(format!("{}.jar", version_dir_name)));
    let classpath_sep = ";";

    // ── Classpath ──
    let classpath = build_classpath(version_json, &lib_dir, &client_jar);
    let classpath_str = classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_sep);

    // ── JVM 参数 ──
    let mut jvm_preamble: Vec<String> = Vec::new(); // -cp 之前
    let mut system_props: Vec<String> = Vec::new(); // -D 类
    let mut jvm_tuning: Vec<String> = Vec::new();   // -cp 之后、mainClass 之前

    // 从版本 JSON 的 arguments.jvm 提取
    if let Some(jvm_args) = &version_json.arguments.jvm {
        let raw = extract_args(jvm_args, has_custom_resolution);
        for arg in raw {
            // 跳过 -cp / -classpath / ${classpath}（classpath 由我们自行构建，
            // 若 ${classpath} 被替换后不跳过，会变成 -cp 之前的裸 classpath 被 java 当主类）
            if arg == "-cp" || arg == "-classpath" || arg == "${classpath}" {
                continue;
            }
            let substituted = substitute_jvm(
                &arg,
                &natives_dir.to_string_lossy(),
                &lib_dir.to_string_lossy(),
                &classpath_str,
                classpath_sep,
                &version_json.id,
                LAUNCHER_NAME,
                LAUNCHER_VERSION,
            );
            // 跳过 -Xmx / -Xms（内存由我们统一设置）
            if substituted.starts_with("-Xmx") || substituted.starts_with("-Xms") {
                continue;
            }
            if substituted.starts_with("-D") {
                system_props.push(substituted);
            } else {
                jvm_preamble.push(substituted);
            }
        }
    }

    // JDK 22+ 前置兼容标志（官方 arguments.jvm 可能已包含，去重）
    if java_major_version >= 22 {
        push_unique(&mut jvm_preamble, "--sun-misc-unsafe-memory-access=allow".into());
        push_unique(&mut jvm_preamble, "--enable-native-access=ALL-UNNAMED".into());
    }

    // Native 库系统属性：旧式版本设置 -Djava.library.path（目录由启动前 ensure 保证就绪）
    add_native_props(&mut system_props, &natives_dir, version_json);

    // 启动器品牌标识（官方 arguments.jvm 可能已包含，去重）
    push_unique(&mut system_props, format!("-Dminecraft.launcher.brand={}", LAUNCHER_NAME));
    push_unique(&mut system_props, format!("-Dminecraft.launcher.version={}", LAUNCHER_VERSION));

    // 编码
    system_props.push("-Dstdout.encoding=UTF-8".into());
    system_props.push("-Dstderr.encoding=UTF-8".into());

    // 游戏窗口标题（1.13+ 读取 minecraft.window.title 系统属性；空 = 不改）
    if !config.window_title.is_empty() {
        system_props.push(format!("-Dminecraft.window.title={}", config.window_title));
    }

    // 内存
    let mem_mb = match config.memory {
        MemoryMode::Auto => guess_auto_memory(&version_dir),
        MemoryMode::Custom(mb) => mb,
    };
    jvm_tuning.push(format!("-Xmx{}m", mem_mb));

    // GC 参数
    jvm_tuning.extend(gc_args(&config.gc_type));

    // 用户自定义 JVM 参数（放在 -cp 后面，mainClass 前面）
    let mut user_jvm: Vec<String> = Vec::new();
    for arg in &config.jvm_args {
        // 禁用 LWJGL Unsafe Agent：剔除 -javaagent:lwjgl-unsafe-agent 类参数
        if config.disable_lwjgl_unsafe_agent
            && arg.starts_with("-javaagent:")
            && arg.to_lowercase().contains("lwjgl-unsafe-agent")
        {
            continue;
        }
        user_jvm.push(arg.clone());
    }
    jvm_tuning.extend(user_jvm);

    // ── 主类 ──
    let mut main_class = version_json.main_class.clone();
    // 禁用 Java Launch Wrapper：老版本（1.6-1.12）主类为 launchwrapper，直接改用客户端主类绕过
    if config.disable_launch_wrapper && main_class == "net.minecraft.launchwrapper.Launch" {
        main_class = "net.minecraft.client.main.Main".to_string();
    }

    // ── 游戏参数 ──
    // 离线模式使用固定占位 UUID/Token，在线模式使用真实的微软返回值
    let (auth_user, auth_uuid, auth_token, auth_xuid) = match auth {
        AuthMode::Offline { username } => (
            username.as_str(),
            "00000000FFFFFFFFFFFFFFFFFFFFFF96436",
            "00000000FFFFFFFFFFFFFFFFFFFFFF96436",
            "",
        ),
        AuthMode::Online { access_token, uuid, xuid } => {
            ("", uuid.as_str(), access_token.as_str(), xuid.as_str())
        }
    };

    let game_ctx = TemplateContext {
        auth_player_name: auth_user,
        version_name: &version_json.id,
        game_directory: &game_directory.to_string_lossy(),
        assets_root: &assets_dir.to_string_lossy(),
        assets_index_name: &version_json.asset_index.id,
        auth_uuid,
        auth_access_token: auth_token,
        auth_xuid,
        user_type: "msa",
        version_type: &version_json.r#type,
        resolution_width,
        resolution_height,
        launcher_name: LAUNCHER_NAME,
        launcher_version: LAUNCHER_VERSION,
    };

    let game_args = match &version_json.arguments.game {
        Some(game_arr) if !game_arr.is_empty() => {
            // 新格式：arguments.game 数组
            let raw = extract_args(game_arr, has_custom_resolution);
            raw.iter().map(|a| substitute(a, &game_ctx)).collect()
        }
        _ => {
            // 旧格式：minecraftArguments 模板字符串
            if let Some(tpl) = &version_json.minecraft_arguments {
                vec![substitute(tpl, &game_ctx)]
            } else {
                // 最低保底
                let mut base = vec![
                    "--username".into(),
                    game_ctx.auth_player_name.to_string(),
                    "--version".into(),
                    game_ctx.version_name.to_string(),
                    "--gameDir".into(),
                    game_ctx.game_directory.to_string(),
                    "--assetsDir".into(),
                    game_ctx.assets_root.to_string(),
                    "--assetIndex".into(),
                    game_ctx.assets_index_name.to_string(),
                    "--uuid".into(),
                    game_ctx.auth_uuid.to_string(),
                    "--accessToken".into(),
                    game_ctx.auth_access_token.to_string(),
                    "--userType".into(),
                    game_ctx.user_type.to_string(),
                    "--versionType".into(),
                    game_ctx.version_type.to_string(),
                ];
                if has_custom_resolution {
                    base.extend([
                        "--width".into(),
                        game_ctx.resolution_width.to_string(),
                        "--height".into(),
                        game_ctx.resolution_height.to_string(),
                    ]);
                }
                base
            }
        }
    };

    // ── 组装完整命令行 ──
    let mut args = Vec::new();
    args.push(java_path.to_string_lossy().to_string());
    args.extend(jvm_preamble);
    args.extend(system_props);
    args.push("-cp".to_string());
    args.push(classpath_str);
    args.extend(jvm_tuning);
    args.push(main_class);
    args.extend(game_args);
    args.extend(config.game_args.iter().cloned());

    LaunchCommand {
        java_path: java_path.to_path_buf(),
        args,
    }
}

// ── 辅助函数 ──────────────────────────────────────────

/// 构建 classpath：libraries 的 artifact + 新式 classifier + client.jar
fn build_classpath(
    version_json: &VersionJson,
    lib_dir: &Path,
    client_jar: &Path,
) -> Vec<PathBuf> {
    let mut cp = Vec::new();

    for lib in &version_json.libraries {
        // 规则过滤
        if let Some(rules) = &lib.rules {
            if !match_rules(rules) {
                continue;
            }
        }
        // 架构匹配（排除 x86/arm64 等非当前架构的 native 变体）
        if !native_classifier_matches(&lib.name) {
            continue;
        }
        let Some(dls) = &lib.downloads else { continue };

        // artifact JAR（通用 JAR）
        if let Some(artifact) = &dls.artifact {
            cp.push(lib_dir.join(&artifact.path));
        }

        // 新式 native classifier JAR（无 natives 字段）
        if lib.natives.is_none() {
            if let Some(classifiers) = &dls.classifiers {
                if let Some(key) = os_classifier_key(None) {
                    if let Some(artifact) = classifiers.get(key) {
                        cp.push(lib_dir.join(&artifact.path));
                    }
                }
            }
        }
    }

    // client.jar
    cp.push(client_jar.to_path_buf());

    cp
}

/// 将 Library 的 Rule 列表匹配当前 OS
fn match_rules(rules: &[Rule]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(OsRule { name: Some(n) }) => n == "windows",
            _ => true,
        };
        match rule.action.as_str() {
            "allow" if os_match => allowed = true,
            "disallow" if os_match => return false,
            _ => {}
        }
    }
    allowed
}

/// 仅在列表不含相同参数时追加。
fn push_unique(list: &mut Vec<String>, arg: String) {
    if !list.iter().any(|a| a == &arg) {
        list.push(arg);
    }
}

/// 检查版本是否包含旧式 natives 库（lwjgl 2.x，有 `natives` 字段）。
pub fn has_old_style_natives(version_json: &VersionJson) -> bool {
    version_json.libraries.iter().any(|lib| {
        lib.natives
            .as_ref()
            .map(|n| n.contains_key("windows"))
            .unwrap_or(false)
    })
}

/// 确保 natives 目录就绪：所有版本都创建 `.minecraft/natives/<ver>/` 根目录
/// （官方 arguments.jvm 的 SharedLibraryExtractPath 等参数指向它，LWJGL 需要目录存在）。
/// 旧式版本（lwjgl 2.x）额外把 natives classifier JAR 解压到该目录。
pub fn ensure_old_style_natives(
    version_json: &VersionJson,
    lib_dir: &Path,
    natives_dir: &Path,
) -> Result<(), String> {
    // 所有版本：确保 natives 根目录存在（LWJGL/Netty 运行时在其中创建子目录）
    std::fs::create_dir_all(natives_dir)
        .map_err(|e| format!("创建 natives 目录失败: {}", e))?;

    if !has_old_style_natives(version_json) {
        return Ok(());
    }

    // 旧式：已解压过（目录有内容）→ 直接复用
    let natives_ready = std::fs::read_dir(natives_dir)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false);
    if natives_ready {
        return Ok(());
    }

    let mut errors: Vec<String> = Vec::new();
    for lib in &version_json.libraries {
        let Some(classifier_key) = lib
            .natives
            .as_ref()
            .and_then(|n| n.get("windows"))
        else {
            continue;
        };
        let Some(classifiers) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()) else {
            continue;
        };
        let Some(artifact) = classifiers.get(classifier_key) else { continue };
        let jar = lib_dir.join(&artifact.path);
        if !jar.exists() {
            errors.push(format!("native JAR 缺失: {}", artifact.path));
            continue;
        }
        if let Err(e) = crate::downloader::extract::extract_jar(
            &jar,
            natives_dir,
            lib.extract.as_ref(),
        ) {
            errors.push(format!("{}: {}", lib.name, e));
        }
    }

    // 解压后目录仍为空 → 解压失败
    let has_content = std::fs::read_dir(natives_dir)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false);
    if !has_content {
        errors.push("natives 解压结果为空".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        // 第一行为大类短句（dialog 正文显示），细节进复制区
        Err(format!("natives 准备失败\n{}", errors.join("\n")))
    }
}

/// 添加 native 相关系统属性。
/// 仅旧式版本设置 `-Djava.library.path`（natives 目录由 ensure_old_style_natives 保证就绪）。
fn add_native_props(props: &mut Vec<String>, natives_dir: &Path, version_json: &VersionJson) {
    if !has_old_style_natives(version_json) {
        return;
    }
    props.push(format!(r#"-Djava.library.path={}"#, natives_dir.to_string_lossy()));
}

/// 根据 GC 类型返回对应的 JVM 参数
fn gc_args(gc: &GcType) -> Vec<String> {
    match gc {
        GcType::PreferZgc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseZGC".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::PreferGenerationalZgc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseZGC".into(),
            "-XX:+ZGenerational".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::StandardG1Gc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseG1GC".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::TunedG1Gc => vec![
            "-XX:+UnlockExperimentalVMOptions".into(),
            "-XX:+UseG1GC".into(),
            "-XX:G1NewSizePercent=20".into(),
            "-XX:G1ReservePercent=20".into(),
            "-XX:G1HeapRegionSize=32M".into(),
            "-XX:MaxGCPauseMillis=50".into(),
            "-XX:+PerfDisableSharedMem".into(),
        ],
        GcType::Custom => vec![],
    }
}

/// 统计版本目录 mods/ 下的 mod 数量（.jar 文件数）。
fn count_mods(version_dir: &Path) -> u32 {
    std::fs::read_dir(version_dir.join("mods"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "jar").unwrap_or(false))
                .count() as u32
        })
        .unwrap_or(0)
}

/// 自动推测游戏内存（MB）：按剩余内存 + mod 数量动态分配。
///
/// 策略（-Xmx 是堆上限，给多不预占，核心约束是别超过系统剩余）：
/// - 基准比例 60%（0 mod）；每 mod +0.3%，60 个 mod 封顶（最多 78%）
/// - 封顶剩余内存的 85%（留呼吸空间防 OOM）；下限 2GB
fn guess_auto_memory(version_dir: &Path) -> u32 {
    auto_alloc_memory(available_system_memory_mb(), count_mods(version_dir))
}

/// 自动分配公式（纯函数，便于测试）：剩余内存 × 比例，封顶 85%，下限 2GB。
fn auto_alloc_memory(available_mb: u64, mods: u32) -> u32 {
    let ratio = 0.60 + 0.003 * (mods.min(60) as f64);
    let alloc = (available_mb as f64 * ratio).min(available_mb as f64 * 0.85);
    alloc.max(2048.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用临时目录模拟版本目录，写入指定数量的 mod jar。
    fn dir_with_mods(n: u32) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("elytra-mods-test-{}", std::process::id()));
        let mods = d.join("mods");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&mods).unwrap();
        for i in 0..n {
            std::fs::write(mods.join(format!("mod{i}.jar")), b"x").unwrap();
        }
        // 非 jar 文件不应计入
        std::fs::write(mods.join("config.json"), b"{}").unwrap();
        d
    }

    #[test]
    fn test_count_mods_only_jars() {
        let d = dir_with_mods(5);
        assert_eq!(count_mods(&d), 5);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_auto_memory_strategy() {
        let avail = 9216u64; // 剩余 9GB
        // 0 mod → 60% = 5529MB
        assert_eq!(auto_alloc_memory(avail, 0), (9216.0 * 0.60) as u32);
        // 30 mod → 69%
        assert_eq!(auto_alloc_memory(avail, 30), (9216.0 * 0.69) as u32);
        // 60 mod → 78% 封顶（比例不再随 mod 增长）
        assert_eq!(auto_alloc_memory(avail, 60), (9216.0 * 0.78) as u32);
        assert_eq!(auto_alloc_memory(avail, 80), (9216.0 * 0.78) as u32);
        // 大 mod 数不超过 85% 封顶
        assert!(auto_alloc_memory(avail, 1000) <= (9216.0 * 0.85) as u32);
    }

    #[test]
    fn test_auto_memory_min_floor() {
        // 剩余 2.5GB：60% = 1536MB → 下限 2048MB；剩余 5GB 时正常分配
        assert_eq!(auto_alloc_memory(2560, 0), 2048);
        assert_eq!(auto_alloc_memory(2560, 30), 2048); // 66% 仍低于下限
        assert_eq!(auto_alloc_memory(5120, 0), (5120.0 * 0.60) as u32);
    }
}

#[cfg(target_os = "windows")]
fn available_system_memory_mb() -> u64 {
    // 使用 Windows API 获取物理内存
    unsafe {
        #[allow(clippy::upper_case_acronyms)]
        type DWORD = u32;
        #[allow(clippy::upper_case_acronyms)]
        type BOOL = i32;
        #[repr(C)]
        #[allow(clippy::upper_case_acronyms)]
        struct MEMORYSTATUSEX {
            dw_length: DWORD,
            dw_memory_load: DWORD,
            ull_total_phys: u64,
            ull_avail_phys: u64,
            ull_total_page_file: u64,
            ull_avail_page_file: u64,
            ull_total_virtual: u64,
            ull_avail_virtual: u64,
            ull_avail_extended_virtual: u64,
        }
        extern "system" {
            fn GlobalMemoryStatusEx(lp_buffer: *mut MEMORYSTATUSEX) -> BOOL;
        }
        let mut status = MEMORYSTATUSEX {
            dw_length: std::mem::size_of::<MEMORYSTATUSEX>() as DWORD,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };
        if GlobalMemoryStatusEx(&mut status) != 0 {
            status.ull_avail_phys / (1024 * 1024)
        } else {
            4096
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn available_system_memory_mb() -> u64 {
    4096
}

/// 在版本目录中扫描第一个 `.jar` 文件（与目录名可能不同）。
pub fn find_jar_in_dir(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).find_map(|e| {
        let p = e.path();
        if p.extension()? == "jar" && p.is_file() { Some(p) } else { None }
    })
}

