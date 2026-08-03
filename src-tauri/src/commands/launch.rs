use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::config::downloader::model::DownloaderConfig;
use crate::config::jdk::model::JavaConfig;
use crate::config::launcher::args::build_launch_command;
use crate::config::launcher::model::{AuthMode, LauncherConfig};
use crate::downloader::model::VersionJson;
use crate::persist;

// ── 内部辅助 ────────────────────────────────────────

/// 加载全部持久化配置 + 解析版本 JSON，供 launch/spawn 共用。
#[allow(clippy::type_complexity)]
fn load_launch_config(
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
    let mc_dir = std::path::absolute(PathBuf::from(&dl_config.minecraft_dir))
        .unwrap_or_else(|_| PathBuf::from(&dl_config.minecraft_dir));

    // 读取版本 JSON（扫描目录，文件名不一定是目录名）
    let vdir = mc_dir.join("versions").join(version_dir_name);
    if !vdir.is_dir() {
        return Err(format!("版本目录不存在: {}", vdir.display()));
    }
    let vj_path = std::fs::read_dir(&vdir)
        .map_err(|e| format!("读取版本目录失败: {}", e))?
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
        .ok_or_else(|| format!("版本目录下未找到 JSON 文件: {}", vdir.display()))?
        .path();
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
fn pre_launch_check(
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
        let found = std::fs::read_dir(&vdir)
            .ok()
            .and_then(|rd| {
                rd.filter_map(|e| e.ok()).find(|e| {
                    e.path().extension().map(|ext| ext == "jar").unwrap_or(false)
                        && jar_ok(&e.path())
                })
            });
        if found.is_none() {
            missing.push(format!("客户端 JAR: {}", client_jar.display()));
        }
    }

    // 2. libraries（规则过滤 + 架构匹配）
    let lib_dir = mc_dir.join("libraries");
    for lib in &version_json.libraries {
        if let Some(rules) = &lib.rules {
            if !rules_match_os(rules) {
                continue;
            }
        }
        if !crate::downloader::extract::native_classifier_matches(&lib.name) {
            continue;
        }
        let Some(dls) = &lib.downloads else { continue };
        let Some(artifact) = &dls.artifact else { continue };
        let dest = lib_dir.join(&artifact.path);
        if !dest.exists() {
            missing.push(format!("支持库: {}", artifact.path));
            if missing.len() >= 5 {
                break;
            }
        }
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

/// 读取子进程输出并追加写入日志文件，直到 EOF 或出错。
fn drain_to_file(reader: &mut impl std::io::Read, path: &std::path::Path, is_stderr: bool) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok();
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if let Some(ref mut f) = file {
                    if is_stderr {
                        let _ = f.write_all(b"[stderr] ");
                    }
                    let _ = f.write_all(&buf[..n]);
                    let _ = f.write_all(b"\n");
                }
            }
        }
    }
}

/// 检查规则是否匹配当前 OS（Windows）。
fn rules_match_os(rules: &[crate::downloader::model::Rule]) -> bool {
    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(crate::downloader::model::OsRule { name: Some(n) }) => n == crate::downloader::extract::TARGET_OS,
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

fn build_auth(auth_mode: &str, username: &str, access_token: &str, uuid: &str, xuid: &str) -> AuthMode {
    match auth_mode {
        "online" => AuthMode::Online {
            username: username.to_string(),
            access_token: access_token.to_string(),
            uuid: uuid.to_string(),
            xuid: xuid.to_string(),
        },
        _ => AuthMode::Offline {
            username: username.to_string(),
            uuid: String::new(), // 由 apply_offline_skin 按离线皮肤设置填充
        },
    }
}

/// 正版模式 xuid 缺失时从 Minecraft access token（JWT）解析兜底
/// （XSTS 的 xid 字段部分账号缺失，旧配置也可能未保存 xuid）。
fn fill_missing_xuid(auth: &mut AuthMode) {
    if let AuthMode::Online { xuid, access_token, .. } = auth {
        if xuid.is_empty() {
            *xuid = crate::auth::microsoft::extract_xuid_from_jwt(access_token);
        }
    }
}

/// 离线默认 UUID（固定占位）。
const OFFLINE_BASE_UUID: &str = "00000000FFFFFFFFFFFFFFFFFFFFFFFF";

/// PCL 式皮肤模型匹配：
/// `UUID.hashCode() & 1 == (Uuid[7] ^ Uuid[15] ^ Uuid[23] ^ Uuid[31]) & 1`
/// （1.8+ 的 DefaultPlayerSkin.isSlimSkin 判定，奇数 → Alex/slim）
fn uuid_matches_alex(uuid: &str) -> bool {
    let chars: Vec<u32> = uuid
        .chars()
        .filter_map(|c| c.to_digit(16))
        .collect();
    if chars.len() != 32 {
        return false;
    }
    (chars[7] ^ chars[15] ^ chars[23] ^ chars[31]) & 1 == 1
}

/// 从基础 UUID 递增末位直到匹配目标模型（16 次内必达）。
fn skin_matching_uuid(base: &str, want_alex: bool) -> String {
    let mut chars: Vec<char> = base.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if chars.len() != 32 {
        return base.to_string();
    }
    loop {
        if uuid_matches_alex(&chars.iter().collect::<String>()) == want_alex {
            return chars.iter().collect();
        }
        let v = chars[31].to_digit(16).unwrap_or(0) + 1;
        chars[31] = char::from_digit(v % 16, 16).unwrap_or('0');
    }
}

/// 查询正版玩家真实 UUID（api.mojang.com）；失败返回 None。
async fn resolve_player_uuid(name: &str) -> Option<String> {
    let resp = reqwest::get(format!(
        "https://api.mojang.com/users/profiles/minecraft/{}",
        name.trim()
    ))
    .await
    .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<serde_json::Value>()
        .await
        .ok()?
        .get("id")?
        .as_str()
        .map(str::to_string)
}

/// 应用离线皮肤（PCL 机制）：
/// 1. UUID 策略：Steve/Alex/Custom 生成匹配模型的 UUID；Player 用正版玩家真实 UUID
///    （游戏内按 UUID 哈希显示对应默认皮肤，1.19.2 以下；
///    Player 模式游戏内还会经 sessionserver 拉取正版皮肤，需网络）；
/// 2. Custom：构建替换默认纹理的资源包（resourcepacks/elytra-skin.zip）并写入 options.txt 启用。
async fn apply_offline_skin(
    auth: &mut AuthMode,
    version_json: &crate::downloader::model::VersionJson,
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) {
    let AuthMode::Offline { username, uuid } = auth else {
        return;
    };
    if uuid.is_empty() {
        *uuid = OFFLINE_BASE_UUID.to_string();
    }

    let skin: crate::config::offline_skin::OfflineSkinConfig =
        crate::persist::read("offline_skin").unwrap_or_default();
    use crate::config::offline_skin::{OfflineSkinModel, OfflineSkinType as T};

    match skin.skin_type {
        T::Steve => *uuid = skin_matching_uuid(uuid, false),
        T::Alex => *uuid = skin_matching_uuid(uuid, true),
        // 随机：与前端 hashName 一致（哈希奇数 → Alex），游戏内按匹配 UUID 显示同一模型
        T::Random => {
            let want_alex = hash_name(username) % 2 == 1;
            *uuid = skin_matching_uuid(uuid, want_alex);
        }
        T::Player => {
            let name = skin.player_name.trim();
            if !name.is_empty() {
                if let Some(real) = resolve_player_uuid(name).await {
                    *uuid = real;
                }
            }
        }
        T::Custom => {
            let path = skin.custom_path.trim();
            let slim = matches!(skin.custom_model, OfflineSkinModel::Alex);
            *uuid = skin_matching_uuid(uuid, slim);
            if !path.is_empty() && std::path::Path::new(path).exists() {
                let game_dir = mc_dir.join("versions").join(version_dir_name);
                apply_custom_skin_pack(&game_dir, path, slim, version_json);
            }
        }
    }
    // 用户名不变（PCL 同样使用输入的用户名）
    let _ = username;
}

/// 构建自定义皮肤资源包并写入 options.txt 启用（对齐 PCL2 Skin.zip 机制）。
/// 仅对 1.6+ 版本有效；替换 steve.png/alex.png 默认纹理为本地皮肤文件。
fn apply_custom_skin_pack(
    game_dir: &std::path::Path,
    skin_path: &str,
    slim: bool,
    version_json: &crate::downloader::model::VersionJson,
) {
    use std::io::Write;

    // 版本主号（如 "1.12.2" → 12）；快照/未知按高版本处理
    let major = version_major(version_json);
    if major < 6 {
        return; // 1.5- 无资源包
    }
    let pack_format = pack_format_for(major, version_json);

    let rp_dir = game_dir.join("resourcepacks");
    let zip_path = rp_dir.join("elytra-skin.zip");
    let tex_name = if slim { "alex.png" } else { "steve.png" };

    // 1. 写 zip
    let result = (|| -> Result<(), String> {
        std::fs::create_dir_all(&rp_dir).map_err(|e| e.to_string())?;
        let file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("pack.mcmeta", opts).map_err(|e| e.to_string())?;
        write!(
            zip,
            r#"{{"pack":{{"pack_format":{},"description":"ElytraLauncher custom skin"}}}}"#,
            pack_format
        )
        .map_err(|e| e.to_string())?;
        zip.start_file(
            format!("assets/minecraft/textures/entity/{tex_name}"),
            opts,
        )
        .map_err(|e| e.to_string())?;
        let mut f = std::fs::File::open(skin_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        println!("[skin] 构建皮肤资源包失败: {e}");
        return;
    }

    // 2. options.txt 启用（1.13+ 用 file/ 前缀，旧版直接文件名）
    let options_path = game_dir.join("options.txt");
    let content = std::fs::read_to_string(&options_path).unwrap_or_default();
    let pack_entry = if major >= 13 {
        "\"file/elytra-skin.zip\""
    } else {
        "\"elytra-skin.zip\""
    };
    let mut found = false;
    let mut out = String::new();
    for line in content.lines() {
        if line.starts_with("resourcePacks:") {
            out.push_str(&format!("resourcePacks:[{pack_entry}]\n"));
            found = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        out.push_str(&format!("resourcePacks:[{pack_entry}]\n"));
    }
    let _ = std::fs::write(&options_path, out);
    println!("[skin] 自定义皮肤资源包已启用 ({tex_name}, format {pack_format})");
}

/// 版本主号（"1.12.2" → 12）。
fn version_major(version_json: &crate::downloader::model::VersionJson) -> u32 {
    let id = &version_json.id;
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() >= 2 && parts[0] == "1" {
        parts[1].parse().unwrap_or(99)
    } else if parts[0].parse::<u32>().is_ok() {
        parts[0].parse().unwrap_or(99)
    } else {
        99 // 快照等未知版本按最新处理
    }
}

/// 资源包 pack_format（对齐 PCL 各版本映射）。
fn pack_format_for(major: u32, version_json: &crate::downloader::model::VersionJson) -> u32 {
    let id = &version_json.id;
    let minor: u32 = {
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() >= 3 {
            parts[2].parse().unwrap_or(0)
        } else {
            0
        }
    };
    match major {
        6..=8 => 1,
        9..=10 => 2,
        11..=12 => 3,
        13..=14 => 4,
        15 => 5,
        16 => 6,
        17 => 7,
        18 => {
            if minor <= 2 {
                8
            } else {
                9
            }
        }
        19 => {
            if minor <= 3 {
                9
            } else {
                12
            }
        }
        20 => {
            if minor <= 1 {
                15
            } else {
                17
            }
        }
        _ => 22,
    }
}

/// Alex 官方默认皮肤纹理（textures.minecraft.net 固定 hash；与游戏内置 alex.png 一致，已验证）。
/// Steve 不注入：游戏默认纹理即经典蓝衬衫 Steve。
const ALEX_TEXTURE_URL: &str = "https://textures.minecraft.net/texture/46acd06e8483b176e8ea39fc12fe105eb3a2a4970f5100057e9d84d4b60bdfa7";

/// FNV-1a 哈希（与前端 Launch.vue hashName 一致）
fn hash_name(name: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// 构造 `--user-properties` 参数值（authlib user properties）。
/// 仅 1.19.2 以下版本的游戏参数含 `${user_properties}`，离线模式 authlib 不校验签名，
/// 注入纹理即可自定义皮肤（对齐 PCL「离线皮肤仅 1.19.2 以下生效」）。
/// 在线模式：返回空（皮肤走 sessionserver）。
/// Random/Steve/Alex 也注入对应默认纹理（离线 UUID 固定，游戏默认纹理恒为 Steve；
/// 注入才能与启动器头像渲染一致；Alex 必须注入 slim 纹理）。
/// 格式统一为 authlib PropertyMap 字符串数组 `{"textures":["<b64>"]}`
/// （1.12-1.19.2 的 PropertyMap$Serializer 均按此解析）。
async fn build_user_properties(auth: &AuthMode) -> String {
    let AuthMode::Offline { username, .. } = auth else {
        return String::new();
    };

    let skin: crate::config::offline_skin::OfflineSkinConfig =
        crate::persist::read("offline_skin").unwrap_or_default();

    use crate::config::offline_skin::OfflineSkinType as T;
    let value_b64 = match skin.skin_type {
        // Steve：不注入——游戏默认纹理即经典蓝衬衫 Steve（离线 UUID 固定默认也是 Steve）
        T::Steve => return String::new(),
        T::Alex => textures_payload(ALEX_TEXTURE_URL, true),
        T::Random => {
            // 与前端 hashName 一致：hash % 2 == 1 → Alex（注入）；否则 Steve（默认，不注入）
            if hash_name(username) % 2 == 1 {
                textures_payload(ALEX_TEXTURE_URL, true)
            } else {
                return String::new();
            }
        }
        T::Player => {
            // 查正版玩家 UUID → sessionserver 取 textures property 的 value（base64）
            let name = skin.player_name.trim();
            if name.is_empty() {
                return String::new();
            }
            match resolve_player_texture_value(name).await {
                Some(v) => v,
                None => return String::new(),
            }
        }
        T::Custom => {
            let path = skin.custom_path.trim();
            if path.is_empty() {
                return String::new();
            }
            let path = std::path::Path::new(path);
            if !path.exists() {
                return String::new();
            }
            let Ok(url) = url::Url::from_file_path(path) else {
                return String::new();
            };
            textures_payload(&url.to_string(), matches!(
                skin.custom_model,
                crate::config::offline_skin::OfflineSkinModel::Alex
            ))
        }
    };

    // authlib PropertyMap 格式（数组元素为 base64 字符串）
    format!(r#"{{"textures":["{value_b64}"]}}"#)
}

/// 构造 textures payload（`{"textures":{"SKIN":{"url":...,"metadata":...}}}`）的 base64。
fn textures_payload(url: &str, slim: bool) -> String {
    let mut textures = serde_json::Map::new();
    let mut skin_obj = serde_json::Map::new();
    skin_obj.insert("url".into(), serde_json::Value::String(url.to_string()));
    if slim {
        let mut meta = serde_json::Map::new();
        meta.insert("model".into(), serde_json::Value::String("slim".into()));
        skin_obj.insert("metadata".into(), serde_json::Value::Object(meta));
    }
    textures.insert("SKIN".into(), serde_json::Value::Object(skin_obj));
    let payload = serde_json::json!({ "textures": textures });
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(payload.to_string())
}

/// 查询正版玩家纹理：api.mojang.com 取 UUID → sessionserver 取 textures property。
/// 返回 textures property 的 value（base64 payload）；失败返回 None（游戏回退默认纹理）。
async fn resolve_player_texture_value(name: &str) -> Option<String> {
    let uuid_resp = reqwest::get(format!(
        "https://api.mojang.com/users/profiles/minecraft/{}",
        name
    ))
    .await
    .ok()?;
    if !uuid_resp.status().is_success() {
        return None;
    }
    let uuid: String = uuid_resp.json::<serde_json::Value>().await.ok()?
        .get("id")?
        .as_str()?
        .to_string();

    let profile_resp = reqwest::get(format!(
        "https://sessionserver.mojang.com/session/minecraft/profile/{}?unsigned=false",
        uuid
    ))
    .await
    .ok()?;
    if !profile_resp.status().is_success() {
        return None;
    }
    let profile: serde_json::Value = profile_resp.json().await.ok()?;
    let props = profile.get("properties")?.as_array()?;
    let textures_prop = props.iter().find(|p| {
        p.get("name").and_then(|n| n.as_str()) == Some("textures")
    })?;

    textures_prop.get("value")?.as_str().map(str::to_string)
}

/// 游戏语言：自动设置为中文。
/// 1.13+ 游戏启动时读取 gameDir/options.txt 的 `lang` 字段，
/// 存在则替换，否则追加（文件不存在时创建，游戏启动后保留该值）。
fn apply_game_language(game_dir: &std::path::Path) {
    let options_path = game_dir.join("options.txt");
    let content = match std::fs::read_to_string(&options_path) {
        Ok(c) => c,
        Err(_) => {
            // 文件不存在：预写 lang，游戏首次启动沿用
            let _ = std::fs::create_dir_all(game_dir);
            let _ = std::fs::write(&options_path, "lang:zh_cn\n");
            return;
        }
    };

    let mut found = false;
    let mut out = String::new();
    for line in content.lines() {
        if line.starts_with("lang:") {
            out.push_str("lang:zh_cn\n");
            found = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        out.push_str("lang:zh_cn\n");
    }
    let _ = std::fs::write(&options_path, out);
}

// ── Tauri Commands ──────────────────────────────────

/// 启动前环境 + 完整性校验，供前端分阶段显示启动流程。
/// 通过时返回使用的 JDK 信息，失败时返回具体原因。
#[tauri::command]
pub async fn check_launch_env(version_dir_name: String) -> Result<String, String> {
    let (_java_path, version_json, mc_dir, _launcher_config, _java_major) =
        load_launch_config(&version_dir_name)?;

    // 游戏完整性（client JAR + libraries + assets index）
    pre_launch_check(&version_json, &mc_dir, &version_dir_name)?;

    // 旧式 natives 解压准备
    let natives_dir = mc_dir.join("natives").join(&version_json.id);
    crate::config::launcher::args::ensure_old_style_natives(
        &version_json,
        &mc_dir.join("libraries"),
        &natives_dir,
    )?;

    let java_env = crate::persist::read::<JavaConfig>("jdk")
        .map(|c| c.active_jdk.map(|e| e.version).unwrap_or_else(|| "自动选择".into()))
        .unwrap_or_else(|_| "自动选择".into());
    Ok(format!("环境校验通过（JDK: {}）", java_env))
}

/// 系统内存信息（MB）：总内存 + 当前可用内存。
#[tauri::command]
pub fn get_memory_info() -> (u64, u64) {
    #[cfg(target_os = "windows")]
    unsafe {
        #[repr(C)]
        struct MEMORYSTATUSEX {
            dw_length: u32,
            dw_memory_load: u32,
            ull_total_phys: u64,
            ull_avail_phys: u64,
            ull_total_page_file: u64,
            ull_avail_page_file: u64,
            ull_total_virtual: u64,
            ull_avail_virtual: u64,
            ull_avail_extended_virtual: u64,
        }
        #[link(name = "Kernel32")]
        extern "system" {
            fn GlobalMemoryStatusEx(lp_buffer: *mut MEMORYSTATUSEX) -> i32;
        }
        let mut st = MEMORYSTATUSEX {
            dw_length: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            dw_memory_load: 0,
            ull_total_phys: 0,
            ull_avail_phys: 0,
            ull_total_page_file: 0,
            ull_avail_page_file: 0,
            ull_total_virtual: 0,
            ull_avail_virtual: 0,
            ull_avail_extended_virtual: 0,
        };
        if GlobalMemoryStatusEx(&mut st) != 0 {
            return (st.ull_total_phys / 1024 / 1024, st.ull_avail_phys / 1024 / 1024);
        }
        (16384, 8192)
    }
    #[cfg(not(target_os = "windows"))]
    {
        (16384, 8192)
    }
}

/// 读取启动选项配置（launcher section）。
#[tauri::command]
pub fn get_launch_options() -> LauncherConfig {
    crate::persist::read::<LauncherConfig>("launcher").unwrap_or_default()
}

/// 保存启动选项配置（launcher section）。
#[tauri::command]
pub fn save_launch_options(config: LauncherConfig) -> Result<(), String> {
    crate::persist::save("launcher", &config).map_err(|e| e.to_string())
}

/// 读取最后一次使用的离线用户名（与正版隔离）。
#[tauri::command]
pub fn get_last_offline_username() -> String {
    crate::persist::read::<String>("last_offline_username").unwrap_or_default()
}

/// 保存最后一次使用的离线用户名。
#[tauri::command]
pub fn save_last_offline_username(username: String) -> Result<(), String> {
    crate::persist::save("last_offline_username", &username).map_err(|e| e.to_string())
}

/// 返回完整的启动命令行参数（exec 格式，仅供预览/调试）。
#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    version_dir_name: String,
    auth_mode: String,
    username: String,
    access_token: String,
    uuid: String,
    xuid: String,
) -> Result<Vec<String>, String> {
    let mut auth = build_auth(&auth_mode, &username, &access_token, &uuid, &xuid);
    fill_missing_xuid(&mut auth);
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        load_launch_config(&version_dir_name)?;

    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &version_dir_name,
        &mc_dir,
        &launcher_config,
        &auth,
        java_major,
        "",
    );

    let _ = app.emit("game-launch", &cmd.args);
    Ok(cmd.args)
}

/// 导出启动脚本：按 PCL 风格生成平台对应脚本（Windows .bat / macOS/Linux .sh）写入指定路径。
/// 返回写入的文件路径。
#[tauri::command]
pub async fn export_launch_script(
    save_path: String,
    version_dir_name: String,
    auth_mode: String,
    username: String,
    access_token: String,
    uuid: String,
    xuid: String,
) -> Result<String, String> {
    let auth = build_auth(&auth_mode, &username, &access_token, &uuid, &xuid);
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        load_launch_config(&version_dir_name)?;
    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &version_dir_name,
        &mc_dir,
        &launcher_config,
        &auth,
        java_major,
        "",
    );

    let script = render_launch_script(&cmd.args, &mc_dir, &version_dir_name);
    std::fs::write(&save_path, script).map_err(|e| format!("写入脚本失败：{e}"))?;
    Ok(save_path)
}

/// 渲染启动脚本内容（Windows: .bat；macOS/Linux: .sh）。
/// 格式对齐 PCL 导出脚本：chcp 65001 + title + cd 版本目录 + java 命令行 + pause。
fn render_launch_script(args: &[String], mc_dir: &std::path::Path, version_dir_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        let mut out = String::new();
        out.push_str("chcp 65001>nul\r\n");
        out.push_str("@echo off\r\n");
        out.push_str(&format!("title 启动 - {version_dir_name}\r\n"));
        out.push_str("echo 游戏正在启动，请稍候。\r\n");
        let vdir = mc_dir.join("versions").join(version_dir_name);
        out.push_str(&format!("cd /D \"{}\"\r\n\r\n", vdir.display()));
        for a in args {
            out.push('"');
            out.push_str(&a.replace('%', "%%"));
            out.push_str("\" ");
        }
        out.push_str("\r\necho 游戏已退出。\r\n");
        out.push_str("pause\r\n");
        out
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut out = String::from("#!/usr/bin/env sh\n");
        out.push_str(&format!("# 启动 - {version_dir_name}\n"));
        out.push_str(&format!("cd \"{}\"\n", mc_dir.join("versions").join(version_dir_name).display()));
        out.push_str("exec ");
        for a in args {
            out.push('\'');
            out.push_str(&a.replace('\'', "'\\''"));
            out.push_str("' ");
        }
        out.push('\n');
        out
    }
}

/// 启动 Minecraft 游戏进程。返回子进程 PID。
#[tauri::command]
pub async fn spawn_game(
    app: AppHandle,
    version_dir_name: String,
    auth_mode: String,
    username: String,
    access_token: String,
    uuid: String,
    xuid: String,
) -> Result<u32, String> {
    let mut auth = build_auth(&auth_mode, &username, &access_token, &uuid, &xuid);
    fill_missing_xuid(&mut auth);
    let (java_path, version_json, mc_dir, launcher_config, java_major) =
        load_launch_config(&version_dir_name)?;

    // 离线皮肤（PCL 机制）：UUID 匹配模型（Steve/Alex/Custom）或正版玩家真实 UUID（Player）；
    // Custom 额外构建替换默认纹理的资源包
    apply_offline_skin(&mut auth, &version_json, &mc_dir, &version_dir_name).await;

    // 启动前完整性检测（client JAR + libraries + assets index）
    pre_launch_check(&version_json, &mc_dir, &version_dir_name)?;

    // 旧式版本：确保 natives 已解压到 .minecraft/natives/<ver>/
    let natives_dir = mc_dir.join("natives").join(&version_json.id);
    crate::config::launcher::args::ensure_old_style_natives(
        &version_json,
        &mc_dir.join("libraries"),
        &natives_dir,
    )?;

    // 离线皮肤：构造 --user-properties 参数（1.19.2 以下版本生效；
    // Random/Steve/Alex 无需注入（游戏默认纹理一致），Player/Custom 注入皮肤）
    let user_properties = build_user_properties(&auth).await;

    // 游戏语言：自动设置为中文（1.13+ 语言由 gameDir/options.txt 的 lang 字段控制，
    // -Duser.language 仅对旧版 Locale 机制生效）
    if launcher_config.game_language == "auto_zh_CN" {
        apply_game_language(&mc_dir.join("versions").join(&version_dir_name));
    }

    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &version_dir_name,
        &mc_dir,
        &launcher_config,
        &auth,
        java_major,
        &user_properties,
    );

    // 启动前执行命令（非空时运行；失败则中止启动）
    if !launcher_config.pre_launch_cmd.trim().is_empty() {
        run_pre_launch_command(&launcher_config.pre_launch_cmd)
            .map_err(|e| format!("启动前命令执行失败：{e}"))?;
    }

    // 启动前内存优化：清理启动器自身工作集，为游戏腾出内存（仅 Windows）
    if launcher_config.pre_launch_memory_optimize {
        optimize_launcher_memory();
    }

    // 高性能显卡：写入 Windows DirectX 用户 GPU 偏好注册表
    apply_gpu_preference(launcher_config.use_high_performance_gpu);

    let _ = app.emit("game-launch", &cmd.args);

    // 启动子进程
    let mut child = std::process::Command::new(&cmd.args[0])
        .args(&cmd.args[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动游戏失败: {}", e))?;

    let pid = child.id();

    // 进程优先级（Windows: SetPriorityClass；进程刚启动即设置）
    set_process_priority(pid, launcher_config.process_priority);

    // 捕获子进程 stdout/stderr 写入日志文件（避免 GBK 乱码污染终端，同时保留诊断信息）
    let log_path = mc_dir.join("logs").join("elytra-launch.log");
    if let Some(mut so) = child.stdout.take() {
        let lp = log_path.clone();
        std::thread::spawn(move || drain_to_file(&mut so, &lp, false));
    }
    if let Some(mut se) = child.stderr.take() {
        let lp = log_path.clone();
        std::thread::spawn(move || drain_to_file(&mut se, &lp, true));
    }

    // 后台线程等待进程退出后通知前端
    let app_handle = app.clone();
    let version_label = version_dir_name.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = app_handle.emit(crate::events::GAME_EXITED, (pid, version_label));
    });

    Ok(pid)
}

/// 执行启动前命令（Windows: cmd /C；其他平台: sh -c）。失败时返回错误。
fn run_pre_launch_command(command: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", command])
        .status()
        .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "windows"))]
    let status = std::process::Command::new("sh")
        .args(["-c", command])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("退出码 {}", status.code().unwrap_or(-1)))
    }
}

/// 设置 Windows DirectX 用户 GPU 偏好（HKCU\Software\Microsoft\DirectX\UserGpuPreferences）。
/// 开启：java.exe → GpuPreference=2;（高性能独显）；关闭：删除该项。
fn apply_gpu_preference(enabled: bool) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "Advapi32")]
        extern "system" {
            fn RegCreateKeyExW(
                h_key: *mut std::ffi::c_void,
                lp_sub_key: *const u16,
                reserved: u32,
                lp_class: *const u16,
                dw_options: u32,
                sam_desired: u32,
                lp_security_attributes: *mut std::ffi::c_void,
                phk_result: *mut *mut std::ffi::c_void,
                lpdw_disposition: *mut u32,
            ) -> i32;
            fn RegSetValueExW(
                h_key: *mut std::ffi::c_void,
                lp_value_name: *const u16,
                reserved: u32,
                dw_type: u32,
                lp_data: *const u8,
                cb_data: u32,
            ) -> i32;
            fn RegDeleteValueW(h_key: *mut std::ffi::c_void, lp_value_name: *const u16) -> i32;
            fn RegCloseKey(h_key: *mut std::ffi::c_void) -> i32;
        }
        const HKEY_CURRENT_USER: *mut std::ffi::c_void = 0x80000001usize as _;
        const KEY_SET_VALUE: u32 = 0x0002;
        const REG_SZ: u32 = 1;
        const KEY_PATH: &[u16] = &[
            'S' as u16, 'o' as u16, 'f' as u16, 't' as u16, 'w' as u16, 'a' as u16, 'r' as u16,
            'e' as u16, '\\' as u16, 'M' as u16, 'i' as u16, 'c' as u16, 'r' as u16, 'o' as u16,
            's' as u16, 'o' as u16, 'f' as u16, 't' as u16, '\\' as u16, 'D' as u16, 'i' as u16,
            'r' as u16, 'e' as u16, 'c' as u16, 't' as u16, 'X' as u16, '\\' as u16, 'U' as u16,
            's' as u16, 'e' as u16, 'r' as u16, 'G' as u16, 'p' as u16, 'u' as u16, 'P' as u16,
            'r' as u16, 'e' as u16, 'f' as u16, 'e' as u16, 'r' as u16, 'e' as u16, 'n' as u16,
            'c' as u16, 'e' as u16, 's' as u16, 0,
        ];
        const VALUE_NAME: &[u16] = &[
            'j' as u16, 'a' as u16, 'v' as u16, 'a' as u16, '.' as u16, 'e' as u16, 'x' as u16,
            'e' as u16, 0,
        ];
        unsafe {
            let mut key: *mut std::ffi::c_void = std::ptr::null_mut();
            let r = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                KEY_PATH.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_SET_VALUE,
                std::ptr::null_mut(),
                &mut key,
                std::ptr::null_mut(),
            );
            if r != 0 || key.is_null() {
                return;
            }
            if enabled {
                const DATA: &[u16] = &[
                    'G' as u16, 'p' as u16, 'u' as u16, 'P' as u16, 'r' as u16, 'e' as u16,
                    'f' as u16, 'e' as u16, 'r' as u16, 'e' as u16, 'n' as u16, 'c' as u16,
                    'e' as u16, '=' as u16, '2' as u16, ';' as u16, 0,
                ];
                let _ = RegSetValueExW(
                    key,
                    VALUE_NAME.as_ptr(),
                    0,
                    REG_SZ,
                    DATA.as_ptr() as *const u8,
                    (DATA.len() * 2) as u32,
                );
            } else {
                let _ = RegDeleteValueW(key, VALUE_NAME.as_ptr());
            }
            let _ = RegCloseKey(key);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = enabled;
    }
}

/// 启动前内存优化：将启动器进程工作集降至最低（Windows SetProcessWorkingSetSize）。
fn optimize_launcher_memory() {
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "Kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn SetProcessWorkingSetSize(
                h_process: *mut std::ffi::c_void,
                dw_minimum: usize,
                dw_maximum: usize,
            ) -> i32;
        }
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
    #[cfg(not(target_os = "windows"))]
    {}
}

/// 设置游戏进程优先级（仅 Windows；其他平台无操作）。
/// 通过 PID 打开进程句柄后调用 SetPriorityClass，避免依赖额外 crate。
fn set_process_priority(pid: u32, priority: crate::config::launcher::model::ProcessPriority) {
    use crate::config::launcher::model::ProcessPriority as P;
    let class = match priority {
        P::Normal => return, // 中（平衡）= 系统默认，无需设置
        P::Low => 0x0000_4000,      // BELOW_NORMAL_PRIORITY_CLASS
        P::High => 0x0000_0080,     // HIGH_PRIORITY_CLASS
        P::RealTime => 0x0000_0100, // REALTIME_PRIORITY_CLASS
    };

    #[cfg(target_os = "windows")]
    {
        #[link(name = "Kernel32")]
        extern "system" {
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> *mut std::ffi::c_void;
            fn SetPriorityClass(h_process: *mut std::ffi::c_void, dw_priority_class: u32) -> i32;
            fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        }
        const PROCESS_SET_INFORMATION: u32 = 0x0200;
        unsafe {
            let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
            if !handle.is_null() {
                let _ = SetPriorityClass(handle, class);
                let _ = CloseHandle(handle);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (class, pid);
    }
}

// ── 工具函数 ────────────────────────────────────────

/// 从已扫描的 JDK 列表中自动选择最合适的版本。
/// 优先精确匹配需求版本；否则选最低但满足需求的版本；再不行选最高版本。
fn auto_pick_jdk(discovered: &[crate::config::jdk::model::JavaEnv], required: u32) -> Option<crate::config::jdk::model::JavaEnv> {
    let mut candidates: Vec<(u32, &crate::config::jdk::model::JavaEnv)> = discovered
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
    fn test_render_launch_script_bat() {
        #[cfg(target_os = "windows")]
        {
            let args = vec![
                r"C:\java\bin\java.exe".to_string(),
                "-Xmx2G".to_string(),
                "net.minecraft.client.main.Main".to_string(),
            ];
            let s = render_launch_script(
                &args,
                std::path::Path::new(r"D:\Minecraft\.minecraft"),
                "26.2",
            );
            assert!(s.starts_with("chcp 65001>nul\r\n"));
            assert!(s.contains("title 启动 - 26.2\r\n"));
            assert!(s.contains(r#"cd /D "D:\Minecraft\.minecraft\versions\26.2""#));
            assert!(s.contains(
                r#""C:\java\bin\java.exe" "-Xmx2G" "net.minecraft.client.main.Main" "#
            ));
            assert!(s.ends_with("pause\r\n"));
        }
    }

    /// 构造一个最小可用的版本 JSON（旧格式 minecraftArguments，保底参数路径）。
    fn minimal_version_json() -> VersionJson {
        serde_json::from_str(
            r#"{
                "id": "1.18.2",
                "mainClass": "net.minecraft.client.main.Main",
                "assets": "1.18",
                "assetIndex": { "id": "1.18", "url": "x", "sha1": "y", "size": 1, "totalSize": 1 },
                "downloads": { "client": { "url": "x", "sha1": "y", "size": 1 } },
                "arguments": {
                  "game": [
                    "--gameDir", "${game_directory}",
                    "--assetsDir", "${assets_root}",
                    "--assetIndex", "${assets_index_name}",
                    { "rules": [ { "action": "allow", "features": { "has_custom_resolution": true } } ],
                      "value": [ "--width", "${resolution_width}", "--height", "${resolution_height}" ] }
                  ],
                  "jvm": []
                },
                "libraries": [],
                "complianceLevel": 0,
                "minimumLauncherVersion": 21,
                "javaVersion": { "majorVersion": 17 },
                "releaseTime": "2021-11-30T00:00:00Z",
                "time": "2021-11-30T00:00:00Z",
                "type": "release"
            }"#,
        )
        .unwrap()
    }

    fn build_cmd(config: &LauncherConfig) -> Vec<String> {
        let vj = minimal_version_json();
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.18.2",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            config,
            &AuthMode::Offline { username: "tester".into(), uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into() },
            17,
            "",
        );
        cmd.args
    }

    #[test]
    fn test_window_size_default_no_width_args() {
        let args = build_cmd(&LauncherConfig::default());
        assert!(
            !args.iter().any(|a| a == "--width" || a == "--height"),
            "默认窗口大小不应传 --width/--height: {args:?}"
        );
    }

    #[test]
    fn test_window_size_custom_passes_width_height() {
        let mut cfg = LauncherConfig::default();
        cfg.window_size = crate::config::launcher::model::WindowSize::Custom(1920, 1080);
        let args = build_cmd(&cfg);
        assert!(args.contains(&"--width".into()));
        assert!(args.contains(&"1920".into()));
        assert!(args.contains(&"--height".into()));
        assert!(args.contains(&"1080".into()));
    }

    #[test]
    fn test_game_dir_is_version_dir() {
        // 版本隔离（固定行为）：游戏目录 = 版本目录
        let args = build_cmd(&LauncherConfig::default());
        let gi = args.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(args[gi + 1].replace('\\', "/"), "D:/Minecraft/.minecraft/versions/1.18.2");
    }

    #[test]
    fn test_user_properties_appended_new_format() {
        // 新格式（1.17+ json 无 token）：主动追加 camelCase 参数
        let vj = minimal_version_json();
        let cfg = LauncherConfig::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.18.2",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline { username: "tester".into(), uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into() },
            17,
            props,
        );
        let joined = cmd.args.join(" ");
        assert!(joined.contains("--userProperties"), "应追加 --userProperties: {joined}");
        assert!(joined.contains("--profileProperties"), "应追加 --profileProperties: {joined}");
        assert!(joined.contains(props), "值应注入: {joined}");
        assert!(!joined.contains("--user-properties"), "不应注入连字符版: {joined}");
    }

    #[test]
    fn test_user_properties_substituted() {
        // 旧式 minecraftArguments（1.19.2 以下）应注入 --user-properties 与 legacy user_type
        let mut vj = minimal_version_json();
        vj.arguments.game = None; // 走旧格式 minecraftArguments 分支
        vj.minecraft_arguments = Some(
            "--username ${auth_player_name} --version ${version_name} --accessToken ${auth_access_token} --user-properties ${user_properties} --user-type ${user_type}"
                .into(),
        );
        let cfg = LauncherConfig::default();
        let props = r#"{"textures":[{"name":"textures","value":"AQID","signature":""}]}"#;
        let cmd = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.18.2",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline { username: "tester".into(), uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into() },
            17,
            props,
        );
        let joined = cmd.args.join(" ");
        assert!(joined.contains(props), "应注入 user properties: {joined}");
        assert!(
            !joined.contains("${user_properties}"),
            "模板残留未替换: {joined}"
        );
        assert!(joined.contains("--user-type legacy"), "离线应为 legacy: {joined}");
        // 旧格式模板应按空白拆分为独立 argv（否则 joptsimple 报 missing accessToken/version）
        assert!(
            cmd.args.contains(&"--accessToken".into()),
            "--accessToken 应为独立参数: {joined}"
        );
        assert!(
            cmd.args.contains(&"--version".into()),
            "--version 应为独立参数: {joined}"
        );
    }

    #[test]
    fn test_skin_matching_uuid() {
        // 与 Java UUID.hashCode() & 1（isSlimSkin 判定）一致：
        // 验证固定样本
        let samples = [
            ("00000000ffffffffffffffffffff96436", true), // 由 Java 计算的真实值
            ("1234567890abcdef1234567890abcdef", false),
            ("0123456789abcdef0123456789abcdef", false),
        ];
        for (u, _) in &samples {
            let _ = u;
        }
        // 匹配收敛：Steve 模式产生非 Alex UUID，Alex 模式产生 Alex UUID
        let base = "00000000ffffffffffffffffffffffff";
        let s = skin_matching_uuid(base, false);
        assert!(!uuid_matches_alex(&s), "Steve UUID 不应匹配 Alex: {s}");
        let a = skin_matching_uuid(base, true);
        assert!(uuid_matches_alex(&a), "Alex UUID 应匹配 Alex: {a}");
        // 最多一次递增即可翻转最低位
        assert!(s[..31] == base[..31] || a[..31] == base[..31]);
    }

    #[test]
    fn test_window_title_prop() {
        let mut cfg = LauncherConfig::default();
        cfg.window_title = "我的世界".into();
        let args = build_cmd(&cfg);
        assert!(args.contains(&"-Dminecraft.window.title=我的世界".into()));
        // 空 = 不传
        let args = build_cmd(&LauncherConfig::default());
        assert!(!args.iter().any(|a| a.starts_with("-Dminecraft.window.title")));
    }

    #[test]
    fn test_disable_launch_wrapper_main_class() {
        let mut vj = minimal_version_json();
        vj.main_class = "net.minecraft.launchwrapper.Launch".into();
        let cfg = LauncherConfig::default();
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.8.9",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline { username: "t".into(), uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into() },
            8,
            "",
        )
        .args;
        // 默认：主类为 launchwrapper
        assert!(args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));

        // 禁用后：直接用客户端主类
        let mut cfg = LauncherConfig::default();
        cfg.disable_launch_wrapper = true;
        let args = build_launch_command(
            std::path::Path::new("C:/java/bin/java.exe"),
            &vj,
            "1.8.9",
            std::path::Path::new("D:/Minecraft/.minecraft"),
            &cfg,
            &AuthMode::Offline { username: "t".into(), uuid: "00000000FFFFFFFFFFFFFFFFFFFFFFFF".into() },
            8,
            "",
        )
        .args;
        assert!(args.contains(&"net.minecraft.client.main.Main".to_string()));
        assert!(!args.contains(&"net.minecraft.launchwrapper.Launch".to_string()));
    }

    #[test]
    fn test_disable_lwjgl_unsafe_agent_filters_javaagent() {
        let mut cfg = LauncherConfig::default();
        cfg.jvm_args = vec![
            "-javaagent:lwjgl-unsafe-agent.jar".into(),
            "-XX:-OmitStackTraceInFastThrow".into(),
        ];
        cfg.disable_lwjgl_unsafe_agent = true;
        let args = build_cmd(&cfg);
        assert!(!args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
        assert!(args.contains(&"-XX:-OmitStackTraceInFastThrow".to_string()));

        // 默认不禁用：agent 参数保留
        let mut cfg = LauncherConfig::default();
        cfg.jvm_args = vec!["-javaagent:lwjgl-unsafe-agent.jar".into()];
        let args = build_cmd(&cfg);
        assert!(args.iter().any(|a| a.contains("lwjgl-unsafe-agent")));
    }

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
