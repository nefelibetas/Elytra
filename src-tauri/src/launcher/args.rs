use std::path::{Path, PathBuf};

use crate::auth::external::{is_ely_by, AUTHLIB_JAR};
use crate::constants;
use crate::game::resource::extract::{
    extract_jar, legacy_classifier_artifact, matches_rules, modern_native_matches,
    modern_natives_key, NativesKind, TARGET_OS,
};
use crate::game::version::model::VersionJson;
use crate::launcher::memory;
use crate::launcher::model::{AuthMode, LaunchOptions, MemoryMode, WindowSize};

// ── 启动协议常量 ──

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
const ARG_SERVER: &str = "--server";
const ARG_PORT: &str = "--port";

const SYS_LAUNCHER_BRAND: &str = "-Dminecraft.launcher.brand=";
const SYS_LAUNCHER_VERSION: &str = "-Dminecraft.launcher.version=";
const SYS_JAVA_LIBRARY_PATH: &str = "-Djava.library.path=";
const SYS_WINDOW_TITLE: &str = "-Dminecraft.window.title=";
const SYS_STDOUT_ENCODING: &str = "-Dstdout.encoding=UTF-8";
const SYS_STDERR_ENCODING: &str = "-Dstderr.encoding=UTF-8";
const SYS_USE_SYSTEM_PROXIES: &str = "-Djava.net.useSystemProxies=true";
const SYS_USER_LANGUAGE_ZH: &str = "-Duser.language=zh_CN";
const SYS_USER_COUNTRY_ZH: &str = "-Duser.country=CN";

const JVM_FLAG_SUN_MISC_UNSAFE: &str = "--sun-misc-unsafe-memory-access=allow";
const JVM_FLAG_NATIVE_ACCESS: &str = "--enable-native-access=ALL-UNNAMED";
const ARG_CP: &str = "-cp";
const ARG_CP_LONG: &str = "-classpath";
const PREFIX_XMX: &str = "-Xmx";
const PREFIX_XMS: &str = "-Xms";
const PREFIX_JAVA_AGENT: &str = "-javaagent:";
const SUBSTR_LWJGL_UNSAFE_AGENT: &str = "lwjgl-unsafe-agent";

pub(crate) const CLASS_LAUNCHWRAPPER: &str = "net.minecraft.launchwrapper.Launch";
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

const LANG_AUTO_ZH_CN: &str = "auto_zh_CN";

pub struct LaunchCommand {
    pub java_path: PathBuf,
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

struct JvmSubst<'a> {
    natives_dir: &'a str,
    lib_dir: &'a str,
    classpath: &'a str,
    classpath_sep: &'a str,
    version_name: &'a str,
    launcher_name: &'a str,
    launcher_version: &'a str,
}

fn substitute_jvm(template: &str, sub: &JvmSubst<'_>) -> String {
    let JvmSubst {
        natives_dir,
        lib_dir,
        classpath,
        classpath_sep,
        version_name,
        launcher_name,
        launcher_version,
    } = *sub;
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

/// 规则 features 是否匹配启动器 feature 集（26.2 起官方 JSON 用 features 区分参数）；仅 `has_custom_resolution` 依用户窗口大小（默认 false = 不传 --width/--height），其余一律 false。
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

/// 规则列表是否匹配当前 OS（Windows）+ features：默认不允许；allow 且匹配则允许，disallow 直接否决。
fn match_rules_json(rules: &[serde_json::Value], has_custom_resolution: bool) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_name = rule
            .get("os")
            .and_then(|o| o.get("name"))
            .and_then(|n| n.as_str());
        let os_match = match os_name {
            Some(n) => n == TARGET_OS,
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

pub struct LaunchRequest<'a> {
    pub version_dir_name: &'a str,
    pub mc_dir: &'a Path,
    pub config: &'a LaunchOptions,
    pub auth: &'a AuthMode,
    pub java_major_version: u32,
    pub user_properties: &'a str,
}

pub fn build_launch_command(
    java_path: &Path,
    version_json: &VersionJson,
    req: &LaunchRequest<'_>,
) -> LaunchCommand {
    let LaunchRequest {
        version_dir_name,
        mc_dir,
        config,
        auth,
        java_major_version,
        user_properties,
    } = *req;
    // ── 路径 ──
    let paths = resolve_launch_paths(version_dir_name, mc_dir, config);

    // ── Classpath ──
    let classpath_str = build_classpath_string(
        version_json,
        &paths.lib_dir,
        &paths.client_jar,
        paths.classpath_sep,
    );

    // ── JVM 参数（preamble / system props / tuning 三段）──
    let (jvm_preamble, system_props, jvm_tuning) = collect_jvm_args(&JvmArgs {
        version_json,
        natives_dir: &paths.natives_dir,
        lib_dir: &paths.lib_dir,
        classpath_str: &classpath_str,
        java_major_version,
        config,
        version_dir: &paths.version_dir,
        has_custom_resolution: paths.has_custom_resolution,
    });

    // ── 主类 ──
    let main_class = resolve_main_class(version_json, config);

    // ── 游戏参数 ──
    let game_args = build_game_args_from_auth(version_json, auth, user_properties, &paths);

    let mut args = Vec::new();
    args.push(java_path.to_string_lossy().to_string());
    args.extend(jvm_preamble);
    args.extend(system_props);
    args.push(ARG_CP.to_string());
    args.push(classpath_str);
    args.extend(jvm_tuning);
    push_external_auth_args(&mut args, auth);
    args.push(main_class);
    args.extend(game_args);
    // 服务器直连：追加到游戏参数末尾，用户自定义 game_args 之前
    push_server_args(&mut args, config);

    LaunchCommand {
        java_path: java_path.to_path_buf(),
        args,
    }
}

// ── 启动命令阶段函数 ──────────────────────────────────

struct LaunchPaths {
    natives_dir: PathBuf,
    lib_dir: PathBuf,
    assets_dir: PathBuf,
    version_dir: PathBuf,
    game_directory: PathBuf,
    client_jar: PathBuf,
    classpath_sep: &'static str,
    resolution_width: u32,
    resolution_height: u32,
    has_custom_resolution: bool,
}

fn resolve_launch_paths(
    version_dir_name: &str,
    mc_dir: &Path,
    config: &LaunchOptions,
) -> LaunchPaths {
    // natives 按 MC 版本共享（同 MC 的 vanilla/ModLoader native 库一致），避免每版本重复解压 LWJGL 产物（~16MB/份）
    let natives_dir = mc_dir.join("natives").join(mc_version_of(version_dir_name));
    let lib_dir = mc_dir.join("libraries");
    let assets_dir = mc_dir.join("assets");
    let version_dir = mc_dir.join("versions").join(version_dir_name);
    let game_directory = version_dir.clone();
    // 窗口大小：默认不传 --width/--height（has_custom_resolution feature 为 false）
    let (resolution_width, resolution_height, has_custom_resolution) = match config.window_size {
        WindowSize::Custom(w, h) => (w, h, true),
        // 占位值仅供模板替换兜底（带 feature 的参数组已被过滤）
        WindowSize::Default => (DEFAULT_RESOLUTION_WIDTH, DEFAULT_RESOLUTION_HEIGHT, false),
    };
    // JAR 文件名不一定等于目录名；与启动前完整性检查共用 resolve_client_jar，避免两处选中不同文件
    let client_jar = resolve_client_jar(&version_dir, version_dir_name)
        .unwrap_or_else(|| version_dir.join(format!("{}.{}", version_dir_name, EXT_JAR)));
    // classpath 分隔符按平台区分：Windows ';' / Unix ':'（硬编码 ';' → ClassNotFoundException；与 mod_loader/forge.rs 一致）
    let classpath_sep = if cfg!(windows) { ";" } else { ":" };

    LaunchPaths {
        natives_dir,
        lib_dir,
        assets_dir,
        version_dir,
        game_directory,
        client_jar,
        classpath_sep,
        resolution_width,
        resolution_height,
        has_custom_resolution,
    }
}

fn build_classpath_string(
    version_json: &VersionJson,
    lib_dir: &Path,
    client_jar: &Path,
    classpath_sep: &str,
) -> String {
    let mut classpath = build_classpath(version_json, lib_dir, client_jar);
    if version_json.main_class.contains(CLASS_PREFIX_FABRIC)
        && !classpath
            .iter()
            .any(|p| p.to_string_lossy().contains(SUBSTR_FABRIC_LOADER))
    {
        append_pcl_fabric_libs(&mut classpath, lib_dir);
    }
    classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_sep)
}

/// 主类：禁用 Java Launch Wrapper 时，老版本（1.6-1.12）的 launchwrapper 直接改用客户端主类绕过。
fn resolve_main_class(version_json: &VersionJson, config: &LaunchOptions) -> String {
    let mut main_class = version_json.main_class.clone();
    if config.disable_launch_wrapper && main_class == CLASS_LAUNCHWRAPPER {
        main_class = CLASS_CLIENT_MAIN.to_string();
    }
    main_class
}

fn build_game_args_from_auth(
    version_json: &VersionJson,
    auth: &AuthMode,
    user_properties: &str,
    paths: &LaunchPaths,
) -> Vec<String> {
    // 离线 UUID 由离线皮肤设置生成（匹配 Steve/Alex 模型或正版 UUID），token 用固定占位；在线用真实微软返回值
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
        AuthMode::External {
            username,
            access_token,
            uuid,
            ..
        } => (
            username.as_str(),
            uuid.as_str(),
            access_token.as_str(),
            "",
            USER_TYPE_MSA,
        ),
    };

    let game_ctx = TemplateContext {
        auth_player_name: auth_user,
        version_name: &version_json.id,
        game_directory: &paths.game_directory.to_string_lossy(),
        assets_root: &paths.assets_dir.to_string_lossy(),
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
        resolution_width: paths.resolution_width,
        resolution_height: paths.resolution_height,
        launcher_name: constants::LAUNCHER_BRAND,
        launcher_version: constants::APP_VERSION,
    };

    build_game_args(
        version_json,
        &game_ctx,
        paths.has_custom_resolution,
        user_properties,
    )
}

/// 第三方登录（authlib-injector）：javaagent 注入 + client side（jar 下载到数据目录；缺失由启动前置检查拦截）。
fn push_external_auth_args(args: &mut Vec<String>, auth: &AuthMode) {
    if let AuthMode::External { api_url, .. } = auth {
        let jar = crate::util::config_dir().join(AUTHLIB_JAR);
        // Ely.by 用 authlib-injector 内置别名；其余标准站传完整 URL
        let server = if is_ely_by(api_url) {
            "ely.by".to_string()
        } else {
            api_url.clone()
        };
        args.push(format!("-javaagent:{}={}", jar.display(), server));
        args.push("-Dauthlibinjector.side=client".to_string());
    }
}

fn push_server_args(args: &mut Vec<String>, config: &LaunchOptions) {
    if let Some(server) = config.server.as_deref() {
        let server = server.trim();
        if !server.is_empty() {
            args.push(ARG_SERVER.to_string());
            args.push(server.to_string());
        }
    }
    if let Some(port) = config.port {
        args.push(ARG_PORT.to_string());
        args.push(port.to_string());
    }
    args.extend(config.game_args.iter().cloned());
}

// ── 辅助函数 ──────────────────────────────────────────

fn build_classpath(version_json: &VersionJson, lib_dir: &Path, client_jar: &Path) -> Vec<PathBuf> {
    let mut cp = Vec::new();

    for lib in &version_json.libraries {
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

        // artifact JAR：按落地路径去重（下面 classifiers 分支可能指向同一 native jar）
        if let Some(artifact) = &dls.artifact {
            let p = lib_dir.join(&artifact.path);
            if !cp.contains(&p) {
                cp.push(p);
            }
        }

        // 新式 natives 且带 classifiers：取当前平台 JAR（历史兼容分支，现代数据无此结构）
        if lib.natives_kind() == Some(NativesKind::Modern) {
            if let Some(classifiers) = &dls.classifiers {
                if let Some(key) = modern_natives_key() {
                    if let Some(artifact) = classifiers.get(&key) {
                        let p = lib_dir.join(&artifact.path);
                        if !cp.contains(&p) {
                            cp.push(p);
                        }
                    }
                }
            }
        }
    }

    // client.jar（NeoForge 除外）：其 patched 混淆 vanilla 不进 classpath（GameLocator 经 -DlibraryDirectory 加载），否则误判 dev 环境报 "Minecraft-Dists" 缺失。
    let is_neoforge = version_json
        .libraries
        .iter()
        .any(|l| l.name.starts_with(MAVEN_NEOFORGE) && l.name.ends_with(MAVEN_NEOFORGE_UNIVERSAL));
    if !is_neoforge {
        cp.push(client_jar.to_path_buf());
    }

    cp
}

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

/// PCL2 版本隔离实例兜底：补 net/fabricmc/ 全部 jar 与 org/ow2/asm/ 各 artifact 最高版本 jar（全加会先命中旧版 → NoSuchMethodError）；已存在路径跳过。
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

/// 去重追加：`list` 中尚不存在 `arg` 时压入（保序，避免重复的启动参数）。
fn push_unique(list: &mut Vec<String>, arg: String) {
    if !list.iter().any(|a| a == &arg) {
        list.push(arg);
    }
}

pub fn has_old_style_natives(version_json: &VersionJson) -> bool {
    version_json
        .libraries
        .iter()
        .any(|l| l.natives_kind() == Some(NativesKind::Legacy))
}

/// 从版本目录名提取 MC 版本（natives 目录按 MC 版本共享，避免每版本各解压一份 LWJGL 产物，~16MB/份）。
pub(crate) use crate::util::mc_version_from_dir as mc_version_of;

/// natives 就绪标记指纹：参与解压的旧式 native 库（名字+落地路径+提取规则）排序后哈希；库增删/换版本/改规则 → 重建。
fn natives_fingerprint(version_json: &VersionJson) -> String {
    let mut parts: Vec<String> = version_json
        .libraries
        .iter()
        .filter_map(|lib| {
            let artifact = legacy_classifier_artifact(lib)?;
            let extract = lib
                .extract
                .as_ref()
                .map(|e| e.exclude.join(","))
                .unwrap_or_default();
            Some(format!("{}|{}|{}", lib.name, artifact.path, extract))
        })
        .collect();
    parts.sort_unstable();
    crate::downloader::hash::sha1_hex(parts.join("\n").as_bytes())
}

fn clear_dir_contents(dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(common_err_ctx_key!(
        "errors.file.read",
        "读取 natives 目录失败"
    ))?;
    for entry in entries {
        let path = entry
            .map_err(common_err_ctx_key!(
                "errors.file.read",
                "读取 natives 目录项失败"
            ))?
            .path();
        let res = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(e) = res {
            // 目录里可能残留游戏运行中占用的 .dll：明确指出是哪一项删不掉
            return Err(keyed_msg!(
                "errors.launch.nativesCleanFailed",
                "清理旧 natives 失败\n{}\n{e}",
                path.display()
            ));
        }
    }
    Ok(())
}

/// 原子写标记：先写临时文件再改名，避免半写内容被当成有效指纹。
fn write_marker(marker: &Path, fingerprint: &str) -> Result<(), String> {
    let tmp = marker.with_extension("tmp");
    std::fs::write(&tmp, fingerprint).map_err(common_err_ctx_key!(
        "errors.file.write",
        "写入 natives 标记失败"
    ))?;
    if marker.exists() {
        // Windows 的 rename 不覆盖已存在文件；先删旧标记（删后崩溃 → 无标记 → 下次重建，安全）
        std::fs::remove_file(marker).map_err(common_err_ctx_key!(
            "errors.file.delete",
            "清理旧 natives 标记失败"
        ))?;
    }
    std::fs::rename(&tmp, marker).map_err(common_err_ctx_key!(
        "errors.file.write",
        "提交 natives 标记失败"
    ))?;
    Ok(())
}

/// 确保 natives 目录就绪：所有版本都创建 `.minecraft/natives/<mc>/`（官方 jvm 的 SharedLibraryExtractPath 指向它）；旧式版本（lwjgl 2.x）额外解压 natives classifier JAR。
pub fn ensure_old_style_natives(
    version_json: &VersionJson,
    lib_dir: &Path,
    natives_dir: &Path,
) -> Result<(), String> {
    // 所有版本：确保 natives 根目录存在（LWJGL/Netty 运行时在其中创建子目录）
    std::fs::create_dir_all(natives_dir).map_err(common_err_ctx_key!(
        "errors.file.createDir",
        "创建 natives 目录失败"
    ))?;

    if !has_old_style_natives(version_json) {
        return Ok(());
    }

    // 标记语义是「本版本所需 native 集合已就绪」而非「该目录解压过」：native 集合不同时旧标记会跳过解压 → 缺原生库（UnsatisfiedLibraryError），故存指纹。
    let fingerprint = natives_fingerprint(version_json);
    let ready_marker = natives_dir.join(NATIVES_READY_MARKER);
    let marker_ok = std::fs::read_to_string(&ready_marker)
        .map(|c| c.trim() == fingerprint)
        .unwrap_or(false);
    if marker_ok {
        return Ok(());
    }

    // 指纹不一致（或旧版遗留空标记）：清空目录再解压——旧 .dll 残留可能被 Windows DLL 搜索顺序加载到。
    clear_dir_contents(natives_dir)?;

    let mut errors: Vec<String> = Vec::new();
    for lib in &version_json.libraries {
        let Some(artifact) = legacy_classifier_artifact(lib) else {
            continue;
        };
        let jar = lib_dir.join(&artifact.path);
        if !jar.exists() {
            errors.push(format!("native JAR 缺失: {}", artifact.path));
            continue;
        }
        if let Err(e) = extract_jar(&jar, natives_dir, lib.extract.as_ref()) {
            errors.push(format!("{}: {}", lib.name, e));
        }
    }

    let has_content = std::fs::read_dir(natives_dir)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false);
    if !has_content {
        errors.push("natives 解压结果为空".to_string());
    }

    if errors.is_empty() {
        write_marker(&ready_marker, &fingerprint)?;
        Ok(())
    } else {
        // 第一行为大类短句（dialog 正文显示），细节进复制区
        Err(keyed_msg!(
            "errors.launch.nativesPrepareFailed",
            "natives 准备失败\n{}",
            errors.join("\n")
        ))
    }
}

/// 添加 native 系统属性：仅旧式版本设置 `-Djava.library.path`（natives 目录由 ensure_old_style_natives 保证就绪）。
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

/// 解析版本目录里的客户端 jar（classpath 与启动前完整性检查共用）；优先级：`{dir_name}.jar` → 目录内唯一 .jar → 体积最大者（`read_dir` 顺序不保证，故不取"第一个"）。
pub fn resolve_client_jar(version_dir: &Path, version_dir_name: &str) -> Option<PathBuf> {
    let named = version_dir.join(format!("{version_dir_name}.{EXT_JAR}"));
    if named.is_file() {
        return Some(named);
    }
    let mut jars: Vec<PathBuf> = std::fs::read_dir(version_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == EXT_JAR).unwrap_or(false))
        .collect();
    if jars.len() <= 1 {
        return jars.pop();
    }
    jars.sort_by(|a, b| {
        let sa = a.metadata().map(|m| m.len()).unwrap_or(0);
        let sb = b.metadata().map(|m| m.len()).unwrap_or(0);
        sb.cmp(&sa).then_with(|| a.file_name().cmp(&b.file_name()))
    });
    jars.into_iter().next()
}

pub fn find_file_in_dir(dir: &Path, ext: &str) -> Option<PathBuf> {
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

/// 定位版本 JSON：优先 `{dir_name}.json`，否则扫描目录内含 `"mainClass"` 者（PCL2 隔离实例目录混有游戏数据 json，字典序第一个会报 "missing field assetIndex"）。
pub fn find_version_json(dir: &Path, dir_name: &str) -> Option<PathBuf> {
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

struct JvmArgs<'a> {
    version_json: &'a VersionJson,
    natives_dir: &'a Path,
    lib_dir: &'a Path,
    classpath_str: &'a str,
    java_major_version: u32,
    config: &'a LaunchOptions,
    version_dir: &'a Path,
    has_custom_resolution: bool,
}

/// 组装 JVM 参数三段（顺序即命令行次序）：preamble（-cp 之前）/ system_props（-D 类）/ tuning（-cp 后、mainClass 前）。
fn collect_jvm_args(req: &JvmArgs<'_>) -> (Vec<String>, Vec<String>, Vec<String>) {
    let JvmArgs {
        version_json,
        natives_dir,
        lib_dir,
        classpath_str,
        java_major_version,
        config,
        version_dir,
        has_custom_resolution,
    } = *req;
    let mut jvm_preamble: Vec<String> = Vec::new(); // -cp 之前
    let mut system_props: Vec<String> = Vec::new(); // -D 类
    let mut jvm_tuning: Vec<String> = Vec::new(); // -cp 之后、mainClass 之前
                                                  // 与上方 classpath 构建保持一致：Windows ';' / Unix ':'
    let classpath_sep = if cfg!(windows) { ";" } else { ":" };

    if let Some(jvm_args) = version_json.jvm_args() {
        let raw = extract_args(jvm_args, has_custom_resolution);
        for arg in raw {
            // 跳过 -cp / -classpath / ${classpath}（否则替换后成为 -cp 之前的裸 classpath，被 java 当主类）
            if arg == ARG_CP || arg == ARG_CP_LONG || arg == PLACEHOLDER_CLASSPATH {
                continue;
            }
            let substituted = substitute_jvm(
                &arg,
                &JvmSubst {
                    natives_dir: &natives_dir.to_string_lossy(),
                    lib_dir: &lib_dir.to_string_lossy(),
                    classpath: classpath_str,
                    classpath_sep,
                    version_name: &version_json.id,
                    launcher_name: constants::LAUNCHER_BRAND,
                    launcher_version: constants::APP_VERSION,
                },
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

    // JDK 兼容前置标志（官方 jvm 可能已含，去重）；门限必须分开：`--sun-misc-unsafe-memory-access` 自 JDK 23（JEP 471）才有，JDK 22 传会以「Unrecognized option」退出；`--enable-native-access` 自 JDK 22 起可用。
    if java_major_version >= 23 {
        push_unique(&mut jvm_preamble, JVM_FLAG_SUN_MISC_UNSAFE.into());
    }
    if java_major_version >= 22 {
        push_unique(&mut jvm_preamble, JVM_FLAG_NATIVE_ACCESS.into());
    }

    add_native_props(&mut system_props, natives_dir, version_json);

    push_unique(
        &mut system_props,
        format!("{}{}", SYS_LAUNCHER_BRAND, constants::LAUNCHER_BRAND),
    );
    push_unique(
        &mut system_props,
        format!("{}{}", SYS_LAUNCHER_VERSION, constants::APP_VERSION),
    );

    system_props.push(SYS_STDOUT_ENCODING.into());
    system_props.push(SYS_STDERR_ENCODING.into());

    // 读取 Windows 系统代理（走代理时 sessionserver/textures 才能拉取皮肤）
    system_props.push(SYS_USE_SYSTEM_PROXIES.into());

    // 游戏窗口标题（1.13+ 读取 minecraft.window.title 系统属性；空 = 不改）
    if !config.window_title.is_empty() {
        system_props.push(format!("{}{}", SYS_WINDOW_TITLE, config.window_title));
    }

    // 游戏语言自动设为中文（-Duser.language 影响游戏/资源本地化）
    if config.game_language == LANG_AUTO_ZH_CN {
        system_props.push(SYS_USER_LANGUAGE_ZH.into());
        system_props.push(SYS_USER_COUNTRY_ZH.into());
    }

    let mem_mb = match config.memory {
        MemoryMode::Auto => memory::guess_auto_memory(version_dir),
        MemoryMode::Custom(mb) => mb,
    };
    jvm_tuning.push(format!("{}{}m", PREFIX_XMX, mem_mb));
    // 最小堆：仅 Custom 模式生效（Auto 的 -Xmx 动态推测）；clamp 到 -Xmx 以内（-Xms > -Xmx 会让 JVM 启动失败）。
    if let Some(min_mb) = config
        .min_memory
        .filter(|_| matches!(config.memory, MemoryMode::Custom(_)))
    {
        jvm_tuning.push(format!("{}{}m", PREFIX_XMS, min_mb.min(mem_mb)));
    }

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

/// 生成游戏参数：arguments.game 数组（含 feature 过滤）/ minecraftArguments 模板（空白拆分）/ 最低保底；再按需注入 --userProperties / --profileProperties（1.17+ camelCase，旧版连字符由 token 替换覆盖；1.19.3+ 已移除解析）。
fn build_game_args(
    version_json: &VersionJson,
    game_ctx: &TemplateContext<'_>,
    has_custom_resolution: bool,
    user_properties: &str,
) -> Vec<String> {
    let mut game_args = match version_json.game_args() {
        Some(game_arr) if !game_arr.is_empty() => {
            let raw = extract_args(game_arr, has_custom_resolution);
            raw.iter().map(|a| substitute(a, game_ctx)).collect()
        }
        _ => {
            if let Some(tpl) = &version_json.minecraft_arguments {
                // 旧格式：模板是空格分隔的参数串，必须**先按空白拆分、再逐 token 替换**（含空格的路径不可被切断）。
                tpl.split_whitespace()
                    .map(|tok| substitute(tok, game_ctx))
                    .filter(|tok| !tok.is_empty())
                    .collect()
            } else {
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

    if has_custom_resolution && !game_args.iter().any(|arg| arg == ARG_WIDTH) {
        game_args.extend([
            ARG_WIDTH.into(),
            game_ctx.resolution_width.to_string(),
            ARG_HEIGHT.into(),
            game_ctx.resolution_height.to_string(),
        ]);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_client_jar_prefers_named_then_largest() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("other.jar"), vec![0u8; 32]).unwrap();
        std::fs::write(d.join("big.jar"), vec![0u8; 4096]).unwrap();
        assert_eq!(
            resolve_client_jar(d, "v").unwrap().file_name().unwrap(),
            "big.jar",
            "无同名 jar 时取体积最大者（read_dir 顺序不保证，取第一个会让两处选中不同文件）"
        );
        std::fs::write(d.join("v.jar"), vec![0u8; 8]).unwrap();
        assert_eq!(
            resolve_client_jar(d, "v").unwrap().file_name().unwrap(),
            "v.jar",
            "有同名 jar 时优先它，即使更小"
        );

        let single = tempfile::tempdir().unwrap();
        std::fs::write(single.path().join("solo.jar"), vec![0u8; 8]).unwrap();
        assert_eq!(
            resolve_client_jar(single.path(), "v")
                .unwrap()
                .file_name()
                .unwrap(),
            "solo.jar",
            "唯一 jar 直接用"
        );

        let empty = tempfile::tempdir().unwrap();
        assert!(
            resolve_client_jar(empty.path(), "v").is_none(),
            "无 jar 应返回 None"
        );
    }

    fn natives_vj(libs_json: &str) -> VersionJson {
        serde_json::from_str(&format!(
            r#"{{
                "id": "1.12.2",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "1.12",
                "assetIndex": {{ "id": "1.12", "url": "x", "sha1": "y", "size": 1, "totalSize": 1 }},
                "downloads": {{ "client": {{ "url": "x", "sha1": "y", "size": 1 }} }},
                "arguments": {{ "game": [], "jvm": [] }},
                "libraries": [{libs_json}],
                "complianceLevel": 0,
                "minimumLauncherVersion": 21,
                "javaVersion": {{ "majorVersion": 8 }},
                "releaseTime": "2021-11-30T00:00:00Z",
                "time": "2021-11-30T00:00:00Z",
                "type": "release"
            }}"#
        ))
        .expect("构造版本 JSON 失败")
    }

    fn legacy_native_lib(name: &str, path: &str) -> String {
        format!(
            r#"{{ "name": "{name}", "natives": {{ "windows": "natives-windows" }},
                 "downloads": {{ "classifiers": {{ "natives-windows": {{ "path": "{path}", "url": "u", "sha1": "s", "size": 1 }} }} }} }}"#
        )
    }

    /// 指纹：native 集合变化必须改变；与顺序无关。
    #[test]
    fn natives_fingerprint_tracks_native_set() {
        let a = legacy_native_lib("org.lwjgl.lwjgl:lwjgl-platform:2.9.4", "n1.jar");
        let b = legacy_native_lib("org.lwjgl.lwjgl:jinput-platform:2.0.5", "n2.jar");
        let f1 = natives_fingerprint(&natives_vj(&a));
        let f2 = natives_fingerprint(&natives_vj(&format!("{a},{b}")));
        assert_ne!(f1, f2, "增加 native 库必须改变指纹（否则新版本会跳过解压）");
        assert_eq!(
            f2,
            natives_fingerprint(&natives_vj(&format!("{b},{a}"))),
            "指纹应与库顺序无关"
        );
        assert_eq!(f1, natives_fingerprint(&natives_vj(&a)), "同一集合指纹稳定");
    }

    /// 同 MC 版本换了 native 集合必须**重新解压**（标记按指纹而非存在与否）。
    #[test]
    fn natives_marker_rebuilds_when_native_set_changes() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("libraries");
        let natives_dir = dir.path().join("natives").join("1.12.2");
        std::fs::create_dir_all(&lib_dir).unwrap();

        let mk_jar = |rel: &str, file: &str| {
            let p = lib_dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let mut z = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            z.start_file(file, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut z, b"x").unwrap();
            z.finish().unwrap();
        };
        mk_jar("n1.jar", "a.dll");
        mk_jar("n2.jar", "b.dll");

        let lib1 = legacy_native_lib("org.lwjgl.lwjgl:lwjgl-platform:2.9.4", "n1.jar");
        let lib2 = legacy_native_lib("org.lwjgl.lwjgl:jinput-platform:2.0.5", "n2.jar");

        ensure_old_style_natives(&natives_vj(&lib1), &lib_dir, &natives_dir).unwrap();
        assert!(natives_dir.join("a.dll").is_file(), "应解出 a.dll");
        assert!(!natives_dir.join("b.dll").exists());

        std::fs::remove_file(natives_dir.join("a.dll")).unwrap();
        ensure_old_style_natives(&natives_vj(&lib1), &lib_dir, &natives_dir).unwrap();
        assert!(
            !natives_dir.join("a.dll").exists(),
            "指纹一致时应复用，不应重新解压"
        );

        ensure_old_style_natives(
            &natives_vj(&format!("{lib1},{lib2}")),
            &lib_dir,
            &natives_dir,
        )
        .unwrap();
        assert!(natives_dir.join("a.dll").is_file(), "重建后 a.dll 应回来");
        assert!(
            natives_dir.join("b.dll").is_file(),
            "回归：native 集合变化时必须重新解压，否则缺 b.dll"
        );
    }

    /// 旧格式 `minecraftArguments` 在含空格的 .minecraft 路径下，`--gameDir`/`--assetsDir` 必须保持为**单个** argv 元素。
    #[test]
    fn legacy_minecraft_arguments_keeps_spaced_paths_whole() {
        let vj: VersionJson = serde_json::from_str(
            r#"{
                "id": "1.7.10",
                "mainClass": "net.minecraft.launchwrapper.Launch",
                "assets": "legacy",
                "assetIndex": { "id": "legacy", "url": "x", "sha1": "y", "size": 1, "totalSize": 1 },
                "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
                "minecraftArguments": "--username ${auth_player_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --userProperties ${user_properties}",
                "complianceLevel": 0,
                "minimumLauncherVersion": 4,
                "javaVersion": { "majorVersion": 8 },
                "releaseTime": "2014-05-14T00:00:00Z",
                "time": "2014-05-14T00:00:00Z",
                "type": "release",
                "libraries": []
            }"#,
        )
        .expect("构造旧格式版本 JSON 失败");

        let game_dir = r"Zhang San/.minecraft/versions/1.7.10";
        let assets_dir = r"Zhang San/.minecraft/assets";
        let ctx = TemplateContext {
            auth_player_name: "Player",
            version_name: "1.7.10",
            game_directory: game_dir,
            assets_root: assets_dir,
            assets_index_name: "legacy",
            auth_uuid: "u",
            auth_access_token: "t",
            auth_xuid: "0",
            user_type: "msa",
            user_properties: "",
            version_type: "release",
            resolution_width: 854,
            resolution_height: 480,
            launcher_name: "Elytra",
            launcher_version: "1",
        };

        let args = build_game_args(&vj, &ctx, false, "");
        let idx = args.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(
            args[idx + 1],
            game_dir,
            "含空格的 --gameDir 路径必须保持单个 argv 元素: {args:?}"
        );
        let aidx = args.iter().position(|a| a == "--assetsDir").unwrap();
        assert_eq!(
            args[aidx + 1],
            assets_dir,
            "含空格的 --assetsDir 路径必须保持单个 argv 元素: {args:?}"
        );
        assert!(!args.iter().any(|a| a == "San"), "路径被截断: {args:?}");
        // `${user_properties}` 展开为空 → 必须丢弃空 token，而不是塞一个空 argv 元素
        assert!(
            !args.iter().any(|a| a.is_empty()),
            "不应产生空参数元素: {args:?}"
        );
    }
}
