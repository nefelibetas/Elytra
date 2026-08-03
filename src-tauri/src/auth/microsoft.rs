use crate::auth::model::MsAccount;
use base64::Engine;
use serde_json::json;
use sha2::Digest;

// ── Azure 应用配置 ──────────────────────────────────────────────
// 使用 PrismLauncher 公开注册的 client id（GitHub 开源项目公开，支持任意
// localhost 端口回调）。注意：微软 2025 年起对 login_with_xbox 校验应用注册，
// 自己新注册的 Azure 应用（未验证）会返回 403 Invalid app registration，
// 因此不能换自定义 client id。
pub const MS_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";
/// 本地回调端口（唯一事实源：account.rs 监听与 REDIRECT_URI 都必须与此一致）。
pub const CALLBACK_PORT: u16 = 27205;
pub const REDIRECT_URI: &str = "http://localhost:27205";

// ── 端点 ─────────────────────────────────────────────────────────
const AUTH_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const REFRESH_URL: &str = "https://login.live.com/oauth20_token.srf";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";

/// XSTS 错误码 → 用户可读的中文说明。
fn xsts_error_message(xerr: i64) -> &'static str {
    match xerr {
        2148916233 => "该 Microsoft 账号没有 Xbox Live 账号，请先注册",
        2148916235 => "该账号所在地区不可用 Xbox Live",
        2148916236 => "未成年账号需要家长同意（家庭组）",
        2148916237 => "未成年账号需要家长同意（家庭组）",
        2148916238 => "未成年账号需要家长同意，请先加入家庭组",
        _ => "Xbox Live 鉴权失败，请稍后重试",
    }
}

#[derive(Debug)]
pub enum MsError {
    /// Azure 未配置 client_id
    NotConfigured,
    /// OAuth 授权阶段失败（含 error_description）
    OAuth(String),
    /// XSTS 阶段失败（XErr 码）
    Xsts(i64),
    /// 未购买 Minecraft（profile 404）
    NoPurchase,
    /// 账号被迁移/不存在
    NoProfile(String),
    /// 网络/HTTP 异常
    Http(String),
    /// 响应格式异常
    Malformed(String),
}

impl std::fmt::Display for MsError {
    /// 第一行为友好短提示（前端弹窗只显示这行），
    /// 换行后为详细技术信息（复制按钮复制全文）。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MsError::NotConfigured => write!(f, "Azure 客户端 ID 未配置"),
            MsError::OAuth(e) => write!(f, "微软登录失败\n{e}"),
            MsError::Xsts(x) => write!(f, "{}", xsts_error_message(*x)),
            MsError::NoPurchase => write!(f, "该账号未购买 Minecraft"),
            MsError::NoProfile(e) => write!(f, "获取游戏档案失败\n{e}"),
            MsError::Http(e) => write!(f, "网络请求失败\n{e}"),
            MsError::Malformed(e) => write!(f, "响应解析失败\n{e}"),
        }
    }
}

impl From<reqwest::Error> for MsError {
    fn from(e: reqwest::Error) -> Self {
        MsError::Http(e.to_string())
    }
}

// ── PKCE ─────────────────────────────────────────────────────────

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

/// 生成 PKCE code_verifier（43–128 字符）与 code_challenge（S256）。
pub fn generate_pkce() -> Pkce {
    let mut bytes = [0u8; 64];
    getrandom::getrandom(&mut bytes).expect("os rng unavailable");
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    Pkce { verifier, challenge }
}

/// 生成随机 state（防 CSRF）。
pub fn generate_state() -> String {
    let mut bytes = [0u8; 24];
    getrandom::getrandom(&mut bytes).expect("os rng unavailable");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 构建 application/x-www-form-urlencoded 请求体。
/// （reqwest 0.13 的 `.form()` 不可用，手工编码）
fn form_body(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 构建授权 URL。
pub fn build_auth_url(state: &str, pkce: &Pkce) -> String {
    format!(
        "{AUTH_URL}?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        urlencode(MS_CLIENT_ID),
        urlencode(REDIRECT_URI),
        urlencode(SCOPE),
        urlencode(state),
        urlencode(&pkce.challenge),
    )
}

// ── OAuth token 交换 ─────────────────────────────────────────────

#[derive(Debug)]
struct MsTokens {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

fn extract_tokens(json: &serde_json::Value) -> Result<MsTokens, MsError> {
    if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
        let desc = json
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return Err(MsError::OAuth(format!("{err}: {desc}")));
    }
    let access_token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("missing access_token".into()))?;
    let refresh_token = json
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("missing refresh_token".into()))?;
    let expires_in = json.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(3600);
    Ok(MsTokens {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.to_string(),
        expires_in,
    })
}

/// 用授权码换微软 token。
async fn exchange_code(client: &reqwest::Client, code: &str, verifier: &str) -> Result<MsTokens, MsError> {
    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body(&[
            ("client_id", MS_CLIENT_ID),
            ("scope", SCOPE),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ]))
        .send()
        .await?;
    let json: serde_json::Value = resp.json().await?;
    extract_tokens(&json)
}

/// 用 refresh_token 刷新微软 token。
async fn refresh_ms(client: &reqwest::Client, refresh_token: &str) -> Result<MsTokens, MsError> {
    let resp = client
        .post(REFRESH_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body(&[
            ("client_id", MS_CLIENT_ID),
            ("scope", SCOPE),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ]))
        .send()
        .await?;
    let json: serde_json::Value = resp.json().await?;
    extract_tokens(&json)
}

// ── Xbox / Minecraft 链路 ────────────────────────────────────────

/// 微软 token → XBL token + userhash。
async fn authenticate_xbl(client: &reqwest::Client, ms_access_token: &str) -> Result<(String, String), MsError> {
    let resp = client
        .post(XBL_URL)
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Err(MsError::Http(format!(
            "Xbox Live 鉴权失败({status}): {}",
            extract_api_error(&json)
        )));
    }
    let token = json
        .get("Token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("XBL missing Token".into()))?;
    let uhs = json
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("XBL missing uhs".into()))?;
    Ok((token.to_string(), uhs.to_string()))
}

/// XBL token → XSTS token + xuid。401 时返回 XErr 码。
/// xid（Xbox 账号 ID）部分账号缺失，非致命：缺失时降级为空串，不阻塞登录。
async fn authenticate_xsts(client: &reqwest::Client, xbl_token: &str) -> Result<(String, String), MsError> {
    let resp = client
        .post(XSTS_URL)
        .json(&json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let xerr = json.get("XErr").and_then(|v| v.as_i64()).unwrap_or(0);
        return Err(MsError::Xsts(xerr));
    }
    if !status.is_success() {
        return Err(MsError::Http(format!(
            "Xbox 鉴权失败({status}): {}",
            extract_api_error(&json)
        )));
    }
    let token = json
        .get("Token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("XSTS missing Token".into()))?;
    let xuid = json
        .pointer("/DisplayClaims/xui/0/xid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((token.to_string(), xuid))
}

/// 从错误 JSON 里提取可读的 errorMessage/error/path。
fn extract_api_error(json: &serde_json::Value) -> String {
    let msg = json
        .get("errorMessage")
        .or_else(|| json.get("error"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let path = json.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if path.is_empty() {
        msg.to_string()
    } else {
        format!("{msg} ({path})")
    }
}

/// XSTS token → Minecraft access_token。
async fn minecraft_login(
    client: &reqwest::Client,
    uhs: &str,
    xsts_token: &str,
) -> Result<String, MsError> {
    let resp = client
        .post(MC_LOGIN_URL)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await?;
    if !status.is_success() {
        return Err(MsError::Http(format!(
            "Minecraft 登录失败({status}): {}",
            extract_api_error(&json)
        )));
    }
    let token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("Minecraft login missing access_token".into()))?;
    Ok(token.to_string())
}

/// 查 Minecraft profile（uuid + 游戏名）。404 = 未购买。
async fn minecraft_profile(
    client: &reqwest::Client,
    mc_access_token: &str,
) -> Result<(String, String), MsError> {
    let resp = client
        .get(MC_PROFILE_URL)
        .bearer_auth(mc_access_token)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(MsError::NoPurchase);
    }
    let json: serde_json::Value = resp.json().await?;
    if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
        return Err(MsError::NoProfile(err.to_string()));
    }
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("profile missing id".into()))?;
    let name = json
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::Malformed("profile missing name".into()))?;
    Ok((id.to_string(), name.to_string()))
}

// ── 高层流程 ─────────────────────────────────────────────────────

async fn complete_after_ms_token(
    client: &reqwest::Client,
    tokens: &MsTokens,
) -> Result<MsAccount, MsError> {
    let (xbl_token, uhs) = authenticate_xbl(client, &tokens.access_token).await?;
    let (xsts_token, xuid) = authenticate_xsts(client, &xbl_token).await?;
    let mc_token = minecraft_login(client, &uhs, &xsts_token).await?;
    let (uuid, name) = minecraft_profile(client, &mc_token).await?;
    // XSTS 的 xid 字段部分账号缺失：从 Minecraft token 的 JWT payload 兜底提取 xuid
    let xuid = if xuid.is_empty() {
        extract_xuid_from_jwt(&mc_token)
    } else {
        xuid
    };
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + tokens.expires_in;
    Ok(MsAccount {
        uuid,
        name,
        access_token: mc_token,
        refresh_token: tokens.refresh_token.clone(),
        xuid,
        expires_at,
    })
}

/// 从 Minecraft access token（JWT）的 payload 提取 xuid claim；失败返回空串。
pub fn extract_xuid_from_jwt(token: &str) -> String {
    let Some(payload) = token.split('.').nth(1) else {
        return String::new();
    };
    let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else {
        return String::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
        return String::new();
    };
    match json.get("xuid") {
        Some(v) => v
            .as_str()
            .map(str::to_string)
            .or_else(|| v.as_i64().map(|n| n.to_string()))
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// 构建正版登录专用 HTTP client；按「正版登录时验证 SSL 证书」配置决定是否校验证书。
fn build_auth_client() -> reqwest::Client {
    let verify = crate::persist::read::<crate::config::downloader::model::DownloaderConfig>("downloader")
        .map(|c| c.verify_ssl)
        .unwrap_or(false);
    reqwest::Client::builder()
        .danger_accept_invalid_certs(!verify)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 授权码 → 完整登录。
pub async fn login_with_code(code: &str, verifier: &str) -> Result<MsAccount, MsError> {
    if MS_CLIENT_ID.starts_with("PASTE_") {
        return Err(MsError::NotConfigured);
    }
    let client = build_auth_client();
    let tokens = exchange_code(&client, code, verifier).await?;
    complete_after_ms_token(&client, &tokens).await
}

/// refresh_token → 刷新并重新拉取完整账号。
pub async fn refresh_login(account: &MsAccount) -> Result<MsAccount, MsError> {
    if MS_CLIENT_ID.starts_with("PASTE_") {
        return Err(MsError::NotConfigured);
    }
    let client = build_auth_client();
    let tokens = refresh_ms(&client, &account.refresh_token).await?;
    complete_after_ms_token(&client, &tokens).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let pkce = generate_pkce();
        assert!((43..=128).contains(&pkce.verifier.len()));
        assert_eq!(pkce.challenge.len(), 43); // 32 bytes → 43 base64url chars
        // challenge 必须是 verifier 的 sha256（S256 一致性）
        let digest = sha2::Sha256::digest(pkce.verifier.as_bytes());
        let expect = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        assert_eq!(pkce.challenge, expect);
        // 两次生成应不同
        let pkce2 = generate_pkce();
        assert_ne!(pkce.verifier, pkce2.verifier);
    }

    #[test]
    fn test_state_random() {
        let a = generate_state();
        let b = generate_state();
        assert_ne!(a, b);
        assert!(a.len() >= 24);
    }

    #[test]
    fn test_build_auth_url() {
        let pkce = generate_pkce();
        let url = build_auth_url("st8", &pkce);
        assert!(url.starts_with(AUTH_URL));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A27205"));
        assert!(url.contains("scope=XboxLive.signin%20offline_access"));
        assert!(url.contains("state=st8"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_extract_tokens_error() {
        let json = serde_json::json!({
            "error": "invalid_grant",
            "error_description": "AADSTS70008: code expired"
        });
        let err = extract_tokens(&json).unwrap_err();
        assert!(matches!(err, MsError::OAuth(_)));
    }

    #[test]
    fn test_redirect_uri_matches_callback_port() {
        assert!(
            REDIRECT_URI.ends_with(&format!(":{CALLBACK_PORT}")),
            "REDIRECT_URI 与 CALLBACK_PORT 不一致：{REDIRECT_URI} vs {CALLBACK_PORT}"
        );
    }
}
