//! 离线皮肤机制（对齐 PCL2）：UUID 模型匹配、自定义皮肤资源包、user-properties 注入。
//! 从 commands/launch.rs 下沉；Player 模式经 auth/mojang 拉取正版皮肤。

use crate::config::launcher::model::AuthMode;
use crate::config::offline_skin::OfflineSkinConfig;
use crate::downloader::model::VersionJson;

/// 离线默认 UUID（固定占位）。
const OFFLINE_BASE_UUID: &str = "00000000FFFFFFFFFFFFFFFFFFFFFFFF";

/// Alex 官方默认皮肤纹理（textures.minecraft.net 固定 hash；与游戏内置 alex.png 一致，已验证）。
/// Steve 不注入：游戏默认纹理即经典蓝衬衫 Steve。
const ALEX_TEXTURE_URL: &str = "https://textures.minecraft.net/texture/46acd06e8483b176e8ea39fc12fe105eb3a2a4970f5100057e9d84d4b60bdfa7";

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

/// 应用离线皮肤（PCL 机制）：
/// - UUID 策略：Steve/Alex/Custom 生成匹配模型的 UUID；Player 用正版玩家真实 UUID
///   （游戏内按 UUID 哈希显示对应默认皮肤，1.19.2 以下；
///   Player 模式游戏内还会经 sessionserver 拉取正版皮肤，需网络）；
/// - Custom：构建替换默认纹理的资源包（resourcepacks/elytra-skin.zip）并写入 options.txt 启用。
///
/// `skin` 由调用方读取一次传入（同一启动流程的 build_user_properties 共用，避免双读）。
pub async fn apply_offline_skin(
    auth: &mut AuthMode,
    version_json: &VersionJson,
    mc_dir: &std::path::Path,
    version_dir_name: &str,
    skin: &OfflineSkinConfig,
) {
    let AuthMode::Offline { username, uuid } = auth else {
        return;
    };
    if uuid.is_empty() {
        *uuid = OFFLINE_BASE_UUID.to_string();
    }

    use crate::config::offline_skin::OfflineSkinType as T;

    match skin.skin_type {
        T::Steve => *uuid = skin_matching_uuid(uuid, false),
        T::Alex => *uuid = skin_matching_uuid(uuid, true),
        // 随机：与前端 hashName 一致（哈希奇数 → Alex），游戏内按匹配 UUID 显示同一模型
        T::Random => *uuid = skin_matching_uuid(uuid, random_is_alex(username)),
        T::Player => {
            let name = skin.player_name.trim();
            if !name.is_empty() {
                if let Ok(real) = crate::auth::mojang::name_to_uuid(name).await {
                    *uuid = real;
                }
            }
        }
        T::Custom => {
            let path = skin.custom_path.trim();
            let slim = custom_skin_slim(skin);
            *uuid = skin_matching_uuid(uuid, slim);
            if !path.is_empty() && std::path::Path::new(path).exists() {
                let game_dir = mc_dir.join("versions").join(version_dir_name);
                apply_custom_skin_pack(&game_dir, path, slim, version_json);
            }
        }
    }
    // 用户名不变（PCL 同样使用输入的用户名）
}

/// 构建自定义皮肤资源包并写入 options.txt 启用（对齐 PCL2 Skin.zip 机制）。
/// 仅对 1.6+ 版本有效；替换 steve.png/alex.png 默认纹理为本地皮肤文件。
fn apply_custom_skin_pack(
    game_dir: &std::path::Path,
    skin_path: &str,
    slim: bool,
    version_json: &VersionJson,
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
    let pack_entry = if major >= 13 {
        "\"file/elytra-skin.zip\""
    } else {
        "\"elytra-skin.zip\""
    };
    crate::config::launcher::env::upsert_option(&game_dir.join("options.txt"), "resourcePacks", &format!("[{pack_entry}]"));
    println!("[skin] 自定义皮肤资源包已启用 ({tex_name}, format {pack_format})");
}

/// 版本主号（"1.12.2" → 12）。
fn version_major(version_json: &VersionJson) -> u32 {
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
fn pack_format_for(major: u32, version_json: &VersionJson) -> u32 {
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

/// FNV-1a 哈希（与前端 Launch.vue hashName 一致）
fn hash_name(name: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Random 皮肤模型决策：与前端 hashName 一致（哈希奇数 → Alex）。
/// apply_offline_skin（UUID）与 build_user_properties（纹理注入）共用，保证一致。
fn random_is_alex(username: &str) -> bool {
    hash_name(username) % 2 == 1
}

/// 自定义皮肤的 slim 判定：Steve=经典（粗手臂），Alex=slim（细手臂），
/// Auto=按皮肤 PNG 高度自动识别（32px 高 → slim，64px 高 → 经典）。
/// 未知/读取失败按经典处理（与旧行为一致）。
fn custom_skin_slim(skin: &crate::config::offline_skin::OfflineSkinConfig) -> bool {
    use crate::config::offline_skin::OfflineSkinModel;
    match skin.custom_model {
        OfflineSkinModel::Alex => true,
        OfflineSkinModel::Auto => {
            png_height(std::path::Path::new(&skin.custom_path)) == Some(32)
        }
        OfflineSkinModel::Steve => false,
    }
}

/// 读取 PNG 头部 IHDR 中的图像高度（字节 20-23 大端）。
fn png_height(path: &std::path::Path) -> Option<u32> {
    use std::io::Read;
    let mut buf = [0u8; 24];
    let mut f = std::fs::File::open(path).ok()?;
    f.read_exact(&mut buf).ok()?;
    // 签名 + IHDR chunk：宽/高为 4 字节大端
    if &buf[0..8] != b"\x89PNG\r\n\x1a\n" || &buf[12..16] != b"IHDR" {
        return None;
    }
    Some(u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]))
}

/// 构造 `--user-properties` 参数值（authlib user properties）。
/// 仅 1.19.2 以下版本的游戏参数含 `${user_properties}`，离线模式 authlib 不校验签名，
/// 注入纹理即可自定义皮肤（对齐 PCL「离线皮肤仅 1.19.2 以下生效」）。
/// 在线模式：返回空（皮肤走 sessionserver）。
/// Random/Steve/Alex 也注入对应默认纹理（离线 UUID 固定，游戏默认纹理恒为 Steve；
/// 注入才能与启动器头像渲染一致；Alex 必须注入 slim 纹理）。
/// 格式统一为 authlib PropertyMap 字符串数组 `{"textures":["<b64>"]}`
/// （1.12-1.19.2 的 PropertyMap$Serializer 均按此解析）。
pub async fn build_user_properties(auth: &AuthMode, skin: &OfflineSkinConfig) -> String {
    let AuthMode::Offline { username, .. } = auth else {
        return String::new();
    };

    use crate::config::offline_skin::OfflineSkinType as T;
    let value_b64 = match skin.skin_type {
        // Steve：不注入——游戏默认纹理即经典蓝衬衫 Steve（离线 UUID 固定默认也是 Steve）
        T::Steve => return String::new(),
        T::Alex => textures_payload(ALEX_TEXTURE_URL, true),
        T::Random => {
            // 与前端 hashName 一致：hash % 2 == 1 → Alex（注入）；否则 Steve（默认，不注入）
            if random_is_alex(username) {
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
            match crate::auth::mojang::texture_value(name).await.ok() {
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
            textures_payload(url.as_str(), custom_skin_slim(skin))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skin_matching_uuid() {
        // 与 Java UUID.hashCode() & 1（isSlimSkin 判定）一致
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
    fn test_png_height() {
        // 最小 PNG 头：签名 + IHDR（64×32 → slim）
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        png.extend_from_slice(&13u32.to_be_bytes()); // IHDR 数据长度
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&64u32.to_be_bytes()); // 宽
        png.extend_from_slice(&32u32.to_be_bytes()); // 高
        png.extend_from_slice(&[0u8; 5]); // 位深/颜色类型等
        let path = std::env::temp_dir().join("elytra_test_png_height.png");
        std::fs::write(&path, &png).unwrap();
        assert_eq!(png_height(&path), Some(32));
        let _ = std::fs::remove_file(&path);
    }
}
