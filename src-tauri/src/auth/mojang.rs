//! Mojang 官方 API 客户端（玩家查询 / 皮肤纹理），供 commands/account.rs 共用。

use crate::downloader::fetcher::http;
use base64::Engine;

/// 拉取 textures property 的 value（base64 payload，非签名版）；统一带 `?unsigned=false`（解码一致且体积更小）。
pub async fn texture_value(uuid: &str) -> Result<String, String> {
    let resp = http()
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false"
        ))
        .send()
        .await
        .map_err(common_err_ctx_key!("errors.account.failed", "获取档案失败"))?;
    if !resp.status().is_success() {
        bail_key!("errors.account.failed", "获取档案失败\nHTTP {}", resp.status());
    }
    let profile: serde_json::Value = resp.json().await.map_err(common_err_ctx_key!("errors.parse", "解析响应失败"))?;
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
    let value_b64 = texture_value(uuid).await?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value_b64)
        .map_err(common_err_ctx_key!("errors.skin.load", "皮肤数据解码失败"))?;
    serde_json::from_slice(&decoded).map_err(common_err_ctx_key!("errors.skin.load", "皮肤数据解析失败"))
}

/// 拉取正版玩家皮肤（含模型类型）；官方 sessionserver 失败时回退 minotar
/// （第三方皮肤源；无 model 信息 → "classic"）。
/// 仅头像渲染路径用（装饰性）；主动操作走官方直连 fetch_skin，失败需如实报错。
pub async fn skin_profile_with_fallback(uuid: &str) -> Result<(String, String), String> {
    match profile_textures(uuid).await {
        Ok(t) => {
            let skin = &t["textures"]["SKIN"];
            let url = skin["url"].as_str().ok_or("该账号未设置皮肤")?;
            let model = skin["metadata"]["model"].as_str().unwrap_or("classic");
            Ok((url.to_string(), model.to_string()))
        }
        Err(_) => Ok((
            format!("https://minotar.net/skin/{uuid}"),
            "classic".to_string(),
        )),
    }
}
