//! 账号与皮肤命令：逻辑在 auth/。

use std::sync::Arc;

use base64::Engine;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::auth::account;
use crate::auth::external;
use crate::auth::microsoft;
use crate::auth::model;
use crate::auth::mojang;
use crate::constants;
use crate::persist;
use crate::task::EventSink;

/// 进行中的登录回调任务（取消时 abort 释放回调端口）。
static LOGIN_TASK: std::sync::LazyLock<parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// 启动 Microsoft 登录，返回授权 URL；结果经 `ms-login-complete` 事件推送。
#[tauri::command]
pub async fn start_ms_login(app: AppHandle) -> Result<String, String> {
    // 释放端口，保证可重入
    if let Some(h) = LOGIN_TASK.lock().take() {
        h.abort();
    }

    let state = microsoft::generate_state().map_err(|e| e.to_string())?;
    let pkce = microsoft::generate_pkce().map_err(|e| e.to_string())?;

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", microsoft::CALLBACK_PORT))
        .await
        .map_err(common_err_ctx_key!("errors.login.failed", 
            "回调端口 {} 被占用",
            microsoft::CALLBACK_PORT
        ))?;

    let auth_url = microsoft::build_auth_url(&state, &pkce).map_err(|e| e.to_string())?;

    // 含超时；完成后端口自动释放
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    let handle = tokio::task::spawn(async move {
        account::wait_callback(sink, listener, &state, &pkce).await;
    });
    *LOGIN_TASK.lock() = Some(handle);

    app.opener()
        .open_url(auth_url.clone(), None::<&str>)
        .map_err(common_err_ctx_key!("errors.login.failed", "打开浏览器失败"))?;

    Ok(auth_url)
}

#[tauri::command]
pub fn cancel_ms_login() {
    if let Some(h) = LOGIN_TASK.lock().take() {
        h.abort();
    }
}

#[tauri::command]
pub fn get_ms_account() -> Option<model::MsAccount> {
    model::read()
}

/// 读取全部已保存的正版账号（登录时间升序，末尾为当前账号）。
#[tauri::command]
pub fn get_ms_accounts() -> Vec<model::MsAccount> {
    model::read_all()
}

/// 移除指定 uuid 账号（连同 refresh_token）；删除当前账号后回退上一账号。
#[tauri::command]
pub fn remove_ms_account(uuid: String) -> Result<(), String> {
    model::remove(&uuid)
}

/// 切换当前正版账号：静默刷新后置为当前；失效返回错误。
#[tauri::command]
pub async fn switch_ms_account(uuid: String) -> Result<model::MsAccount, String> {
    let account = model::read_all()
        .into_iter()
        .find(|a| a.uuid == uuid)
        .ok_or("该账号不在列表中，请重新登录")?;
    let refreshed = microsoft::refresh_login(&account)
        .await
        .map_err(|e| e.to_string())?;
    model::save(&refreshed)?;
    Ok(refreshed)
}

/// 内置默认皮肤（Steve / Alex）base64：编译期内置，不依赖已下载版本。
#[tauri::command]
pub fn fetch_default_skin(skin: String) -> Result<String, String> {
    let data: &[u8] = match skin.as_str() {
        "steve" => include_bytes!("../../resources/steve.png"),
        "alex" => include_bytes!("../../resources/alex.png"),
        _ => return Err("未知皮肤类型".into()),
    };
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

/// 头像用皮肤 PNG（base64）：官方失败自动回退 minotar。
async fn fetch_skin_for_avatar(uuid: String) -> Result<String, String> {
    let (url, _model) = mojang::skin_profile_with_fallback(&uuid).await?;
    account::fetch_b64(&url, "下载皮肤").await
}

/// 保存用皮肤 PNG（base64）：仅官方直连，失败如实报错。
async fn fetch_skin_official(uuid: String) -> Result<String, String> {
    let textures = mojang::profile_textures(&uuid).await?;
    let skin_url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    account::fetch_b64(skin_url, "下载皮肤").await
}

#[tauri::command]
pub async fn fetch_account_skin(uuid: String) -> Result<String, String> {
    fetch_skin_for_avatar(uuid).await
}

#[derive(serde::Serialize)]
pub struct SkinProfile {
    skin: String,
    /// 模型类型："classic"（Steve 宽）或 "slim"（Alex 窄）。
    model: String,
}

/// 正版皮肤 PNG + 模型类型，一次 IPC 供 3D 头像渲染；官方失败自动回退 minotar。
#[tauri::command]
pub async fn fetch_account_skin_profile(uuid: String) -> Result<SkinProfile, String> {
    let (url, model) = mojang::skin_profile_with_fallback(&uuid).await?;
    let b64 = account::fetch_b64(&url, "下载皮肤").await?;
    Ok(SkinProfile { skin: b64, model })
}

#[tauri::command]
pub async fn fetch_external_skin(api_url: String, uuid: String) -> Result<String, String> {
    let (url, _model) = external::external_skin_info(&api_url, &uuid).await?;
    account::fetch_b64(&url, "下载皮肤").await
}

#[tauri::command]
pub async fn fetch_external_skin_profile(
    api_url: String,
    uuid: String,
) -> Result<SkinProfile, String> {
    let (url, model) = external::external_skin_info(&api_url, &uuid).await?;
    let b64 = account::fetch_b64(&url, "下载皮肤").await?;
    Ok(SkinProfile { skin: b64, model })
}

async fn write_b64_file(b64: &str, path: &str) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(common_err_ctx_key!("errors.skin.load", "皮肤数据解码失败"))?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(common_err_ctx_key!("errors.file.write", "写入文件失败"))
}

#[tauri::command]
pub async fn save_external_skin(api_url: String, uuid: String, path: String) -> Result<(), String> {
    let b64 = fetch_external_skin(api_url, uuid).await?;
    write_b64_file(&b64, &path).await
}

#[tauri::command]
pub async fn save_skin(uuid: String, path: String) -> Result<(), String> {
    let b64 = fetch_skin_official(uuid).await?;
    write_b64_file(&b64, &path).await
}

/// 上传本地皮肤到官方账号；token 由后端取并自动刷新。
#[tauri::command]
pub async fn upload_skin_file(file_path: String, variant: String) -> Result<(), String> {
    if !matches!(variant.as_str(), "CLASSIC" | "SLIM") {
        bail!(
            "未知皮肤模型
{variant}"
        );
    }
    let token = account::valid_access_token().await?;
    account::upload_skin(&token, &file_path, &variant).await
}

#[tauri::command]
pub async fn fetch_capes() -> Result<Vec<account::CapeInfo>, String> {
    let token = account::valid_access_token().await?;
    account::fetch_capes(&token).await
}

#[tauri::command]
pub async fn set_active_cape(cape_id: String) -> Result<(), String> {
    let token = account::valid_access_token().await?;
    account::set_active_cape(&token, &cape_id).await
}

#[tauri::command]
pub async fn disable_cape() -> Result<(), String> {
    let token = account::valid_access_token().await?;
    account::disable_cape(&token).await
}

/// 确保当前正版账号凭证可用（临近过期才刷新）；未过期时零网络开销。
#[tauri::command]
pub async fn ensure_ms_account() -> Result<model::MsAccount, String> {
    account::ensure_valid_account().await
}

#[tauri::command]
pub async fn refresh_ms_account() -> Result<model::MsAccount, String> {
    let account = model::read().ok_or("未登录 Microsoft 账号")?;
    let refreshed = microsoft::refresh_login(&account)
        .await
        .map_err(|e| e.to_string())?;
    model::save(&refreshed)?;
    Ok(refreshed)
}

// ── 第三方登录（authlib-injector / 皮肤站 / Yggdrasil 兼容服务器）──

/// 第三方登录：验证地址 + 账号密码 → 保存账号 + 确保 jar。
#[tauri::command]
pub async fn external_login(
    api_url: String,
    username: String,
    password: String,
) -> Result<external::ExternalAccount, String> {
    let acc = external::login(&api_url, &username, &password).await?;
    external::save(&acc)?;
    // 确保 authlib-injector.jar 就绪（启动注入用）；失败不阻塞登录
    if let Err(e) = external::ensure_jar().await {
        eprintln!("[external] authlib-injector 下载失败: {e}");
    }
    Ok(acc)
}

#[tauri::command]
pub fn get_external_accounts() -> Vec<external::ExternalAccount> {
    external::read_all()
}

/// 切换当前第三方账号（置为列表末尾 = 启用）。
#[tauri::command]
pub fn switch_external_account(api_url: String, uuid: String) -> Result<(), String> {
    external::switch(&api_url, &uuid)
}

#[tauri::command]
pub async fn refresh_external_account(
    api_url: String,
    uuid: String,
) -> Result<external::ExternalAccount, String> {
    external::refresh_account(&api_url, &uuid).await
}

/// 切换第三方账号角色（refresh + selectedProfile，无密码）。
#[tauri::command]
pub async fn switch_external_role(
    api_url: String,
    uuid: String,
    profile_id: String,
    profile_name: String,
) -> Result<external::ExternalAccount, String> {
    external::switch_role(&api_url, &uuid, &profile_id, &profile_name).await
}

#[tauri::command]
pub fn remove_external_account(api_url: String, uuid: String) -> Result<(), String> {
    external::remove(&api_url, &uuid)
}

/// 第三方配置（无配置回默认内置预设）。
#[tauri::command]
pub fn get_external_config() -> external::ExternalConfig {
    external::ExternalConfig::load()
}

#[tauri::command]
pub fn save_external_config(config: external::ExternalConfig) -> Result<(), String> {
    persist::save(constants::SECT_EXTERNAL, &config).map_err(common_err_ctx_key!("errors.config.write", "保存失败"))
}
