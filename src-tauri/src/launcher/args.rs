use std::path::{Path, PathBuf};

use crate::auth::external::{AUTHLIB_JAR, is_ely_by};
use crate::constants;
use crate::game::resource::extract::{
    extract_jar, legacy_classifier_artifact, matches_rules, modern_native_matches,
    modern_natives_key, NativesKind, TARGET_OS,
};
use crate::game::version::model::VersionJson;
use crate::launcher::memory;
use crate::launcher::model::{AuthMode, LaunchOptions, MemoryMode, WindowSize};

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
const ARG_SERVER: &str = "--server";
const ARG_PORT: &str = "--port";

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

/// JVM 参数模板的占位符替换输入。
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

/// 启动命令构建的调用方输入（除 java 路径与版本 JSON 外的全部参数）。
pub struct LaunchRequest<'a> {
    pub version_dir_name: &'a str,
    pub mc_dir: &'a Path,
    pub config: &'a LaunchOptions,
    pub auth: &'a AuthMode,
    pub java_major_version: u32,
    pub user_properties: &'a str,
}

/// 构建完整的 Minecraft 启动命令。
///
/// 参数说明：
/// - `java_path`: JDK 的 java.exe 路径
/// - `version_json`: 已解析的版本 JSON
/// - `req`: 其余调用方输入（版本目录名 / .minecraft 目录 / 用户配置 / 认证模式 /
///   JDK 主版本号 / user properties）
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
    // 第三方登录（authlib-injector）：javaagent 注入 + client side
    // （jar 登录时下载到数据目录；缺失由启动前置检查拦截）
    push_external_auth_args(&mut args, auth);
    args.push(main_class);
    args.extend(game_args);
    // 服务器直连：快速进服（追加到游戏参数末尾，用户自定义 game_args 之前）
    push_server_args(&mut args, config);

    LaunchCommand {
        java_path: java_path.to_path_buf(),
        args,
    }
}

// ── 启动命令阶段函数 ──────────────────────────────────

/// 启动路径解析结果（natives / libraries / assets / 版本目录 / 窗口与平台差异）。
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

/// 解析启动所需的各种目录：natives / libraries / assets / 版本目录 / client.jar。
fn resolve_launch_paths(
    version_dir_name: &str,
    mc_dir: &Path,
    config: &LaunchOptions,
) -> LaunchPaths {
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
        WindowSize::Custom(w, h) => (w, h, true),
        // 占位值仅供模板替换兜底（带 feature 的参数组已被过滤）
        WindowSize::Default => {
            (DEFAULT_RESOLUTION_WIDTH, DEFAULT_RESOLUTION_HEIGHT, false)
        }
    };
    // JAR 文件名不一定等于目录名（目录可能因冲突被重命名）；
    // 与启动前完整性检查共用 resolve_client_jar，避免两处选中不同文件
    let client_jar = resolve_client_jar(&version_dir, version_dir_name)
        .unwrap_or_else(|| version_dir.join(format!("{}.{}", version_dir_name, EXT_JAR)));
    // classpath 分隔符按平台区分：Windows 用 ';'，Unix（Linux/macOS）用 ':'。
    // 硬编码 ';' 会让 Java 把整串 jar 当成单个路径，导致 ClassNotFoundException
    // （与 mod_loader/forge.rs 的处理保持一致）
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

/// 构建 classpath 字符串：libraries 产物 + PCL2 fabric 兜底，按平台分隔符拼接。
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

/// 按认证模式组装游戏参数（新格式数组 / 旧格式模板 / 保底参数）。
fn build_game_args_from_auth(
    version_json: &VersionJson,
    auth: &AuthMode,
    user_properties: &str,
    paths: &LaunchPaths,
) -> Vec<String> {
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

/// 第三方登录（authlib-injector）：javaagent 注入 + client side
/// （jar 登录时下载到数据目录；缺失由启动前置检查拦截）。
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

/// 服务器直连与用户自定义参数（追加到游戏参数末尾，用户自定义 game_args 之前）。
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

        // artifact JAR（通用 JAR）——按落地路径去重：下面 classifiers 分支可能指向
        // 同一个 native jar（历史 lwjgl3 早期数据），重复进 classpath 无意义
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

/// 去重追加：`list` 中尚不存在 `arg` 时压入（保序，避免重复的启动参数）。
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

/// natives 就绪标记的指纹：本次参与解压的旧式 native 库（名字 + 落地路径 + 提取规则）。
///
/// 顺序无关（排序后哈希）；任一库增删、换版本、改提取规则都会改变指纹 → 触发重建。
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

/// 清空目录内容（保留目录本身），用于 natives 集合变更时重建。
fn clear_dir_contents(dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(common_err_ctx!("读取 natives 目录失败"))?;
    for entry in entries {
        let path = entry
            .map_err(common_err_ctx!("读取 natives 目录项失败"))?
            .path();
        let res = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(e) = res {
            // 目录里可能残留游戏运行中占用的 .dll：明确指出是哪一项删不掉
            return Err(format!("清理旧 natives 失败\n{}\n{e}", path.display()));
        }
    }
    Ok(())
}

/// 原子写标记：先写临时文件再改名，避免半写内容被当成有效指纹。
fn write_marker(marker: &Path, fingerprint: &str) -> Result<(), String> {
    let tmp = marker.with_extension("tmp");
    std::fs::write(&tmp, fingerprint).map_err(common_err_ctx!("写入 natives 标记失败"))?;
    if marker.exists() {
        // Windows 的 rename 不覆盖已存在文件；先删旧标记（删后崩溃 → 无标记 → 下次重建，安全）
        std::fs::remove_file(marker).map_err(common_err_ctx!("清理旧 natives 标记失败"))?;
    }
    std::fs::rename(&tmp, marker).map_err(common_err_ctx!("提交 natives 标记失败"))?;
    Ok(())
}

/// 确保 natives 目录就绪：所有版本都创建 `.minecraft/natives/<mc>/` 根目录
/// （官方 arguments.jvm 的 SharedLibraryExtractPath 等参数指向它，LWJGL 需要目录存在）。
/// 旧式版本（lwjgl 2.x）额外把 natives classifier JAR 解压到该目录。
pub fn ensure_old_style_natives(
    version_json: &VersionJson,
    lib_dir: &Path,
    natives_dir: &Path,
) -> Result<(), String> {
    // 所有版本：确保 natives 根目录存在（LWJGL/Netty 运行时在其中创建子目录）
    std::fs::create_dir_all(natives_dir).map_err(common_err_ctx!("创建 natives 目录失败"))?;

    if !has_old_style_natives(version_json) {
        return Ok(());
    }

    // 标记的语义是「**本版本**需要的 native 集合已就绪」，而不是「这个目录解压过」：
    // natives 目录按 MC 版本共享（vanilla / Forge / OptiFine 同名一份），若换来的
    // 版本 native 集合不同，旧标记会让新版本直接跳过解压 → 缺原生库
    // （UnsatisfiedLibraryError，且报错完全指不到这里）。因此标记内容存指纹。
    let fingerprint = natives_fingerprint(version_json);
    let ready_marker = natives_dir.join(NATIVES_READY_MARKER);
    let marker_ok = std::fs::read_to_string(&ready_marker)
        .map(|c| c.trim() == fingerprint)
        .unwrap_or(false);
    if marker_ok {
        return Ok(());
    }

    // 指纹不一致（或旧版遗留的空标记）：清空目录再解压 —— 不清会留下别的版本集合
    // 的旧 .dll，Windows 的 DLL 搜索顺序可能加载到旧的那份。
    clear_dir_contents(natives_dir)?;

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
        if let Err(e) = extract_jar(&jar, natives_dir, lib.extract.as_ref()) {
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
        write_marker(&ready_marker, &fingerprint)?;
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

/// 解析版本目录里的客户端 jar —— **classpath 与启动前完整性检查共用同一条规则**。
///
/// 优先级：`{dir_name}.jar` → 目录内唯一的 .jar（导入的实例文件名常与目录名不同）
/// → 体积最大的 .jar（client jar 通常最大，且结果确定）。
/// 不取"第一个"：`read_dir` 顺序不保证，两处各取一次会选中不同文件 ——
/// 表现为"完整性检查通过，但启动用的不是客户端 jar"。
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

/// 在目录中扫描第一个指定扩展名的文件（文件名可能与目录名不同，如目录被重命名）。
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

/// 定位版本 JSON：优先 `{dir_name}.json`（标准布局），否则扫描目录内
/// 含 `"mainClass"` 键的 json——PCL2 版本隔离实例目录混有游戏数据 json
/// （TrashSlotSaveState/patchouli_data/usercache 等），字典序第一个
/// 可能不是版本 json，直接解析会报 "missing field assetIndex"。
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

/// JVM 参数三段组装输入。
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

/// 组装 JVM 参数三段：
/// - preamble：-cp 之前（版本 JSON 参数 + JDK 22+ 兼容标志）
/// - system_props：-D 类（natives/品牌/编码/代理/窗口标题/语言）
/// - tuning：-cp 之后、mainClass 之前（内存/GC/用户自定义）
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
                                                  // 与上方 classpath 构建保持一致：按平台选择分隔符（Windows ';' / Unix ':'）
    let classpath_sep = if cfg!(windows) { ";" } else { ":" };

    // 从版本 JSON 的 arguments.jvm 提取
    if let Some(jvm_args) = version_json.jvm_args() {
        let raw = extract_args(jvm_args, has_custom_resolution);
        for arg in raw {
            // 跳过 -cp / -classpath / ${classpath}（classpath 由我们自行构建，
            // 若 ${classpath} 被替换后不跳过，会变成 -cp 之前的裸 classpath 被 java 当主类）
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

    // JDK 兼容前置标志（官方 arguments.jvm 可能已包含，去重）。
    // 门限必须分开：`--sun-misc-unsafe-memory-access` 是 JDK 23（JEP 471）才引入的选项，
    // 在 JDK 22 上传它会让 JVM 以「Unrecognized option」直接退出（游戏完全起不来）；
    // `--enable-native-access` 自 JDK 22 起正式可用。
    if java_major_version >= 23 {
        push_unique(&mut jvm_preamble, JVM_FLAG_SUN_MISC_UNSAFE.into());
    }
    if java_major_version >= 22 {
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
    // 最小堆：仅 Custom 模式生效（Auto 下 -Xmx 动态推测，固定 -Xms 可能超上限）；
    // 值 clamp 到 -Xmx 以内（-Xms > -Xmx 会让 JVM 直接启动失败）。
    if let Some(min_mb) = config
        .min_memory
        .filter(|_| matches!(config.memory, MemoryMode::Custom(_)))
    {
        jvm_tuning.push(format!("{}{}m", PREFIX_XMS, min_mb.min(mem_mb)));
    }

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
    let mut game_args = match version_json.game_args() {
        Some(game_arr) if !game_arr.is_empty() => {
            // 新格式：arguments.game 数组
            let raw = extract_args(game_arr, has_custom_resolution);
            raw.iter().map(|a| substitute(a, game_ctx)).collect()
        }
        _ => {
            // 旧格式：minecraftArguments 模板字符串
            if let Some(tpl) = &version_json.minecraft_arguments {
                // 旧格式：模板是空格分隔的参数串，必须**先按空白拆分、再逐 token 替换占位符**
                // （与官方启动器一致）。若先替换再拆分，含空格的 .minecraft 路径会被
                // split_whitespace 二次切断（`--gameDir C:\Users\Zhang San\...` →
                // `--gameDir C:\Users\Zhang` + 两个游离 token），导致版本隔离失效。
                // 替换后为空的 token 需丢弃（如 `${user_properties}` 展开为空），
                // 否则会多出一个空 argv 元素。
                tpl.split_whitespace()
                    .map(|tok| substitute(tok, game_ctx))
                    .filter(|tok| !tok.is_empty())
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
        assert!(resolve_client_jar(empty.path(), "v").is_none(), "无 jar 应返回 None");
    }

    /// 版本 JSON 构造器（旧式 native 库列表可变）。
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

    /// 旧式 native 库（natives 字段 + classifiers）。
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

    /// 标记语义回归：同 MC 版本换了 native 集合必须**重新解压**，而不是看到旧标记就跳过。
    ///
    /// 此前标记是空文件、只看"存在与否"，而 natives 目录按 MC 版本共享
    /// （vanilla / Forge / OptiFine 同名一份）→ 新版本直接跳过解压 → 缺原生库。
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

        // 第一次：只解出 a.dll，并写指纹标记
        ensure_old_style_natives(&natives_vj(&lib1), &lib_dir, &natives_dir).unwrap();
        assert!(natives_dir.join("a.dll").is_file(), "应解出 a.dll");
        assert!(!natives_dir.join("b.dll").exists());

        // 同指纹 → 走复用快路径：删掉 a.dll 也不会被补回来（证明没重复解压）
        std::fs::remove_file(natives_dir.join("a.dll")).unwrap();
        ensure_old_style_natives(&natives_vj(&lib1), &lib_dir, &natives_dir).unwrap();
        assert!(
            !natives_dir.join("a.dll").exists(),
            "指纹一致时应复用，不应重新解压"
        );

        // 指纹变化 → 清空重建：a.dll 与 b.dll 都在
        ensure_old_style_natives(&natives_vj(&format!("{lib1},{lib2}")), &lib_dir, &natives_dir)
            .unwrap();
        assert!(natives_dir.join("a.dll").is_file(), "重建后 a.dll 应回来");
        assert!(
            natives_dir.join("b.dll").is_file(),
            "回归：native 集合变化时必须重新解压，否则缺 b.dll"
        );
    }

    /// 回归：旧格式 `minecraftArguments`（1.7.10/1.8.9 等无 `arguments.game`）在含空格
    /// 的 .minecraft 路径下，`--gameDir`/`--assetsDir` 必须保持为**单个** argv 元素。
    /// 此前「先替换占位符、再 split_whitespace」会把含空格路径切成多个 token。
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

        let game_dir = r"C:\Users\Zhang San\.minecraft\versions\1.7.10";
        let assets_dir = r"C:\Users\Zhang San\.minecraft\assets";
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
            args[idx + 1], game_dir,
            "含空格的 --gameDir 路径必须保持单个 argv 元素: {args:?}"
        );
        let aidx = args.iter().position(|a| a == "--assetsDir").unwrap();
        assert_eq!(
            args[aidx + 1], assets_dir,
            "含空格的 --assetsDir 路径必须保持单个 argv 元素: {args:?}"
        );
        // 路径没有被切成 `...Zhang` + 游离 `San` token
        assert!(!args.iter().any(|a| a == "San"), "路径被截断: {args:?}");
        // `${user_properties}` 展开为空 → 必须丢弃空 token，而不是塞一个空 argv 元素
        assert!(
            !args.iter().any(|a| a.is_empty()),
            "不应产生空参数元素: {args:?}"
        );
    }
}
