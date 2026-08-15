//! 离线皮肤机制：UUID 模型匹配 + user-properties 纹理注入。
//! 固定随机行为（用户名哈希决定 Steve/Alex，与游戏内默认皮肤一致）；
//! 配置层已砍（2026-08），后续由自定义皮肤站登录（外置认证）接管皮肤注入。

use crate::config::launcher::model::AuthMode;

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

/// 应用离线皮肤：按用户名哈希（random_is_alex）生成匹配模型的 UUID，
/// 游戏内按 UUID 哈希显示对应默认皮肤（与前端头像渲染一致）。
pub fn apply_offline_skin(auth: &mut AuthMode) {
    let AuthMode::Offline { username, uuid } = auth else {
        return;
    };
    if uuid.is_empty() {
        *uuid = OFFLINE_BASE_UUID.to_string();
    }
    *uuid = skin_matching_uuid(uuid, random_is_alex(username));
}

/// FNV-1a 哈希（与前端 hashName 一致）。
fn hash_name(name: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Random 皮肤模型决策：哈希奇数 → Alex（slim）。apply_offline_skin 与 build_user_properties 共用。
fn random_is_alex(username: &str) -> bool {
    hash_name(username) % 2 == 1
}

/// 构造 `--user-properties` 参数值（authlib user properties）。
/// 仅 1.19.2 以下版本的游戏参数含 `${user_properties}`，离线模式 authlib 不校验签名，
/// 注入纹理即可自定义皮肤（对齐 PCL「离线皮肤仅 1.19.2 以下生效」）。
/// 在线模式：返回空（皮肤走 sessionserver）。
/// Alex（哈希决策）注入 slim 纹理；Steve 不注入（游戏默认纹理即经典 Steve）。
/// 格式统一为 authlib PropertyMap 字符串数组 `{"textures":["<b64>"]}`
/// （1.12-1.19.2 的 PropertyMap$Serializer 均按此解析）。
pub fn build_user_properties(auth: &AuthMode) -> String {
    let AuthMode::Offline { username, .. } = auth else {
        return String::new();
    };
    if !random_is_alex(username) {
        return String::new();
    }
    let value_b64 = textures_payload(ALEX_TEXTURE_URL, true);
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
    fn test_random_decision_consistent() {
        // UUID 决策与纹理注入决策一致（同一哈希）；注入返回值与决策对应
        let base = "00000000ffffffffffffffffffffffff";
        for name in ["Steve", "Alex", "BridgeFishDev", "Notch", "测试用户"] {
            let want_alex = random_is_alex(name);
            let uuid = skin_matching_uuid(base, want_alex);
            assert_eq!(
                uuid_matches_alex(&uuid),
                want_alex,
                "Random 决策 {name} 应一致",
            );
            let props = build_user_properties(&AuthMode::Offline {
                username: name.into(),
                uuid: base.into(),
            });
            assert_eq!(
                !props.is_empty(),
                want_alex,
                "user-properties 注入应与 Alex 决策一致: {name}",
            );
        }
    }
}
