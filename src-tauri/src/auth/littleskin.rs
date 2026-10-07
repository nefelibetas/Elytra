//! LittleSkin 第三方站皮肤/披风管理域（Blessing Skin API + Yggdrasil Connect OAuth）。
//!
//! # 能力边界（关键结论：站点没有公开的材质上传端点）
//!
//! 启动器**无法**把本地 PNG 上传到第三方站，依据：
//! - Blessing Skin 6.0.2 的 `routes/api.php` 只有 `user` / `players` / `closet` / `admin`
//!   四组路由，**没有** 材质上传端点；唯一上传入口是网页表单 `POST /skinlib`
//!   （`SkinlibController@handleUpload`，依赖浏览器会话与 CSRF，不属于 OAuth API）。
//! - `ClosetController@add` 只接受站点皮肤库中已存在的材质 TID（`Texture::find($tid)`），
//!   并要求该材质公开或归当前用户所有。
//! - `PlayerController@setTexture` 除 TID 外还要求该材质**已在用户衣柜中**
//!   （`$user->closet()->where('texture_tid', $tid)->doesntExist()` 即报错）。
//! - LittleSkin 的 Yggdrasil Connect 权限只有 `openid` / `offline_access` / `Yggdrasil.*`，
//!   没有材质写权限；其 Yggdrasil 路由 `PUT /api/yggdrasil/api/user/profile/{uuid}/skin`
//!   实测返回 404。
//!
//! 因此本模块的写入能力是「管理站点上**已存在**的材质」：按 TID 加入/移出衣柜、
//! 改名、应用到角色、重置。要新增皮肤，只能引导用户在站点网页上传后取其 TID
//! （材质页 `https://littleskin.cn/skinlib/show/{tid}` 的 URL 末尾数字）。
//!
//! # 组成
//! - OAuth 设备代码流（RFC 8628）：[`start_device_authorization`] / [`poll_device_authorization`]
//! - 令牌（access / refresh）经 [`persist::secrets`] 加密存储，临近过期自动刷新；**令牌绝不进日志**
//! - 业务 API：[`fetch_user`] / [`fetch_players`] / [`apply_textures`] / [`reset_textures`] /
//!   [`fetch_closet`] / [`add_closet_item`] / [`rename_closet_item`] / [`remove_closet_item`]

use std::collections::HashMap;
use std::future::Future;
use std::sync::LazyLock;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::constants;
use crate::downloader;
use crate::persist;

/// 单次 API 请求超时（含令牌端点）。
const API_TIMEOUT: Duration = Duration::from_secs(20);

/// 令牌端点未返回 `expires_in` 时的保守假设（1 小时）。
/// 取 0 会让每次业务请求都判定为「已过期」而反复刷新，白白消耗速率限制。
const DEFAULT_EXPIRES_IN: u64 = 3600;

/// 设备代码流申请的权限（空格分隔）：读用户信息 + 读写角色材质 + 读写衣柜 + 刷新令牌。
///
/// 不含 `openid`（不需要 ID 令牌，身份信息走 `GET /api/user`，避免解码不透明令牌）；
/// 不含 `Yggdrasil.*`（与材质管理无关，且 `PlayerProfiles.Read` 与 `Select` 互斥）。
pub const SCOPE: &str = "User.Read Player.ReadWrite Closet.ReadWrite offline_access";

/// 设备代码流 grant_type（RFC 8628）。
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// 未配置客户端 ID 时的可读错误（首行 + 细节，不 panic）。
const NOT_CONFIGURED: &str = "未配置 LittleSkin 客户端 ID，第三方站皮肤管理不可用\n\
     请在项目根目录 .env 中设置 ELYTRA_LITTLESKIN_CLIENT_ID 后重新构建启动器";

/// 未授权（本地无令牌）时的可读错误。
const NOT_AUTHORIZED: &str = "尚未授权 LittleSkin\n请先在启动器中完成授权（设备代码流）";

// ---------------------------------------------------------------- 基础工具

/// LittleSkin 客户端 ID（构建时经 `build.rs` 从 `.env` 注入）。
/// 未配置/为空 → 可读错误（**绝不 panic**）。
pub fn client_id() -> Result<&'static str, String> {
    constants::LITTLESKIN_CLIENT_ID
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| NOT_CONFIGURED.to_string())
}

/// 复用的 HTTP 客户端（带系统代理与超时；与下载/登录链路同一实例）。
fn client() -> std::sync::Arc<reqwest::Client> {
    downloader::fetcher::http()
}

/// 当前 Unix 秒（时钟异常回退 0）。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 截断文本供错误细节使用（避免把整页 HTML 塞进用户可见错误）。
fn truncate(text: &str) -> String {
    const MAX_CHARS: usize = 200;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(MAX_CHARS).collect();
        out.push('…');
        out
    }
}

/// `application/x-www-form-urlencoded` 单分量编码（纯函数）：
/// 仅保留 RFC 3986 unreserved 字符（`A-Za-z0-9-._~`），空格 → `+`，其余字节 → `%XX`（大写）。
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 表单体 / 查询串构造（纯函数）：`k=v&k=v`，键值均按 urlencoded 规则编码。
fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// JSON 值 → i64（容忍字符串数字；缺失/非法 → None）。
fn json_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// 取必需字符串字段（纯函数）。
fn required_str(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("响应缺少字段 {key}"))
}

/// 校验站点 ID（pid / tid）为正整数——它们会拼进 URL 路径，必须防止路径注入。
fn ensure_positive_id(value: i64, what: &str) -> Result<i64, String> {
    if value > 0 {
        Ok(value)
    } else {
        Err(format!("{what} 非法：{value}\n必须为正整数"))
    }
}

/// 校验衣柜条目名称：去首尾空白后非空。
fn ensure_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        Err("名称不能为空\n衣柜条目名称用于在站点的衣柜中展示".to_string())
    } else {
        Ok(name.to_string())
    }
}

/// 错误细节兜底：服务端未给 `error_description` 时退回 HTTP 状态。
fn detail_or_status(detail: String, status: reqwest::StatusCode) -> String {
    if detail.trim().is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {detail}")
    }
}

// ---------------------------------------------------------------- OAuth 错误

/// 令牌端点错误分类（RFC 6749 §5.2 / RFC 8628 §3.5 + LittleSkin 定制）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthError {
    /// 用户尚未完成授权：按 interval 继续轮询
    AuthorizationPending,
    /// 轮询过快：按 RFC 8628 放慢后继续
    SlowDown,
    /// 用户拒绝授权
    AccessDenied,
    /// 设备代码 / 用户代码已过期，需重新发起授权
    ExpiredToken,
    /// 应用未通过 LittleSkin 设备代码流白名单审核（LittleSkin 定制）
    InvalidClient,
    /// 请求参数非法
    InvalidRequest,
    /// 申请的权限不可用（含冲突的 Yggdrasil 权限组合）
    InvalidScope,
    /// 授权已失效 / 被吊销（刷新令牌场景）
    InvalidGrant,
    /// 其它未归类错误
    Other,
}

impl OAuthError {
    /// 由 OAuth `error` 字段取值分类（纯函数）。
    pub fn from_code(code: &str) -> Self {
        match code {
            "authorization_pending" => Self::AuthorizationPending,
            "slow_down" => Self::SlowDown,
            "access_denied" => Self::AccessDenied,
            "expired_token" => Self::ExpiredToken,
            "invalid_client" => Self::InvalidClient,
            "invalid_request" => Self::InvalidRequest,
            "invalid_scope" => Self::InvalidScope,
            "invalid_grant" => Self::InvalidGrant,
            _ => Self::Other,
        }
    }

    /// 面向用户的中文短文案（错误消息首行）。
    pub fn message(self) -> &'static str {
        match self {
            Self::AuthorizationPending => "等待用户在浏览器中完成授权",
            Self::SlowDown => "授权轮询过于频繁",
            Self::AccessDenied => "用户拒绝了授权",
            Self::ExpiredToken => "授权已超时，请重新发起授权",
            Self::InvalidClient => {
                "应用未通过 LittleSkin 设备代码流白名单审核\n\
                 请在 LittleSkin「OAuth 2 应用」管理页把回调 URL 设为 \
                 https://open.littleskin.cn/oauth/callback，并向官方邮箱发送标题为\
                 「申请 OAuth 设备代码流白名单」的邮件工单（一般一周内审核）"
            }
            Self::InvalidRequest => "授权请求参数错误",
            Self::InvalidScope => "申请的权限不被 LittleSkin 支持",
            Self::InvalidGrant => "LittleSkin 授权已失效，请重新授权",
            Self::Other => "LittleSkin 授权失败",
        }
    }

    /// 是否为终态错误（`authorization_pending` / `slow_down` 需继续轮询）。
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::AuthorizationPending | Self::SlowDown)
    }
}

/// 解析令牌端点错误响应（纯函数）→ 分类 + 服务端 `error_description` 细节。
fn parse_oauth_error(body: &Value) -> (OAuthError, String) {
    let code = body.get("error").and_then(Value::as_str).unwrap_or("");
    let detail = body
        .get("error_description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| code.to_string());
    (OAuthError::from_code(code), detail)
}

// ---------------------------------------------------------------- 令牌

/// LittleSkin OAuth 令牌（经 [`persist::secrets`] 加密存储；**绝不写日志、不回传前端**）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsToken {
    /// Bearer 访问令牌
    pub access_token: String,
    /// 刷新令牌（申请 `offline_access` 才返回）
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// 过期时刻（Unix 秒，本机时钟）
    pub expires_at: u64,
    /// 站点用户 UID（`GET /api/user`）
    #[serde(default)]
    pub uid: Option<i64>,
    /// 站点昵称
    #[serde(default)]
    pub nickname: Option<String>,
}

impl LsToken {
    /// 对外视图（**只含非敏感字段**）。
    pub fn view(&self) -> AccountView {
        AccountView {
            uid: self.uid,
            nickname: self.nickname.clone(),
            expires_at: self.expires_at,
        }
    }
}

/// 已授权站点账号的对外视图（不含任何令牌字段）。
#[derive(Debug, Clone, Serialize)]
pub struct AccountView {
    /// 站点用户 UID（未取到时为 None）
    pub uid: Option<i64>,
    /// 站点昵称
    pub nickname: Option<String>,
    /// 访问令牌过期时刻（Unix 秒）
    pub expires_at: u64,
}

/// 令牌是否临近 / 已过期（纯函数）：提前 [`constants::LITTLESKIN_REFRESH_SKEW_SECS`] 秒视为过期。
fn is_expired(token: &LsToken, now: u64) -> bool {
    now.saturating_add(constants::LITTLESKIN_REFRESH_SKEW_SECS) >= token.expires_at
}

/// 解析令牌响应（纯函数）：`{token_type, expires_in, access_token, refresh_token?, id_token?}`。
///
/// `id_token` **不解析也不存储**——LittleSkin 明确要求把令牌视为不透明字符串，
/// 身份信息改用 `GET /api/user` 获取。
fn parse_token_response(value: &Value, now: u64) -> Result<LsToken, String> {
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "授权响应缺少 access_token\n请重试；若持续失败，请确认应用已通过 LittleSkin 白名单审核"
                .to_string()
        })?;
    let expires_in = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_EXPIRES_IN);
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(LsToken {
        access_token: access_token.to_string(),
        refresh_token,
        expires_at: now.saturating_add(expires_in),
        uid: None,
        nickname: None,
    })
}

/// 读取本地令牌（无 / 解密失败 → None）。
fn load_token() -> Option<LsToken> {
    persist::secrets::read_secret(constants::SECT_LITTLESKIN_TOKEN)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// 保存令牌（加密存储）。
fn save_token(token: &LsToken) -> Result<(), String> {
    let bytes = serde_json::to_vec(token).map_err(|e| format!("保存授权信息失败\n{e}"))?;
    persist::secrets::save_secret(constants::SECT_LITTLESKIN_TOKEN, &bytes)
}

/// 本地授权状态（**不联网**）：None = 未授权。
pub fn account_view() -> Option<AccountView> {
    load_token().map(|t| t.view())
}

/// 退出授权：清除本地加密令牌并丢弃进行中的授权会话。
pub fn logout() -> Result<(), String> {
    PENDING.lock().clear();
    persist::secrets::remove_secret(constants::SECT_LITTLESKIN_TOKEN)
}

/// 当前有效访问令牌：无令牌 → 未授权；临近过期 → 先刷新。
async fn access_token() -> Result<String, String> {
    let token = load_token().ok_or_else(|| NOT_AUTHORIZED.to_string())?;
    if !is_expired(&token, now_secs()) {
        return Ok(token.access_token);
    }
    Ok(refresh(&token).await?.access_token)
}

/// 强制刷新（收到 401 后重试一次用）。
async fn force_refresh() -> Result<String, String> {
    let token = load_token().ok_or_else(|| NOT_AUTHORIZED.to_string())?;
    Ok(refresh(&token).await?.access_token)
}

/// 用刷新令牌换新访问令牌并落盘。
///
/// 刷新令牌失效 / 应用被移出白名单时清除本地令牌，要求用户重新授权。
async fn refresh(token: &LsToken) -> Result<LsToken, String> {
    let refresh_token = token.refresh_token.clone().ok_or_else(|| {
        "LittleSkin 授权已失效，请重新授权\n未取得刷新令牌（申请 offline_access 权限后才会返回）"
            .to_string()
    })?;
    let client_id = client_id()?;
    let body = form_encode(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", client_id),
    ]);
    let resp = client()
        .post(format!("{}/oauth/token", constants::LITTLESKIN_OPEN))
        .header(reqwest::header::ACCEPT, "application/json")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("刷新 LittleSkin 令牌失败\n{e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    // 令牌端点响应不回显到错误消息里：任何情况下都不把可能含令牌的字节带进日志/UI
    let value: Value = serde_json::from_str(&text).map_err(|_| {
        format!("刷新 LittleSkin 令牌失败\nHTTP {status}：响应不是 JSON")
    })?;
    if let Some(code) = value.get("error").and_then(Value::as_str) {
        let (kind, detail) = parse_oauth_error(&value);
        if matches!(
            OAuthError::from_code(code),
            OAuthError::InvalidGrant | OAuthError::InvalidClient
        ) {
            // 刷新令牌被吊销 / 应用不再受信：本地令牌已无价值，清掉以免反复失败
            let _ = persist::secrets::remove_secret(constants::SECT_LITTLESKIN_TOKEN);
        }
        return Err(format!(
            "{}\n{}",
            kind.message(),
            detail_or_status(detail, status)
        ));
    }
    let mut fresh = parse_token_response(&value, now_secs())
        .map_err(|e| format!("刷新 LittleSkin 令牌失败\n{e}"))?;
    // 刷新响应可能不下发新的 refresh_token：沿用旧的
    if fresh.refresh_token.is_none() {
        fresh.refresh_token = token.refresh_token.clone();
    }
    fresh.uid = token.uid;
    fresh.nickname = token.nickname.clone();
    save_token(&fresh)?;
    Ok(fresh)
}

// ---------------------------------------------------------------- 设备代码流

/// 进行中的授权会话（**仅存进程内存**：device_code 不落盘、不回传前端）。
#[derive(Debug, Clone)]
struct PendingAuth {
    device_code: String,
    interval: u64,
    /// 代码对过期时刻（Unix 秒）
    expires_at: u64,
    /// 下次允许轮询的时刻（Unix 秒，遵守 interval）
    next_poll_at: u64,
}

/// 句柄 → 授权会话。
static PENDING: LazyLock<Mutex<HashMap<String, PendingAuth>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 设备代码流的展示信息 + 轮询句柄。
#[derive(Debug, Clone, Serialize)]
pub struct DeviceAuthView {
    /// 不透明轮询句柄（本次授权会话 id）
    pub handle: String,
    /// 用户代码（授权页称「授权码」，需展示给用户）
    pub user_code: String,
    /// 授权页面 URL
    pub verification_uri: String,
    /// 已代入用户代码的授权页面 URL（直接打开即可）
    pub verification_uri_complete: String,
    /// 代码对有效期（秒）
    pub expires_in: u64,
    /// 建议轮询间隔（秒）
    pub interval: u64,
}

/// 轮询结果（回传前端；`status` 为 `pending` / `slow_down` 时继续轮询）。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PollOutcome {
    /// 用户尚未完成授权
    Pending,
    /// 轮询过快（后端已自动放慢），继续等待
    SlowDown,
    /// 授权成功，令牌已加密保存
    Authorized {
        /// 站点账号视图
        account: AccountView,
    },
    /// 用户拒绝授权（会话结束）
    Denied,
    /// 授权超时（会话结束）
    Expired,
    /// 失败（`message` 为「中文首行\n技术细节」）
    Failed {
        /// 可读错误消息
        message: String,
    },
}

/// 开始设备代码流授权：申请设备代码对，返回授权码与授权页 URL。
///
/// `device_code` 只留在后端内存（[`PENDING`]），前端拿到的只是不透明 `handle`。
pub async fn start_device_authorization() -> Result<DeviceAuthView, String> {
    let client_id = client_id()?;
    let body = form_encode(&[("client_id", client_id), ("scope", SCOPE)]);
    let resp = client()
        .post(format!("{}/oauth/device_code", constants::LITTLESKIN_OPEN))
        .header(reqwest::header::ACCEPT, "application/json")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("申请 LittleSkin 授权失败\n{e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    // 令牌端点响应不回显到错误消息里（同 refresh）
    let value: Value = serde_json::from_str(&text).map_err(|_| {
        format!("申请 LittleSkin 授权失败\nHTTP {status}：响应不是 JSON")
    })?;
    if value.get("error").and_then(Value::as_str).is_some() {
        let (kind, detail) = parse_oauth_error(&value);
        return Err(format!(
            "{}\n{}",
            kind.message(),
            detail_or_status(detail, status)
        ));
    }

    let user_code = required_str(&value, "user_code")
        .map_err(|e| format!("申请 LittleSkin 授权失败\n{e}"))?;
    let device_code = required_str(&value, "device_code")
        .map_err(|e| format!("申请 LittleSkin 授权失败\n{e}"))?;
    let verification_uri = required_str(&value, "verification_uri")
        .map_err(|e| format!("申请 LittleSkin 授权失败\n{e}"))?;
    // 服务器未提供 complete URL 时按 spec 的拼接方式兜底
    let verification_uri_complete = value
        .get("verification_uri_complete")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{verification_uri}?user_code={}", encode_component(&user_code)));
    let expires_in = value
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(300)
        .max(1);
    let interval = value
        .get("interval")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .max(1);

    let now = now_secs();
    let handle = uuid::Uuid::new_v4().to_string();
    {
        let mut map = PENDING.lock();
        // 顺手清理过期会话，避免句柄表随重试无限增长
        map.retain(|_, p| p.expires_at > now);
        map.insert(
            handle.clone(),
            PendingAuth {
                device_code,
                interval,
                expires_at: now.saturating_add(expires_in),
                next_poll_at: now,
            },
        );
    }
    Ok(DeviceAuthView {
        handle,
        user_code,
        verification_uri,
        verification_uri_complete,
        expires_in,
        interval,
    })
}

/// 轮询授权结果（前端按 `interval` 秒调用，直到 `status` 不再是 `pending` / `slow_down`）。
pub async fn poll_device_authorization(handle: &str) -> PollOutcome {
    let now = now_secs();
    let Some(pending) = ({ PENDING.lock().get(handle).cloned() }) else {
        return PollOutcome::Failed {
            message: "授权会话不存在或已结束，请重新发起授权\n启动器重启会丢弃进行中的设备代码流"
                .to_string(),
        };
    };
    if now >= pending.expires_at {
        PENDING.lock().remove(handle);
        return PollOutcome::Expired;
    }
    // 本地节流：未到下次允许的轮询时刻直接返回 pending（不发请求，避免触发 slow_down）
    {
        let mut map = PENDING.lock();
        match map.get_mut(handle) {
            Some(entry) if now < entry.next_poll_at => return PollOutcome::Pending,
            Some(entry) => entry.next_poll_at = now.saturating_add(entry.interval),
            None => {
                return PollOutcome::Failed {
                    message: "授权会话不存在或已结束，请重新发起授权\n会话已被清理".to_string(),
                }
            }
        }
    }

    let client_id = match client_id() {
        Ok(id) => id,
        Err(e) => return PollOutcome::Failed { message: e },
    };
    let body = form_encode(&[
        ("grant_type", DEVICE_CODE_GRANT),
        ("client_id", client_id),
        ("device_code", &pending.device_code),
    ]);
    let resp = match client()
        .post(format!("{}/oauth/token", constants::LITTLESKIN_OPEN))
        .header(reqwest::header::ACCEPT, "application/json")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .timeout(API_TIMEOUT)
        .send()
        .await
    {
        // 网络抖动不终止会话，让前端稍后重试
        Ok(resp) => resp,
        Err(e) => {
            return PollOutcome::Failed {
                message: format!("LittleSkin 授权轮询失败\n{e}"),
            }
        }
    };
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            // 令牌端点响应不回显到错误消息里（同 refresh）
            return PollOutcome::Failed {
                message: format!("LittleSkin 授权响应无法解析\nHTTP {status}：响应不是 JSON"),
            }
        }
    };

    if let Some(code) = value.get("error").and_then(Value::as_str) {
        let kind = OAuthError::from_code(code);
        let (_, detail) = parse_oauth_error(&value);
        return match kind {
            OAuthError::AuthorizationPending => PollOutcome::Pending,
            OAuthError::SlowDown => {
                // RFC 8628：收到 slow_down 时把轮询间隔至少增加 5 秒
                if let Some(entry) = PENDING.lock().get_mut(handle) {
                    entry.interval = entry.interval.saturating_add(5);
                }
                PollOutcome::SlowDown
            }
            OAuthError::AccessDenied => {
                PENDING.lock().remove(handle);
                PollOutcome::Denied
            }
            OAuthError::ExpiredToken => {
                PENDING.lock().remove(handle);
                PollOutcome::Expired
            }
            terminal => {
                if terminal.is_terminal() {
                    PENDING.lock().remove(handle);
                }
                PollOutcome::Failed {
                    message: format!(
                        "{}\n{}",
                        terminal.message(),
                        detail_or_status(detail, status)
                    ),
                }
            }
        };
    }

    let token = match parse_token_response(&value, now_secs()) {
        Ok(token) => token,
        Err(e) => {
            PENDING.lock().remove(handle);
            return PollOutcome::Failed { message: e };
        }
    };
    PENDING.lock().remove(handle);
    if let Err(e) = save_token(&token) {
        return PollOutcome::Failed { message: e };
    }
    // 身份信息尽力而为：拉取失败不改变「授权成功」这一事实
    let account = match fetch_account().await {
        Ok(view) => view,
        Err(_) => token.view(),
    };
    PollOutcome::Authorized { account }
}

// ---------------------------------------------------------------- 业务 API 错误

/// 站点 API 失败分类（**必须区分令牌失效 401 与权限不足 403**，提示不同）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// HTTP 401：访问令牌无效 / 过期 → 调用方可刷新后重试
    Unauthorized,
    /// 权限不足：HTTP 200 但 `body.code = 403`，或 HTTP 403
    Forbidden(String),
    /// 站点业务错误：HTTP 200 但 `body.code != 0`
    Business {
        /// 站点业务码
        code: i64,
        /// 站点消息
        message: String,
    },
    /// 速率限制（HTTP 429，60 次/分钟）
    RateLimited(Option<u64>),
    /// HTTP 4xx / 5xx
    Http {
        /// HTTP 状态码
        status: u16,
        /// 站点消息或响应片段
        detail: String,
    },
    /// 网络层失败
    Network(String),
    /// 响应无法解析
    Decode(String),
}

impl ApiError {
    /// 面向用户的中文短文案（首行）+ 技术细节。
    pub fn to_message(&self) -> String {
        match self {
            Self::Unauthorized => {
                "LittleSkin 授权已失效，请重新授权\nHTTP 401：访问令牌无效或已过期".to_string()
            }
            Self::Forbidden(detail) => format!(
                "当前授权缺少 LittleSkin 权限，请重新授权并授予角色与衣柜读写权限\n\
                 HTTP 200 code=403: {detail}"
            ),
            Self::Business { code, message } => {
                format!("LittleSkin 拒绝了该操作\ncode={code}: {message}")
            }
            Self::RateLimited(retry) => match retry {
                Some(secs) => format!(
                    "LittleSkin 请求过于频繁，请稍后再试\n已触发速率限制（60 次/分钟），需等待 {secs} 秒"
                ),
                None => "LittleSkin 请求过于频繁，请稍后再试\n已触发速率限制（60 次/分钟）"
                    .to_string(),
            },
            Self::Http { status, detail } => {
                let hint = if (300..400).contains(status) {
                    "（站点返回重定向，通常表示未授权）"
                } else {
                    ""
                };
                format!("LittleSkin 请求失败\nHTTP {status}{hint}: {detail}")
            }
            Self::Network(e) => format!("无法连接 LittleSkin\n{e}"),
            Self::Decode(d) => format!("LittleSkin 响应无法解析\n{d}"),
        }
    }
}

/// Blessing Skin 响应信封分类（纯函数）。
///
/// 站点约定：成功 `{"code":0,…}`；业务失败 `code != 0`；
/// **权限不足 = HTTP 200 但 `code = 403`**；非信封响应按 HTTP 状态判定。
fn classify(status: u16, body: &Value) -> Result<(), ApiError> {
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match json_i64(body.get("code")) {
        Some(0) => Ok(()),
        Some(403) => Err(ApiError::Forbidden(message)),
        Some(code) => Err(ApiError::Business { code, message }),
        None => {
            if (200..300).contains(&status) {
                Ok(())
            } else if status == 403 {
                Err(ApiError::Forbidden(message))
            } else {
                Err(ApiError::Http {
                    status,
                    detail: message,
                })
            }
        }
    }
}

/// 发送站点 API 请求：附带 `Accept` / 中文语言头，解析并分类响应信封。
async fn send(req: reqwest::RequestBuilder) -> Result<Value, ApiError> {
    let resp = req
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::ACCEPT_LANGUAGE, "zh-CN")
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ApiError::Unauthorized);
    }
    let code = status.as_u16();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        return Err(ApiError::RateLimited(retry));
    }
    let text = resp.text().await.unwrap_or_default();
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| ApiError::Decode(format!("HTTP {code} {}", truncate(&text))))?;
    classify(code, &value)?;
    Ok(value)
}

/// 发送请求并只关心成败（无响应体的写操作）。
async fn send_ok(req: reqwest::RequestBuilder) -> Result<(), ApiError> {
    send(req).await.map(|_| ())
}

/// 执行一次带授权的调用：取令牌 → 调用 → 遇 401 强制刷新令牌后重试一次。
async fn authed<T, F, Fut>(call: F) -> Result<T, String>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<T, ApiError>>,
{
    let token = access_token().await?;
    match call(token).await {
        Err(ApiError::Unauthorized) => {
            let token = force_refresh().await?;
            call(token).await.map_err(|e| e.to_message())
        }
        result => result.map_err(|e| e.to_message()),
    }
}

// ---------------------------------------------------------------- URL / 请求体构造（纯函数）

/// `{site}/api/user`
fn user_url() -> String {
    format!("{}/api/user", constants::LITTLESKIN_SITE)
}

/// `{site}/api/players`
fn players_url() -> String {
    format!("{}/api/players", constants::LITTLESKIN_SITE)
}

/// `{site}/api/players/{pid}/textures`
fn player_textures_url(pid: i64) -> String {
    format!("{}/api/players/{pid}/textures", constants::LITTLESKIN_SITE)
}

/// `{site}/api/closet`
fn closet_base() -> String {
    format!("{}/api/closet", constants::LITTLESKIN_SITE)
}

/// `{site}/api/closet/{tid}`
fn closet_item_url(tid: i64) -> String {
    format!("{}/{}", closet_base(), tid)
}

/// `{site}/api/closet?category=&page=`
fn closet_url(category: ClosetCategory, page: u32) -> String {
    format!(
        "{}?category={}&page={}",
        closet_base(),
        category.as_str(),
        page.max(1)
    )
}

/// 应用材质请求体（纯函数）：只放入非空字段（站点要求至少一个）。
fn texture_body(skin: Option<i64>, cape: Option<i64>) -> Value {
    let mut map = Map::new();
    if let Some(tid) = skin {
        map.insert("skin".to_string(), Value::from(tid));
    }
    if let Some(tid) = cape {
        map.insert("cape".to_string(), Value::from(tid));
    }
    Value::Object(map)
}

/// 重置材质的查询串（纯函数）：站点文档的 `skin` / `cape` 参数形式（值任意）。
fn reset_query(skin: bool, cape: bool) -> String {
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    if skin {
        pairs.push(("skin", "1"));
    }
    if cape {
        pairs.push(("cape", "1"));
    }
    form_encode(&pairs)
}

// ---------------------------------------------------------------- 业务数据结构

/// 站点用户（`GET /api/user` 子集）。
#[derive(Debug, Clone, Serialize)]
pub struct SiteUser {
    /// 站点 UID
    pub uid: i64,
    /// 昵称
    pub nickname: String,
    /// 权限等级：-1 封禁 / 0 普通 / 1 管理员 / 2 超管
    pub permission: i64,
    /// 是否通过邮箱验证
    pub verified: bool,
}

/// 站点角色（`GET /api/players` 条目）。
#[derive(Debug, Clone, Serialize)]
pub struct SitePlayer {
    /// 角色 ID
    pub pid: i64,
    /// 角色名
    pub name: String,
    /// 已应用皮肤 TID（0 = 未设置）
    pub tid_skin: i64,
    /// 已应用披风 TID（0 = 未设置）
    pub tid_cape: i64,
    /// 最后修改时间（站点字符串）
    pub last_modified: String,
}

/// 衣柜分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosetCategory {
    /// 皮肤（站点查询值 `skin`，匹配材质 type `steve` / `alex`）
    Skin,
    /// 披风（站点查询值 `cape`）
    Cape,
}

impl ClosetCategory {
    /// 解析分类字符串（大小写不敏感，仅接受 `skin` / `cape`）。
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "skin" => Ok(Self::Skin),
            "cape" => Ok(Self::Cape),
            other => Err(format!("未知的衣柜分类：{other}\n只支持 skin / cape")),
        }
    }

    /// 站点查询值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::Cape => "cape",
        }
    }
}

/// 衣柜条目（`GET /api/closet` 的 `data` 元素）。
#[derive(Debug, Clone, Serialize)]
pub struct ClosetItem {
    /// 材质 TID
    pub tid: i64,
    /// 材质自身名称
    pub name: String,
    /// 材质类型：steve / alex / cape
    pub kind: String,
    /// 材质文件 hash
    pub hash: String,
    /// 文件大小（字节）
    pub size: i64,
    /// 材质是否公开
    pub is_public: bool,
    /// 上传时间（站点字符串）
    pub upload_at: String,
    /// 点赞数
    pub likes: i64,
    /// 衣柜中的条目名（`pivot.item_name`）
    pub item_name: String,
    /// 站点公开预览图 URL（`GET /preview/{tid}`，无需鉴权）
    pub preview_url: String,
}

/// 衣柜分页（`GET /api/closet`）。
#[derive(Debug, Clone, Serialize)]
pub struct ClosetPage {
    /// 当前页
    pub page: u32,
    /// 每页条数
    pub per_page: u32,
    /// 总条数
    pub total: u32,
    /// 总页数
    pub last_page: u32,
    /// 条目
    pub items: Vec<ClosetItem>,
}

// ---------------------------------------------------------------- 响应解析（纯函数）

/// 解析 `GET /api/user`。
fn parse_user(value: &Value) -> Result<SiteUser, String> {
    let uid = json_i64(value.get("uid")).ok_or_else(|| "用户响应缺少 uid".to_string())?;
    Ok(SiteUser {
        uid,
        nickname: value
            .get("nickname")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        permission: json_i64(value.get("permission")).unwrap_or(0),
        verified: value
            .get("verified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// 解析 `GET /api/players`（响应为数组，无信封）。
fn parse_players(value: &Value) -> Result<Vec<SitePlayer>, String> {
    let array = value
        .as_array()
        .ok_or_else(|| format!("角色列表响应不是数组\n{}", truncate(&value.to_string())))?;
    array
        .iter()
        .map(|item| {
            let pid =
                json_i64(item.get("pid")).ok_or_else(|| "角色缺少 pid".to_string())?;
            let name = item
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| "角色缺少 name".to_string())?
                .to_string();
            Ok(SitePlayer {
                pid,
                name,
                tid_skin: json_i64(item.get("tid_skin")).unwrap_or(0),
                tid_cape: json_i64(item.get("tid_cape")).unwrap_or(0),
                last_modified: item
                    .get("last_modified")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

/// 解析单个衣柜条目。
fn parse_closet_item(value: &Value) -> Result<ClosetItem, String> {
    let tid = json_i64(value.get("tid")).ok_or_else(|| "衣柜条目缺少 tid".to_string())?;
    Ok(ClosetItem {
        tid,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind: value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        hash: value
            .get("hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        size: json_i64(value.get("size")).unwrap_or(0),
        is_public: value
            .get("public")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        upload_at: value
            .get("upload_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        likes: json_i64(value.get("likes")).unwrap_or(0),
        item_name: value
            .get("pivot")
            .and_then(|pivot| pivot.get("item_name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        preview_url: format!("{}/preview/{}", constants::LITTLESKIN_SITE, tid),
    })
}

/// 解析 `GET /api/closet` 分页响应。
fn parse_closet_page(value: &Value) -> Result<ClosetPage, String> {
    let page = json_i64(value.get("current_page")).unwrap_or(1).max(1) as u32;
    let array = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "衣柜响应缺少 data 数组".to_string())?;
    let items = array
        .iter()
        .map(parse_closet_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ClosetPage {
        page,
        per_page: json_i64(value.get("per_page")).unwrap_or(0).max(0) as u32,
        total: json_i64(value.get("total")).unwrap_or(0).max(0) as u32,
        last_page: json_i64(value.get("last_page"))
            .unwrap_or(i64::from(page))
            .max(1) as u32,
        items,
    })
}

// ---------------------------------------------------------------- 业务 API

/// `GET /api/user`：站点用户信息（同时用于校验令牌可用性与取 UID）。
pub async fn fetch_user() -> Result<SiteUser, String> {
    authed(move |token| async move {
        let value = send(client().get(user_url()).bearer_auth(token)).await?;
        parse_user(&value).map_err(ApiError::Decode)
    })
    .await
}

/// 拉取站点账号信息并组装为对外视图（联网）。
pub async fn fetch_account() -> Result<AccountView, String> {
    let user = fetch_user().await?;
    Ok(AccountView {
        uid: Some(user.uid),
        nickname: Some(user.nickname),
        expires_at: load_token().map(|t| t.expires_at).unwrap_or(0),
    })
}

/// `GET /api/players`：角色列表。
pub async fn fetch_players() -> Result<Vec<SitePlayer>, String> {
    authed(move |token| async move {
        let value = send(client().get(players_url()).bearer_auth(token)).await?;
        parse_players(&value).map_err(ApiError::Decode)
    })
    .await
}

/// `PUT /api/players/{pid}/textures`：应用材质（`skin` / `cape` 至少一个）。
///
/// ⚠️ 站点要求 TID **已在用户衣柜中**（`PlayerController@setTexture`），
/// 未加入衣柜的材质会返回业务错误；请先 [`add_closet_item`]。
pub async fn apply_textures(pid: i64, skin: Option<i64>, cape: Option<i64>) -> Result<(), String> {
    let pid = ensure_positive_id(pid, "角色 ID")?;
    if skin.is_none() && cape.is_none() {
        return Err("请至少选择一个材质（皮肤或披风）\nskin / cape 不能同时为空".to_string());
    }
    if let Some(tid) = skin {
        ensure_positive_id(tid, "皮肤 TID")?;
    }
    if let Some(tid) = cape {
        ensure_positive_id(tid, "披风 TID")?;
    }
    let body = texture_body(skin, cape);
    authed(move |token| {
        let body = body.clone();
        async move {
            send_ok(
                client()
                    .put(player_textures_url(pid))
                    .bearer_auth(token)
                    .json(&body),
            )
            .await
        }
    })
    .await
}

/// `DELETE /api/players/{pid}/textures`：重置材质（`skin` / `cape` 至少一个为 true）。
pub async fn reset_textures(pid: i64, skin: bool, cape: bool) -> Result<(), String> {
    let pid = ensure_positive_id(pid, "角色 ID")?;
    if !skin && !cape {
        return Err("请至少选择一个要重置的材质（皮肤或披风）\nskin / cape 不能同时为 false"
            .to_string());
    }
    let query = reset_query(skin, cape);
    authed(move |token| {
        let query = query.clone();
        async move {
            send_ok(
                client()
                    .delete(format!("{}?{query}", player_textures_url(pid)))
                    .bearer_auth(token),
            )
            .await
        }
    })
    .await
}

/// `GET /api/closet?category=&page=`：衣柜列表（皮肤 / 披风分类）。
pub async fn fetch_closet(category: ClosetCategory, page: u32) -> Result<ClosetPage, String> {
    authed(move |token| async move {
        let value = send(client().get(closet_url(category, page)).bearer_auth(token)).await?;
        parse_closet_page(&value).map_err(ApiError::Decode)
    })
    .await
}

/// `POST /api/closet`：把**站点上已存在**的材质加入衣柜。
///
/// 站点 API 没有材质上传端点（见模块注释），`tid` 只能来自：
/// 用户自己在站点网页上传的材质，或任意公开材质（材质页 URL 末尾数字）。
pub async fn add_closet_item(tid: i64, name: &str) -> Result<(), String> {
    let tid = ensure_positive_id(tid, "材质 TID")?;
    let name = ensure_name(name)?;
    let body = json!({ "tid": tid, "name": name });
    authed(move |token| {
        let body = body.clone();
        async move {
            send_ok(
                client()
                    .post(closet_base())
                    .bearer_auth(token)
                    .json(&body),
            )
            .await
        }
    })
    .await
}

/// `PUT /api/closet/{tid}`：重命名衣柜条目。
pub async fn rename_closet_item(tid: i64, name: &str) -> Result<(), String> {
    let tid = ensure_positive_id(tid, "材质 TID")?;
    let name = ensure_name(name)?;
    let body = json!({ "name": name });
    authed(move |token| {
        let body = body.clone();
        async move {
            send_ok(
                client()
                    .put(closet_item_url(tid))
                    .bearer_auth(token)
                    .json(&body),
            )
            .await
        }
    })
    .await
}

/// `DELETE /api/closet/{tid}`：从衣柜移除条目。
pub async fn remove_closet_item(tid: i64) -> Result<(), String> {
    let tid = ensure_positive_id(tid, "材质 TID")?;
    authed(move |token| async move {
        send_ok(
            client()
                .delete(closet_item_url(tid))
                .bearer_auth(token),
        )
        .await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------ 表单 / URL 构造

    /// urlencoded 分量编码：保留 unreserved，空格 → `+`，其余 → 大写 %XX。
    #[test]
    fn encode_component_follows_urlencoded() {
        assert_eq!(encode_component("User.Read"), "User.Read");
        assert_eq!(encode_component("a b"), "a+b");
        assert_eq!(encode_component("a&b=c"), "a%26b%3Dc");
        assert_eq!(encode_component("50%"), "50%25");
        assert_eq!(encode_component("-._~"), "-._~");
        // 中文按 UTF-8 字节百分号编码（「皮」= E7 9A AE）
        assert_eq!(encode_component("皮"), "%E7%9A%AE");
        // 换行等控制字符也必须编码，避免注入表单分隔符
        assert_eq!(encode_component("a\nb"), "a%0Ab");
    }

    /// 表单体构造：`k=v` 以 `&` 连接，键值都编码。
    #[test]
    fn form_encode_joins_pairs() {
        assert_eq!(form_encode(&[]), "");
        assert_eq!(
            form_encode(&[("client_id", "42"), ("scope", "User.Read Closet.ReadWrite")]),
            "client_id=42&scope=User.Read+Closet.ReadWrite"
        );
        assert_eq!(form_encode(&[("grant_type", "refresh_token")]), "grant_type=refresh_token");
    }

    /// 申请权限必须覆盖业务 API 所需的三个 scope，且申请刷新令牌。
    #[test]
    fn scope_covers_required_permissions() {
        for needed in ["User.Read", "Player.ReadWrite", "Closet.ReadWrite", "offline_access"] {
            assert!(SCOPE.split(' ').any(|s| s == needed), "scope 缺少 {needed}");
        }
        // 不需要 openid（不解码令牌）与 Yggdrasil 权限（互斥且与本功能无关）
        assert!(!SCOPE.contains("openid"));
        assert!(!SCOPE.contains("Yggdrasil"));
    }

    /// 客户端 ID 校验：已配置则可用，未配置返回可读错误（绝不 panic）。
    #[test]
    fn client_id_is_configured_or_reported() {
        match client_id() {
            Ok(id) => assert!(!id.trim().is_empty(), "客户端 ID 不应为空串"),
            Err(message) => {
                assert!(message.starts_with("未配置 LittleSkin 客户端 ID"), "{message}");
                assert!(message.contains('\n'), "错误应为首行文案 + 技术细节");
            }
        }
    }

    /// 站点 URL 构造（路径与查询串需与文档一致）。
    #[test]
    fn api_urls_match_documentation() {
        let site = constants::LITTLESKIN_SITE;
        assert_eq!(user_url(), format!("{site}/api/user"));
        assert_eq!(players_url(), format!("{site}/api/players"));
        assert_eq!(
            player_textures_url(7),
            format!("{site}/api/players/7/textures")
        );
        assert_eq!(closet_base(), format!("{site}/api/closet"));
        assert_eq!(closet_item_url(12), format!("{site}/api/closet/12"));
        assert_eq!(
            closet_url(ClosetCategory::Skin, 1),
            format!("{site}/api/closet?category=skin&page=1")
        );
        assert_eq!(
            closet_url(ClosetCategory::Cape, 3),
            format!("{site}/api/closet?category=cape&page=3")
        );
        // 页码 0 归一为 1（站点分页从 1 开始）
        assert!(closet_url(ClosetCategory::Skin, 0).ends_with("page=1"));
    }

    /// 应用材质请求体：只含传入的字段。
    #[test]
    fn texture_body_only_contains_given_fields() {
        assert_eq!(texture_body(Some(3), None), json!({ "skin": 3 }));
        assert_eq!(texture_body(None, Some(9)), json!({ "cape": 9 }));
        assert_eq!(texture_body(Some(3), Some(9)), json!({ "skin": 3, "cape": 9 }));
        assert_eq!(texture_body(None, None), json!({}));
    }

    /// 重置材质查询串。
    #[test]
    fn reset_query_uses_documented_params() {
        assert_eq!(reset_query(true, false), "skin=1");
        assert_eq!(reset_query(false, true), "cape=1");
        assert_eq!(reset_query(true, true), "skin=1&cape=1");
        assert_eq!(reset_query(false, false), "");
    }

    // ------------------------------------------------ 入参校验

    /// pid / tid 必须是正整数（拼进 URL 路径，防注入）。
    #[test]
    fn ids_must_be_positive() {
        assert_eq!(ensure_positive_id(1, "角色 ID").unwrap(), 1);
        assert!(ensure_positive_id(0, "角色 ID").is_err());
        assert!(ensure_positive_id(-5, "材质 TID").is_err());
        let err = ensure_positive_id(-5, "材质 TID").unwrap_err();
        assert!(err.starts_with("材质 TID 非法"), "{err}");
    }

    /// 衣柜名称去除首尾空白且不允许为空。
    #[test]
    fn closet_name_is_trimmed_and_required() {
        assert_eq!(ensure_name("  我的皮肤 ").unwrap(), "我的皮肤");
        assert!(ensure_name("   ").is_err());
    }

    /// 衣柜分类解析：大小写不敏感，非法值报可读错误。
    #[test]
    fn closet_category_parsing() {
        assert_eq!(ClosetCategory::parse("skin").unwrap(), ClosetCategory::Skin);
        assert_eq!(ClosetCategory::parse(" CAPE ").unwrap(), ClosetCategory::Cape);
        assert_eq!(ClosetCategory::Skin.as_str(), "skin");
        assert_eq!(ClosetCategory::Cape.as_str(), "cape");
        let err = ClosetCategory::parse("hat").unwrap_err();
        assert!(err.starts_with("未知的衣柜分类"), "{err}");
    }

    /// 错误细节截断：短文本原样，长文本截断且按字符边界（中文安全）。
    #[test]
    fn truncate_is_char_safe() {
        assert_eq!(truncate("  hi  "), "hi");
        let long: String = "中".repeat(500);
        let cut = truncate(&long);
        assert!(cut.chars().count() <= 201, "截断后长度应受限");
        assert!(cut.ends_with('…'));
    }

    // ------------------------------------------------ OAuth 错误分类

    /// `error` 取值 → 分类；只有 pending / slow_down 需要继续轮询。
    #[test]
    fn oauth_error_classification() {
        assert_eq!(
            OAuthError::from_code("authorization_pending"),
            OAuthError::AuthorizationPending
        );
        assert_eq!(OAuthError::from_code("slow_down"), OAuthError::SlowDown);
        assert_eq!(OAuthError::from_code("access_denied"), OAuthError::AccessDenied);
        assert_eq!(OAuthError::from_code("expired_token"), OAuthError::ExpiredToken);
        assert_eq!(OAuthError::from_code("invalid_client"), OAuthError::InvalidClient);
        assert_eq!(OAuthError::from_code("invalid_scope"), OAuthError::InvalidScope);
        assert_eq!(OAuthError::from_code("invalid_grant"), OAuthError::InvalidGrant);
        assert_eq!(OAuthError::from_code("teapot"), OAuthError::Other);

        assert!(!OAuthError::AuthorizationPending.is_terminal());
        assert!(!OAuthError::SlowDown.is_terminal());
        for terminal in [
            OAuthError::AccessDenied,
            OAuthError::ExpiredToken,
            OAuthError::InvalidClient,
            OAuthError::InvalidRequest,
            OAuthError::InvalidScope,
            OAuthError::InvalidGrant,
            OAuthError::Other,
        ] {
            assert!(terminal.is_terminal(), "{terminal:?} 应为终态");
        }
    }

    /// 未加白名单（invalid_client）必须给出可操作的提示，而不是技术黑话。
    #[test]
    fn invalid_client_message_is_actionable() {
        let message = OAuthError::InvalidClient.message();
        assert!(message.starts_with("应用未通过 LittleSkin 设备代码流白名单审核"));
        assert!(message.contains("open.littleskin.cn/oauth/callback"));
        assert!(message.contains("邮件工单"));
    }

    /// 错误响应解析：JSON 取 error_description，非 JSON 退回原文。
    #[test]
    fn error_body_parsing() {
        let body = json!({
            "error": "invalid_client",
            "error_description": "Client was not found or not whitelisted"
        });
        let (kind, detail) = parse_oauth_error(&body);
        assert_eq!(kind, OAuthError::InvalidClient);
        assert_eq!(detail, "Client was not found or not whitelisted");

        // 无 description → 退回 error 码
        let (kind, detail) = parse_oauth_error(&json!({ "error": "slow_down" }));
        assert_eq!(kind, OAuthError::SlowDown);
        assert_eq!(detail, "slow_down");
    }

    /// HTTP 状态与错误细节的组合格式。
    #[test]
    fn detail_includes_status() {
        assert_eq!(
            detail_or_status(String::new(), reqwest::StatusCode::BAD_REQUEST),
            "HTTP 400 Bad Request"
        );
        assert_eq!(
            detail_or_status("boom".to_string(), reqwest::StatusCode::BAD_REQUEST),
            "HTTP 400 Bad Request: boom"
        );
    }

    // ------------------------------------------------ 令牌解析与过期判定

    /// 设备代码流成功响应 → 令牌（含刷新令牌，过期时刻按 expires_in 推算）。
    #[test]
    fn parse_token_response_full() {
        let value = json!({
            "token_type": "Bearer",
            "expires_in": 259200,
            "access_token": "at",
            "refresh_token": "rt",
            "id_token": "opaque.jwt.string"
        });
        let token = parse_token_response(&value, 1_000).unwrap();
        assert_eq!(token.access_token, "at");
        assert_eq!(token.refresh_token.as_deref(), Some("rt"));
        assert_eq!(token.expires_at, 1_000 + 259_200);
        assert_eq!(token.view().uid, None);
    }

    /// 未申请 offline_access 时无刷新令牌；缺 access_token 必须报错。
    #[test]
    fn parse_token_response_edge_cases() {
        let token = parse_token_response(&json!({ "access_token": "at" }), 5).unwrap();
        assert_eq!(token.refresh_token, None);
        assert_eq!(
            token.expires_at,
            5 + DEFAULT_EXPIRES_IN,
            "缺 expires_in 时用保守默认值，避免每次请求都触发刷新"
        );

        let err = parse_token_response(&json!({ "token_type": "Bearer" }), 0).unwrap_err();
        assert!(err.starts_with("授权响应缺少 access_token"), "{err}");
        let err = parse_token_response(&json!({ "access_token": "" }), 0).unwrap_err();
        assert!(err.starts_with("授权响应缺少 access_token"), "{err}");
    }

    /// 过期判定含提前刷新窗口：now + skew >= expires_at 即视为过期。
    #[test]
    fn expiry_respects_refresh_skew() {
        let skew = constants::LITTLESKIN_REFRESH_SKEW_SECS;
        let token = LsToken {
            access_token: "at".to_string(),
            refresh_token: None,
            expires_at: 10_000,
            uid: None,
            nickname: None,
        };
        assert!(!is_expired(&token, 10_000 - skew - 1));
        assert!(is_expired(&token, 10_000 - skew));
        assert!(is_expired(&token, 10_000));
        assert!(is_expired(&token, 99_999));
        // 时钟回拨到 0 时不应 panic（saturating）：提前窗口未覆盖到该时刻
        assert!(!is_expired(&token, 0));
    }

    /// 对外视图（回传前端）绝不能包含令牌字段。
    #[test]
    fn account_view_has_no_token_fields() {
        let token = LsToken {
            access_token: "access-secret".to_string(),
            refresh_token: Some("refresh-secret".to_string()),
            expires_at: 123,
            uid: Some(7),
            nickname: Some("小明".to_string()),
        };
        let json = serde_json::to_string(&token.view()).unwrap();
        assert!(!json.contains("access_token"));
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("access-secret"));
        assert!(!json.contains("refresh-secret"));
        assert!(json.contains("\"uid\":7"));
        assert!(json.contains("小明"));
    }

    /// 令牌可经 secrets 序列化往返（加密存储用的就是这条路径）。
    #[test]
    fn token_roundtrips_through_json() {
        let token = LsToken {
            access_token: "at".to_string(),
            refresh_token: Some("rt".to_string()),
            expires_at: 42,
            uid: Some(1),
            nickname: Some("n".to_string()),
        };
        let bytes = serde_json::to_vec(&token).unwrap();
        let back: LsToken = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.access_token, "at");
        assert_eq!(back.refresh_token.as_deref(), Some("rt"));
        assert_eq!(back.expires_at, 42);
    }

    // ------------------------------------------------ 响应信封分类

    /// 成功（code=0）、业务失败（code!=0）、权限不足（code=403）三类必须区分。
    #[test]
    fn classify_envelope() {
        assert_eq!(classify(200, &json!({ "code": 0, "message": "ok" })), Ok(()));
        assert_eq!(
            classify(200, &json!({ "code": 1, "message": "积分不足" })),
            Err(ApiError::Business {
                code: 1,
                message: "积分不足".to_string()
            })
        );
        assert_eq!(
            classify(200, &json!({ "code": 403, "message": "无权限" })),
            Err(ApiError::Forbidden("无权限".to_string()))
        );
        // 数组响应（角色列表）没有 code 字段：按 HTTP 状态判定
        assert_eq!(classify(200, &json!([])), Ok(()));
        assert_eq!(
            classify(500, &json!({ "message": "boom" })),
            Err(ApiError::Http {
                status: 500,
                detail: "boom".to_string()
            })
        );
        assert_eq!(
            classify(403, &json!({ "message": "forbidden" })),
            Err(ApiError::Forbidden("forbidden".to_string()))
        );
        // 站点未授权时会 302 到登录页
        assert!(matches!(classify(302, &json!({})), Err(ApiError::Http { status: 302, .. })));
    }

    /// 错误文案：首行中文提示 + 换行 + 技术细节；401 与 403 提示必须不同。
    #[test]
    fn api_error_messages_are_distinct_and_readable() {
        let unauthorized = ApiError::Unauthorized.to_message();
        let forbidden = ApiError::Forbidden("scope".to_string()).to_message();
        assert_ne!(unauthorized, forbidden);
        assert!(unauthorized.starts_with("LittleSkin 授权已失效"));
        assert!(forbidden.starts_with("当前授权缺少 LittleSkin 权限"));
        for message in [
            unauthorized,
            forbidden,
            ApiError::Business {
                code: 1,
                message: "积分不足".to_string(),
            }
            .to_message(),
            ApiError::RateLimited(Some(30)).to_message(),
            ApiError::RateLimited(None).to_message(),
            ApiError::Http {
                status: 302,
                detail: String::new(),
            }
            .to_message(),
            ApiError::Network("connection reset".to_string()).to_message(),
            ApiError::Decode("HTTP 200 <html>".to_string()).to_message(),
        ] {
            let mut lines = message.lines();
            assert!(lines.next().is_some_and(|l| !l.is_empty()), "{message}");
            assert!(message.contains('\n'), "错误应包含技术细节行: {message}");
        }
    }

    // ------------------------------------------------ 响应解析

    /// 用户信息解析（容忍字符串形式的数字）。
    #[test]
    fn parse_user_response() {
        let user = parse_user(&json!({
            "uid": 2,
            "nickname": "steve",
            "permission": 0,
            "verified": true
        }))
        .unwrap();
        assert_eq!(user.uid, 2);
        assert_eq!(user.nickname, "steve");
        assert_eq!(user.permission, 0);
        assert!(user.verified);

        let user = parse_user(&json!({ "uid": "9" })).unwrap();
        assert_eq!(user.uid, 9);
        assert_eq!(user.nickname, "");
        assert!(!user.verified);

        assert!(parse_user(&json!({})).is_err());
    }

    /// 角色列表解析：数组直出，缺失字段回退默认值。
    #[test]
    fn parse_players_response() {
        let players = parse_players(&json!([
            { "pid": 1, "uid": 1, "name": "a", "tid_skin": 4, "tid_cape": 0,
              "last_modified": "2020-01-01 00:00:00" },
            { "pid": "2", "name": "b" }
        ]))
        .unwrap();
        assert_eq!(players.len(), 2);
        assert_eq!(players[0].tid_skin, 4);
        assert_eq!(players[0].last_modified, "2020-01-01 00:00:00");
        assert_eq!(players[1].pid, 2);
        assert_eq!(players[1].tid_cape, 0);

        // 非数组响应必须报错（例如站点返回错误信封）
        assert!(parse_players(&json!({ "code": 403 })).is_err());
        // 元素缺 pid 必须报错
        assert!(parse_players(&json!([{ "name": "x" }])).is_err());
    }

    /// 衣柜分页解析：分页元信息 + `pivot.item_name` + 预览图 URL。
    #[test]
    fn parse_closet_page_response() {
        let page = parse_closet_page(&json!({
            "per_page": 6,
            "total": 7,
            "current_page": 2,
            "last_page": 2,
            "data": [
                {
                    "tid": 11,
                    "name": "default",
                    "type": "steve",
                    "hash": "abc",
                    "size": 5,
                    "uploader": 2,
                    "public": true,
                    "upload_at": "2020-01-01 00:00:00",
                    "likes": 2,
                    "pivot": { "item_name": "我的皮肤" }
                },
                { "tid": 12, "type": "cape" }
            ]
        }))
        .unwrap();
        assert_eq!(page.page, 2);
        assert_eq!(page.per_page, 6);
        assert_eq!(page.total, 7);
        assert_eq!(page.last_page, 2);
        assert_eq!(page.items.len(), 2);
        let first = &page.items[0];
        assert_eq!(first.tid, 11);
        assert_eq!(first.kind, "steve");
        assert_eq!(first.item_name, "我的皮肤");
        assert!(first.is_public);
        assert_eq!(first.likes, 2);
        assert_eq!(
            first.preview_url,
            format!("{}/preview/11", constants::LITTLESKIN_SITE)
        );
        // 缺 pivot / 缺字段的条目不应 panic，名称回落空串
        assert_eq!(page.items[1].item_name, "");
        assert_eq!(page.items[1].size, 0);
        assert!(!page.items[1].is_public);

        // 缺 data 数组必须报错
        assert!(parse_closet_page(&json!({ "total": 0 })).is_err());
        // 条目缺 tid 必须报错
        assert!(parse_closet_page(&json!({ "data": [{ "name": "x" }] })).is_err());
    }

    /// 分页响应缺省值：页码归一为 1。
    #[test]
    fn parse_closet_page_defaults() {
        let page = parse_closet_page(&json!({ "data": [] })).unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.last_page, 1);
        assert_eq!(page.total, 0);
        assert!(page.items.is_empty());
    }

    /// 数字容忍：字符串数字也能解析（部分实现/代理会改变类型）。
    #[test]
    fn json_i64_tolerates_strings() {
        assert_eq!(json_i64(Some(&json!(3))), Some(3));
        assert_eq!(json_i64(Some(&json!("3"))), Some(3));
        assert_eq!(json_i64(Some(&json!("x"))), None);
        assert_eq!(json_i64(Some(&json!(null))), None);
        assert_eq!(json_i64(None), None);
    }

    /// 轮询结果 JSON 形状（前端按 status 分支）。
    #[test]
    fn poll_outcome_shape() {
        assert_eq!(
            serde_json::to_string(&PollOutcome::Pending).unwrap(),
            "{\"status\":\"pending\"}"
        );
        assert_eq!(
            serde_json::to_string(&PollOutcome::SlowDown).unwrap(),
            "{\"status\":\"slow_down\"}"
        );
        assert_eq!(
            serde_json::to_string(&PollOutcome::Denied).unwrap(),
            "{\"status\":\"denied\"}"
        );
        assert_eq!(
            serde_json::to_string(&PollOutcome::Expired).unwrap(),
            "{\"status\":\"expired\"}"
        );
        let json = serde_json::to_string(&PollOutcome::Authorized {
            account: AccountView {
                uid: Some(1),
                nickname: Some("n".to_string()),
                expires_at: 9,
            },
        })
        .unwrap();
        assert!(json.starts_with("{\"status\":\"authorized\""), "{json}");
        assert!(!json.contains("access_token"));

        let json = serde_json::to_string(&PollOutcome::Failed {
            message: "boom\nHTTP 400".to_string(),
        })
        .unwrap();
        assert!(json.contains("\"status\":\"failed\""));
        assert!(json.contains("boom"));
    }

    /// 设备代码流响应字段缺失时给出可读错误（不 panic）。
    #[test]
    fn required_str_reports_missing_field() {
        assert_eq!(required_str(&json!({ "a": "v" }), "a").unwrap(), "v");
        assert!(required_str(&json!({ "a": "" }), "a").is_err());
        let err = required_str(&json!({}), "user_code").unwrap_err();
        assert!(err.contains("user_code"), "{err}");
    }
}
