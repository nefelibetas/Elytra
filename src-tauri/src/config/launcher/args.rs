use std::path::{Path, PathBuf};

use crate::config::launcher::memory;
use crate::config::launcher::model::{AuthMode, LauncherConfig, MemoryMode};
use crate::constants;
use crate::downloader::extract::{
    legacy_classifier_artifact, matches_rules, modern_native_matches, modern_natives_key,
    NativesKind,
};
use crate::downloader::model::VersionJson;

// ── 启动协议常量（集中维护，防拼写错误导致静默失败）──

// 游戏启动参数名（1.13+ camelCase，旧版 JSON token 模板用连字符版）
const ARG_USERNAME: &str = "--username";
const ARG_VERSION: &str = "--version";
const ARG_GAME_DIR: &str = "--gameDir";
const ARG_ASSETS_DIR: &str = "--assetsDir";
const ARG_ASSET_INDEX: &str = "--assetIndex";
const ARG_UUID: &str = "--uuid";
const ARG_ACCESS_TOKEN: &str = "--accessToken";
const ARG_USER_TYPE: &str = "--userType";
const ARG_VERSION_TYPE: &str = "--versionType";
const ARG_WIDTH: &str = "--width";
const ARG_HEIGHT: &str = "--height";
const ARG_USER_PROPERTIES: &str = "--userProperties";
const ARG_USER_PROPERTIES_LEGACY: &str = "--user-properties";
const ARG_PROFILE_PROPERTIES: &str = "--profileProperties";

// JVM 系统属性键（-D 前缀 + 值；拼接场景用带 = 前缀常量）
const SYS_LAUNCHER_BRAND: &str = "-Dminecraft.launcher.brand=";
const SYS_LAUNCHER_VERSION: &str = "-Dminecraft.launcher.version=";
const SYS_JAVA_LIBRARY_PATH: &str = "-Djava.library.path=";
const SYS_WINDOW_TITLE: &str = "-Dminecraft.window.title=";
const SYS_STDOUT_ENCODING: &str = "-Dstdout.encoding=UTF-8";
const SYS_STDERR_ENCODING: &str = "-Dstderr.encoding=UTF-8";
const SYS_USE_SYSTEM_PROXIES: &str = "-Djava.net.useSystemProxies=true";
const SYS_USER_LANGUAGE_ZH: &str = "-Duser.language=zh_CN";
const SYS_USER_COUNTRY_ZH: &str = "-Duser.country=CN";

// JVM 标志与跳过前缀
const JVM_FLAG_SUN_MISC_UNSAFE: &str = "--sun-misc-unsafe-memory-access=allow";
const JVM_FLAG_NATIVE_ACCESS: &str = "--enable-native-access=ALL-UNNAMED";
const ARG_CP: &str = "-cp";
const ARG_CP_LONG: &str = "-classpath";
const PREFIX_XMX: &str = "-Xmx";
const PREFIX_XMS: &str = "-Xms";
const PREFIX_JAVA_AGENT: &str = "-javaagent:";
const SUBSTR_LWJGL_UNSAFE_AGENT: &str = "lwjgl-unsafe-agent";

// 类名 / Maven 坐标前缀
const CLASS_LAUNCHWRAPPER: &str = "net.minecraft.launchwrapper.Launch";
const CLASS_CLIENT_MAIN: &str = "net.minecraft.client.main.Main";
const CLASS_PREFIX_FABRIC: &str = "net.fabricmc";
const SUBSTR_FABRIC_LOADER: &str = "fabric-loader";
const MAVEN_NEOFORGE: &str = "net.neoforged:neoforge:";
const MAVEN_NEOFORGE_UNIVERSAL: &str = ":universal";

// 模板占位符（与官方 arguments 模板一致；${quickPlayPath} 启动器不支持 → 替换为空串）
const PLACEHOLDER_AUTH_PLAYER_NAME: &str = "${auth_player_name}";
const PLACEHOLDER_VERSION_NAME: &str = "${version_name}";
const PLACEHOLDER_GAME_DIRECTORY: &str = "${game_directory}";
const PLACEHOLDER_ASSETS_ROOT: &str = "${assets_root}";
const PLACEHOLDER_ASSETS_INDEX_NAME: &str = "${assets_index_name}";
const PLACEHOLDER_AUTH_UUID: &str = "${auth_uuid}";
const PLACEHOLDER_AUTH_ACCESS_TOKEN: &str = "${auth_access_token}";
const PLACEHOLDER_AUTH_XUID: &str = "${auth_xuid}";
const PLACEHOLDER_USER_TYPE: &str = "${user_type}";
const PLACEHOLDER_USER_PROPERTIES: &str = "${user_properties}";
const PLACEHOLDER_VERSION_TYPE: &str = "${version_type}";
const PLACEHOLDER_RESOLUTION_WIDTH: &str = "${resolution_width}";
const PLACEHOLDER_RESOLUTION_HEIGHT: &str = "${resolution_height}";
const PLACEHOLDER_LAUNCHER_NAME: &str = "${launcher_name}";
const PLACEHOLDER_LAUNCHER_VERSION: &str = "${launcher_version}";
const PLACEHOLDER_CLIENT_ID: &str = "${clientid}";
const PLACEHOLDER_QUICK_PLAY_PATH: &str = "${quickPlayPath}";
const PLACEHOLDER_NATIVES_DIRECTORY: &str = "${natives_directory}";
const PLACEHOLDER_LIBRARY_DIRECTORY: &str = "${library_directory}";
const PLACEHOLDER_CLASSPATH: &str = "${classpath}";
const PLACEHOLDER_CLASSPATH_SEPARATOR: &str = "${classpath_separator}";

// 文件标记 / 扩展名
const NATIVES_READY_MARKER: &str = ".elytra-ready";
const EXT_JAR: &str = "jar";
const EXT_JSON: &str = "json";
const JSON_MAIN_CLASS_KEY: &str = "\"mainClass\"";

// 离线模式固定值（token 为占位，游戏仅校验非空；用户类型协议枚举）
const OFFLINE_TOKEN_PLACEHOLDER: &str = "00000000FFFFFFFFFFFFFFFFFFFFFF96436";
const USER_TYPE_LEGACY: &str = "legacy";
const USER_TYPE_MSA: &str = "msa";
const EMPTY_JSON_OBJECT: &str = "{}";
const DEFAULT_RESOLUTION_WIDTH: u32 = 854;
const DEFAULT_RESOLUTION_HEIGHT: u32 = 480;

/// 游戏语言：自动中文（与前端 Settings 联动）
const LANG_AUTO_ZH_CN: &str = "auto_zh_CN";

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
    /// `--user-properties` 参数值（离线皮肤注入点；1.19.2 以下版本消费）
    user_properties: &'a str,
    version_type: &'a str,
    resolution_width: u32,
    resolution_height: u32,
    launcher_name: &'a str,
    launcher_version: &'a str,
}

/// 替换游戏参数模板中的 `${key}` 占位符。
fn substitute<'a>(template: &str, ctx: &TemplateContext<'a>) -> String {
    let mut s = template.to_string();
    s = s.replace(PLACEHOLDER_AUTH_PLAYER_NAME, ctx.auth_player_name);
    s = s.replace(PLACEHOLDER_VERSION_NAME, ctx.version_name);
    s = s.replace(PLACEHOLDER_GAME_DIRECTORY, ctx.game_directory);
    s = s.replace(PLACEHOLDER_ASSETS_ROOT, ctx.assets_root);
    s = s.replace(PLACEHOLDER_ASSETS_INDEX_NAME, ctx.assets_index_name);
    s = s.replace(PLACEHOLDER_AUTH_UUID, ctx.auth_uuid);
    s = s.replace(PLACEHOLDER_AUTH_ACCESS_TOKEN, ctx.auth_access_token);
    s = s.replace(PLACEHOLDER_AUTH_XUID, ctx.auth_xuid);
    s = s.replace(PLACEHOLDER_USER_TYPE, ctx.user_type);
    s = s.replace(PLACEHOLDER_USER_PROPERTIES, ctx.user_properties);
    s = s.replace(PLACEHOLDER_VERSION_TYPE, ctx.version_type);
    s = s.replace(
        PLACEHOLDER_RESOLUTION_WIDTH,
        &ctx.resolution_width.to_string(),
    );
    s = s.replace(
        PLACEHOLDER_RESOLUTION_HEIGHT,
        &ctx.resolution_height.to_string(),
    );
    s = s.replace(PLACEHOLDER_LAUNCHER_NAME, ctx.launcher_name);
    s = s.replace(PLACEHOLDER_LAUNCHER_VERSION, ctx.launcher_version);
    s = s.replace(PLACEHOLDER_CLIENT_ID, constants::CLIENT_ID);
    s = s.replace(PLACEHOLDER_QUICK_PLAY_PATH, "");
    s
}

#[allow(clippy::too_many_arguments)]
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
    s = s.replace(PLACEHOLDER_NATIVES_DIRECTORY, natives_dir);
    s = s.replace(PLACEHOLDER_LIBRARY_DIRECTORY, lib_dir);
    s = s.replace(PLACEHOLDER_CLASSPATH, classpath);
    s = s.replace(PLACEHOLDER_CLASSPATH_SEPARATOR, classpath_sep);
    s = s.replace(PLACEHOLDER_VERSION_NAME, version_name);
    s = s.replace(PLACEHOLDER_LAUNCHER_NAME, launcher_name);
    s = s.replace(PLACEHOLDER_LAUNCHER_VERSION, launcher_version);
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
            Some(n) => n == crate::downloader::extract::TARGET_OS,
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
    user_properties: &str,
) -> LaunchCommand {
    // ── 路径 ──
    // natives 目录按 MC 版本共享（同 MC 的 vanilla/ModLoader 版本 native 库一致，
    // 避免每个版本在 natives/<版本id>/ 各解压一份 LWJGL 运行时产物，约 16MB/份）
    let natives_dir = mc_dir.join("natives").join(mc_version_of(version_dir_name));
    let lib_dir = mc_dir.join("libraries");
    let assets_dir = mc_dir.join("assets");
    let version_dir = mc_dir.join("versions").join(version_dir_name);
    // 版本隔离：每个版本使用独立游戏目录（存档/配置隔离）
    let game_directory = version_dir.clone();
    // 窗口大小：默认不传 --width/--height（has_custom_resolution feature 为 false）
    let (resolution_width, resolution_height, has_custom_resolution) = match config.window_size {
        crate::config::launcher::model::WindowSize::Custom(w, h) => (w, h, true),
        // 占位值仅供模板替换兜底（带 feature 的参数组已被过滤）
        crate::config::launcher::model::WindowSize::Default => {
            (DEFAULT_RESOLUTION_WIDTH, DEFAULT_RESOLUTION_HEIGHT, false)
        }
    };
    // JAR 文件名不一定等于目录名（目录可能因冲突被重命名）
    let client_jar = find_file_in_dir(&version_dir, EXT_JAR)
        .unwrap_or_else(|| version_dir.join(format!("{}.{}", version_dir_name, EXT_JAR)));
    // classpath 分隔符按平台区分：Windows 用 ';'，Unix（Linux/macOS）用 ':'。
    // 硬编码 ';' 会让 Java 把整串 jar 当成单个路径，导致 ClassNotFoundException
    // （与 mod_loader/forge.rs 的处理保持一致）
    let classpath_sep = if cfg!(windows) { ";" } else { ":" };

    // ── Classpath ──
    let mut classpath = build_classpath(version_json, &lib_dir, &client_jar);
    if version_json.main_class.contains(CLASS_PREFIX_FABRIC)
        && !classpath
            .iter()
            .any(|p| p.to_string_lossy().contains(SUBSTR_FABRIC_LOADER))
    {
        append_pcl_fabric_libs(&mut classpath, &lib_dir);
    }
    let classpath_str = classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_sep);

    // ── JVM 参数（preamble / system props / tuning 三段）──
    let (jvm_preamble, system_props, jvm_tuning) = collect_jvm_args(
        version_json,
        &natives_dir,
        &lib_dir,
        &classpath_str,
        java_major_version,
        config,
        &version_dir,
        has_custom_resolution,
    );

    // ── 主类 ──
    let mut main_class = version_json.main_class.clone();
    // 禁用 Java Launch Wrapper：老版本（1.6-1.12）主类为 launchwrapper，直接改用客户端主类绕过
    if config.disable_launch_wrapper && main_class == CLASS_LAUNCHWRAPPER {
        main_class = CLASS_CLIENT_MAIN.to_string();
    }

    // ── 游戏参数 ──
    // 离线模式 UUID 由离线皮肤设置生成（匹配 Steve/Alex 模型或正版玩家真实 UUID），
    // Token 使用固定占位；在线模式使用真实的微软返回值
    let (auth_user, auth_uuid, auth_token, auth_xuid, user_type) = match auth {
        AuthMode::Offline { username, uuid } => (
            username.as_str(),
            uuid.as_str(),
            OFFLINE_TOKEN_PLACEHOLDER,
            "",
            USER_TYPE_LEGACY,
        ),
        AuthMode::Online {
            username,
            access_token,
            uuid,
            xuid,
        } => (
            username.as_str(),
            uuid.as_str(),
            access_token.as_str(),
            xuid.as_str(),
            USER_TYPE_MSA,
        ),
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
        user_type,
        // 空值用空 JSON（老版本 json 的 token 不能被替换为空串，Main 解析会崩）
        user_properties: if user_properties.is_empty() {
            EMPTY_JSON_OBJECT
        } else {
            user_properties
        },
        version_type: &version_json.r#type,
        resolution_width,
        resolution_height,
        launcher_name: constants::LAUNCHER_BRAND,
        launcher_version: constants::APP_VERSION,
    };

    // ── 游戏参数（新格式数组 / 旧格式模板 / 保底参数）──
    let game_args = build_game_args(
        version_json,
        &game_ctx,
        has_custom_resolution,
        user_properties,
    );

    let mut args = Vec::new();
    args.push(java_path.to_string_lossy().to_string());
    args.extend(jvm_preamble);
    args.extend(system_props);
    args.push(ARG_CP.to_string());
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
fn build_classpath(version_json: &VersionJson, lib_dir: &Path, client_jar: &Path) -> Vec<PathBuf> {
    let mut cp = Vec::new();

    for lib in &version_json.libraries {
        // 规则过滤
        if let Some(rules) = &lib.rules {
            if !matches_rules(rules) {
                continue;
            }
        }
        // 新式 natives：库名后缀过滤非当前平台的架构变体（排除 x86/arm64 等）
        if lib.natives_kind() == Some(NativesKind::Modern) && !modern_native_matches(&lib.name) {
            continue;
        }
        let Some(dls) = &lib.downloads else { continue };

        // artifact JAR（通用 JAR）
        if let Some(artifact) = &dls.artifact {
            cp.push(lib_dir.join(&artifact.path));
        }

        // 新式 natives 且带 classifiers：取当前平台 JAR（历史兼容分支，现代数据无此结构）
        if lib.natives_kind() == Some(NativesKind::Modern) {
            if let Some(classifiers) = &dls.classifiers {
                if let Some(key) = modern_natives_key() {
                    if let Some(artifact) = classifiers.get(&key) {
                        cp.push(lib_dir.join(&artifact.path));
                    }
                }
            }
        }
    }

    // client.jar（NeoForge 除外）：NeoForge 官方部署中 patched/混淆 vanilla 均不进 classpath，
    // GameLocator 生产分支经 -DlibraryDirectory 直接从 libraries 目录加载 minecraft-client-patched；
    // 若把版本目录的混淆 vanilla 放进 classpath，RequiredSystemFiles 会在其上找到全部系统文件，
    // 误判为 dev 环境报 "Minecraft-Dists attribute" 缺失。
    let is_neoforge = version_json
        .libraries
        .iter()
        .any(|l| l.name.starts_with(MAVEN_NEOFORGE) && l.name.ends_with(MAVEN_NEOFORGE_UNIVERSAL));
    if !is_neoforge {
        cp.push(client_jar.to_path_buf());
    }

    cp
}

/// 语义化版本比较（"9.10.1" > "9.6" > "9.2"）：按 '.' 分段数字比较。
fn version_gt(a: &str, b: &str) -> bool {
    let pa: Vec<u64> = a.split('.').filter_map(|s| s.parse().ok()).collect();
    let pb: Vec<u64> = b.split('.').filter_map(|s| s.parse().ok()).collect();
    for i in 0..pa.len().max(pb.len()) {
        let va = pa.get(i).copied().unwrap_or(0);
        let vb = pb.get(i).copied().unwrap_or(0);
        if va != vb {
            return va > vb;
        }
    }
    false
}

/// PCL2 版本隔离实例兜底（见 build_launch_command 触发条件）：
/// 补 net/fabricmc/（fabric-loader + sponge-mixin 全部 jar，目录结构
/// net/fabricmc/{group}/{version}/）与 org/ow2/asm/（每个 artifact 取最高版本
/// jar——lib 目录常有多版本并存，全加会按 classpath 顺序先命中旧版导致
/// NoSuchMethodError）。已存在的路径跳过（防重复）。
fn append_pcl_fabric_libs(cp: &mut Vec<PathBuf>, lib_dir: &Path) {
    let mut push_jar = |p: PathBuf| {
        if p.extension().is_some_and(|e| e == EXT_JAR) && !cp.contains(&p) {
            cp.push(p);
        }
    };
    let fabricmc = lib_dir.join("net").join("fabricmc");
    if let Ok(groups) = std::fs::read_dir(&fabricmc) {
        for group in groups.flatten() {
            let Ok(vers) = std::fs::read_dir(group.path()) else {
                continue;
            };
            for ver in vers.flatten() {
                let Ok(files) = std::fs::read_dir(ver.path()) else {
                    continue;
                };
                for f in files.flatten() {
                    push_jar(f.path());
                }
            }
        }
    }
    let asm_dir = lib_dir.join("org").join("ow2").join("asm");
    if let Ok(artifacts) = std::fs::read_dir(&asm_dir) {
        for artifact in artifacts.flatten() {
            let Ok(vers) = std::fs::read_dir(artifact.path()) else {
                continue;
            };
            let mut best: Option<(String, PathBuf)> = None;
            for ver in vers.flatten() {
                if !ver.path().is_dir() {
                    continue;
                }
                let name = ver.file_name().to_string_lossy().to_string();
                if best.as_ref().is_none_or(|(bn, _)| version_gt(&name, bn)) {
                    best = Some((name, ver.path()));
                }
            }
            if let Some((_, dir)) = best {
                let Ok(files) = std::fs::read_dir(dir) else {
                    continue;
                };
                for f in files.flatten() {
                    push_jar(f.path());
                }
            }
        }
    }
}

/// 检查版本是否包含旧式 natives 库（lwjgl 2.x，有 `natives` 字段）。
fn push_unique(list: &mut Vec<String>, arg: String) {
    if !list.iter().any(|a| a == &arg) {
        list.push(arg);
    }
}

/// 检查版本是否包含旧式 natives 库（lwjgl 2.x，有 `natives` 字段）。
pub fn has_old_style_natives(version_json: &VersionJson) -> bool {
    version_json
        .libraries
        .iter()
        .any(|l| l.natives_kind() == Some(NativesKind::Legacy))
}

/// 从版本目录名提取 MC 版本（natives 目录按 MC 版本共享——同一 MC 的 vanilla/ModLoader
/// 版本 native 库一致，避免每个版本在 natives/<版本id>/ 各解压一份（LWJGL 运行时解压产物约 16MB/份）。
pub(crate) use crate::util::mc_version_from_dir as mc_version_of;

/// 确保 natives 目录就绪：所有版本都创建 `.minecraft/natives/<mc>/` 根目录
/// （官方 arguments.jvm 的 SharedLibraryExtractPath 等参数指向它，LWJGL 需要目录存在）。
/// 旧式版本（lwjgl 2.x）额外把 natives classifier JAR 解压到该目录。
pub fn ensure_old_style_natives(
    version_json: &VersionJson,
    lib_dir: &Path,
    natives_dir: &Path,
) -> Result<(), String> {
    // 所有版本：确保 natives 根目录存在（LWJGL/Netty 运行时在其中创建子目录）
    std::fs::create_dir_all(natives_dir).map_err(|e| format!("创建 natives 目录失败\n{}", e))?;

    if !has_old_style_natives(version_json) {
        return Ok(());
    }

    // 旧式：解压完成标记（.elytra-ready）存在 → 直接复用。
    // 只查“目录非空”不可靠：上次解压中断会留下部分文件，误判为已就绪，
    // 启动后缺原生库才暴露。marker 在全部解压成功后写入。
    let ready_marker = natives_dir.join(NATIVES_READY_MARKER);
    if ready_marker.exists() {
        return Ok(());
    }

    let mut errors: Vec<String> = Vec::new();
    for lib in &version_json.libraries {
        // 仅旧式（natives 字段）参与解压；has_old_style_natives 保证版本整体含旧式库
        let Some(artifact) = legacy_classifier_artifact(lib) else {
            continue;
        };
        let jar = lib_dir.join(&artifact.path);
        if !jar.exists() {
            errors.push(format!("native JAR 缺失: {}", artifact.path));
            continue;
        }
        if let Err(e) =
            crate::downloader::extract::extract_jar(&jar, natives_dir, lib.extract.as_ref())
        {
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
        std::fs::write(&ready_marker, "")
            .map_err(|e| format!("写入 natives 完成标记失败\n{}", e))?;
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
    props.push(format!(
        "{}{}",
        SYS_JAVA_LIBRARY_PATH,
        natives_dir.to_string_lossy()
    ));
}

/// 在目录中扫描第一个指定扩展名的文件（文件名可能与目录名不同，如目录被重命名）。
pub fn find_file_in_dir(dir: &std::path::Path, ext: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|e| {
            let p = e.path();
            if p.extension()? == ext && p.is_file() {
                Some(p)
            } else {
                None
            }
        })
}

/// 定位版本 JSON：优先 `{dir_name}.json`（标准布局），否则扫描目录内
/// 含 `"mainClass"` 键的 json——PCL2 版本隔离实例目录混有游戏数据 json
/// （TrashSlotSaveState/patchouli_data/usercache 等），字典序第一个
/// 可能不是版本 json，直接解析会报 "missing field assetIndex"。
pub fn find_version_json(dir: &std::path::Path, dir_name: &str) -> Option<std::path::PathBuf> {
    let named = dir.join(format!("{dir_name}.{EXT_JSON}"));
    if named.is_file() {
        return Some(named);
    }
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|e| {
            let p = e.path();
            if p.extension()? == EXT_JSON && p.is_file() {
                let text = std::fs::read_to_string(&p).ok()?;
                if text.contains(JSON_MAIN_CLASS_KEY) {
                    return Some(p);
                }
            }
            None
        })
}

/// 组装 JVM 参数三段：
/// - preamble：-cp 之前（版本 JSON 参数 + JDK 22+ 兼容标志）
/// - system_props：-D 类（natives/品牌/编码/代理/窗口标题/语言）
/// - tuning：-cp 之后、mainClass 之前（内存/GC/用户自定义）
#[allow(clippy::too_many_arguments)]
fn collect_jvm_args(
    version_json: &VersionJson,
    natives_dir: &Path,
    lib_dir: &Path,
    classpath_str: &str,
    java_major_version: u32,
    config: &LauncherConfig,
    version_dir: &Path,
    has_custom_resolution: bool,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut jvm_preamble: Vec<String> = Vec::new(); // -cp 之前
    let mut system_props: Vec<String> = Vec::new(); // -D 类
    let mut jvm_tuning: Vec<String> = Vec::new(); // -cp 之后、mainClass 之前
    // 与上方 classpath 构建保持一致：按平台选择分隔符（Windows ';' / Unix ':'）
    let classpath_sep = if cfg!(windows) { ";" } else { ":" };

    // 从版本 JSON 的 arguments.jvm 提取
    if let Some(jvm_args) = &version_json.arguments.jvm {
        let raw = extract_args(jvm_args, has_custom_resolution);
        for arg in raw {
            // 跳过 -cp / -classpath / ${classpath}（classpath 由我们自行构建，
            // 若 ${classpath} 被替换后不跳过，会变成 -cp 之前的裸 classpath 被 java 当主类）
            if arg == ARG_CP || arg == ARG_CP_LONG || arg == PLACEHOLDER_CLASSPATH {
                continue;
            }
            let substituted = substitute_jvm(
                &arg,
                &natives_dir.to_string_lossy(),
                &lib_dir.to_string_lossy(),
                classpath_str,
                classpath_sep,
                &version_json.id,
                constants::LAUNCHER_BRAND,
                constants::APP_VERSION,
            );
            // 跳过 -Xmx / -Xms（内存由我们统一设置）
            if substituted.starts_with(PREFIX_XMX) || substituted.starts_with(PREFIX_XMS) {
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
        push_unique(&mut jvm_preamble, JVM_FLAG_SUN_MISC_UNSAFE.into());
        push_unique(&mut jvm_preamble, JVM_FLAG_NATIVE_ACCESS.into());
    }

    // Native 库系统属性：旧式版本设置 -Djava.library.path（目录由启动前 ensure 保证就绪）
    add_native_props(&mut system_props, natives_dir, version_json);

    // 启动器品牌标识（官方 arguments.jvm 可能已包含，去重）
    push_unique(
        &mut system_props,
        format!("{}{}", SYS_LAUNCHER_BRAND, constants::LAUNCHER_BRAND),
    );
    push_unique(
        &mut system_props,
        format!("{}{}", SYS_LAUNCHER_VERSION, constants::APP_VERSION),
    );

    // 编码
    system_props.push(SYS_STDOUT_ENCODING.into());
    system_props.push(SYS_STDERR_ENCODING.into());

    // 游戏进程读取 Windows 系统代理（梯子/PAC 场景下 sessionserver/textures 才能拉取皮肤）
    system_props.push(SYS_USE_SYSTEM_PROXIES.into());

    // 游戏窗口标题（1.13+ 读取 minecraft.window.title 系统属性；空 = 不改）
    if !config.window_title.is_empty() {
        system_props.push(format!("{}{}", SYS_WINDOW_TITLE, config.window_title));
    }

    // 游戏语言：自动设置为中文（-Duser.language 影响游戏/资源本地化）
    if config.game_language == LANG_AUTO_ZH_CN {
        system_props.push(SYS_USER_LANGUAGE_ZH.into());
        system_props.push(SYS_USER_COUNTRY_ZH.into());
    }

    // 内存
    let mem_mb = match config.memory {
        MemoryMode::Auto => memory::guess_auto_memory(version_dir),
        MemoryMode::Custom(mb) => mb,
    };
    jvm_tuning.push(format!("{}{}m", PREFIX_XMX, mem_mb));

    // GC 参数
    jvm_tuning.extend(memory::gc_args(&config.gc_type));

    // 用户自定义 JVM 参数（放在 -cp 后面，mainClass 前面）
    let mut user_jvm: Vec<String> = Vec::new();
    for arg in &config.jvm_args {
        // 禁用 LWJGL Unsafe Agent：剔除 -javaagent:lwjgl-unsafe-agent 类参数
        if config.disable_lwjgl_unsafe_agent
            && arg.starts_with(PREFIX_JAVA_AGENT)
            && arg.to_lowercase().contains(SUBSTR_LWJGL_UNSAFE_AGENT)
        {
            continue;
        }
        user_jvm.push(arg.clone());
    }
    jvm_tuning.extend(user_jvm);

    (jvm_preamble, system_props, jvm_tuning)
}

/// 生成游戏参数：新格式 arguments.game 数组（含 feature 过滤）/ 旧格式
/// minecraftArguments 模板（空白拆分）/ 无参数版本的最低保底参数。
/// 随后按需注入 --userProperties / --profileProperties（1.17+ 官方 json 已移除 token，
/// Main 仍解析；参数名 1.17+ 为 camelCase，旧版为连字符 --user-properties（由 json token
/// 替换覆盖，不重复追加）；1.18+ 的 Minecraft.getProfileProperties 直接消费
/// --profileProperties（非空即注入纹理），1.19.3+ Main 移除解析
/// （allowsUnrecognizedOptions 静默忽略，与 PCL「仅 1.19.2 以下」一致）。
fn build_game_args(
    version_json: &VersionJson,
    game_ctx: &TemplateContext<'_>,
    has_custom_resolution: bool,
    user_properties: &str,
) -> Vec<String> {
    let mut game_args = match &version_json.arguments.game {
        Some(game_arr) if !game_arr.is_empty() => {
            // 新格式：arguments.game 数组
            let raw = extract_args(game_arr, has_custom_resolution);
            raw.iter().map(|a| substitute(a, game_ctx)).collect()
        }
        _ => {
            // 旧格式：minecraftArguments 模板字符串
            if let Some(tpl) = &version_json.minecraft_arguments {
                // 旧格式：模板是空格分隔的参数串，替换后按空白拆分（与官方启动器一致）
                substitute(tpl, game_ctx)
                    .split_whitespace()
                    .map(String::from)
                    .collect()
            } else {
                // 最低保底
                let mut base = vec![
                    ARG_USERNAME.into(),
                    game_ctx.auth_player_name.to_string(),
                    ARG_VERSION.into(),
                    game_ctx.version_name.to_string(),
                    ARG_GAME_DIR.into(),
                    game_ctx.game_directory.to_string(),
                    ARG_ASSETS_DIR.into(),
                    game_ctx.assets_root.to_string(),
                    ARG_ASSET_INDEX.into(),
                    game_ctx.assets_index_name.to_string(),
                    ARG_UUID.into(),
                    game_ctx.auth_uuid.to_string(),
                    ARG_ACCESS_TOKEN.into(),
                    game_ctx.auth_access_token.to_string(),
                    ARG_USER_TYPE.into(),
                    game_ctx.user_type.to_string(),
                    ARG_VERSION_TYPE.into(),
                    game_ctx.version_type.to_string(),
                ];
                if has_custom_resolution {
                    base.extend([
                        ARG_WIDTH.into(),
                        game_ctx.resolution_width.to_string(),
                        ARG_HEIGHT.into(),
                        game_ctx.resolution_height.to_string(),
                    ]);
                }
                base
            }
        }
    };

    // 主动注入 user properties（见函数注释，1.17+ 与 1.19.3+ 语义差异）
    if !user_properties.is_empty()
        && !game_args
            .iter()
            .any(|a| a == ARG_USER_PROPERTIES_LEGACY || a == ARG_USER_PROPERTIES)
    {
        game_args.push(ARG_USER_PROPERTIES.into());
        game_args.push(user_properties.to_string());
        game_args.push(ARG_PROFILE_PROPERTIES.into());
        game_args.push(user_properties.to_string());
    }
    game_args
}
