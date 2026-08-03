//! Mojang 官方 API 客户端（玩家查询 / 皮肤纹理）。
//! 供 commands/account.rs（账号与皮肤）与 commands/launch.rs（离线皮肤）共用，
//! 消除 name→UUID 与 textures property 拉取的多处重复实现。

use crate::downloader::fetcher::http;

/// 玩家名 → Mojang UUID（api.mojang.com）。
/// 404 返回「玩家不存在」；其余网络/解析错误带详情。
pub async fn name_to_uuid(name: &str) -> Result<String, String> {
    let resp = http()
        .get(format!(
            "https://api.mojang.com/users/profiles/minecraft/{}",
            name.trim()
        ))
        .send()
        .await
        .map_err(|e| format!("查询玩家失败：{e}"))?;
    if resp.status() == 404 {
        return Err(format!("玩家「{name}」不存在"));
    }
    if !resp.status().is_success() {
        return Err(format!("查询玩家失败({})", resp.status()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    json["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "查询结果异常".to_string())
}

/// 拉取 textures property 的 value（base64 payload，非签名版）。
/// 统一带 `?unsigned=false`：解码结果与签名版一致，且体积更小。
pub async fn texture_value(uuid: &str) -> Result<String, String> {
    let resp = http()
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false"
        ))
        .send()
        .await
        .map_err(|e| format!("获取档案失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("获取档案失败：HTTP {}", resp.status()));
    }
    let profile: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let props = profile
        .get("properties")
        .and_then(|v| v.as_array())
        .ok_or("档案属性缺失")?;
    let textures_prop = props
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))
        .ok_or("该玩家未设置皮肤")?;
    textures_prop
        .get("value")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "纹理数据缺失".to_string())
}

/// 拉取角色纹理并解码（base64 NBT → JSON），含 textures/SKIN、textures/CAPE 等字段。
pub async fn profile_textures(uuid: &str) -> Result<serde_json::Value, String> {
    use base64::Engine;
    let value_b64 = texture_value(uuid).await?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value_b64)
        .map_err(|e| format!("皮肤数据解码失败：{e}"))?;
    serde_json::from_slice(&decoded).map_err(|e| format!("皮肤数据解析失败：{e}"))
}
