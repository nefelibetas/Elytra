//! 账号与皮肤命令（薄胶水层）：逻辑在 auth/（microsoft=协议、mojang=API、account=服务）。

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

/// 当前进行中的登录回调等待任务（取消登录时 abort 以释放回调端口；完成后残留无害，下次启动覆盖）。
static LOGIN_TASK: std::sync::LazyLock<parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// 启动 Microsoft 登录：起本地回调服务、打开浏览器，返回授权 URL。
/// 结果通过 `ms-login-complete` 事件推送。
#[tauri::command]
pub async fn start_ms_login(app: AppHandle) -> Result<String, String> {
    // 中止上次未完成的登录（若有）：释放回调端口，保证重复点击登录可重入
    if let Some(h) = LOGIN_TASK.lock().take() {
        h.abort();
    }

    let state = microsoft::generate_state().map_err(|e| e.to_string())?;
    let pkce = microsoft::generate_pkce().map_err(|e| e.to_string())?;

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", microsoft::CALLBACK_PORT))
        .await
        .map_err(common_err_ctx!("回调端口 {} 被占用", microsoft::CALLBACK_PORT))?;

    let auth_url = microsoft::build_auth_url(&state, &pkce).map_err(|e| e.to_string())?;

    // 后台等待一次回调（含超时），完成后端口自动释放
    let app_handle = app.clone();
    let handle = tokio::task::spawn(async move {
        account::wait_callback(app_handle, listener, &state, &pkce).await;
    });
    *LOGIN_TASK.lock() = Some(handle);

    // 打开系统浏览器
    app.opener()
        .open_url(auth_url.clone(), None::<&str>)
        .map_err(common_err_ctx!("打开浏览器失败"))?;

    Ok(auth_url)
}

/// 取消进行中的微软登录（abort 回调等待任务，释放回调端口；幂等，无任务时无操作）。
#[tauri::command]
pub fn cancel_ms_login() {
    if let Some(h) = LOGIN_TASK.lock().take() {
        h.abort();
    }
}

/// 读取已保存的正版账号（无则 None）。
#[tauri::command]
pub fn get_ms_account() -> Option<model::MsAccount> {
    model::read()
}

/// 读取全部已保存的正版账号（登录时间升序，末尾为当前账号）。
#[tauri::command]
pub fn get_ms_accounts() -> Vec<model::MsAccount> {
    model::read_all()
}

/// 移除指定 uuid 的正版账号（连同其长期 refresh_token）。删除当前账号后自动回退到上一账号。
#[tauri::command]
pub fn remove_ms_account(uuid: String) -> Result<(), String> {
    model::remove(&uuid)
}

/// 切换当前正版账号：按 uuid 找到账号，refresh_token 静默刷新后置为当前。
/// refresh_token 失效时返回错误（该账号需重新走 OAuth 登录）。
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

/// 提取游戏内置默认皮肤（经典蓝衬衫 Steve / Alex），返回 base64。
/// 纹理编译期内置（resources/*.png，源自客户端 jar 的 player/wide/steve.png 与 player/slim/alex.png）——
/// 不依赖已下载版本：新版 jar 纹理路径已改为 player/wide|slim 子目录，旧扫描逻辑必然失败。
#[tauri::command]
pub fn fetch_default_skin(skin: String) -> Result<String, String> {
    let data: &[u8] = match skin.as_str() {
        "steve" => include_bytes!("../../resources/steve.png"),
        "alex" => include_bytes!("../../resources/alex.png"),
        _ => return Err("未知皮肤类型".into()),
    };
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

/// 拉取指定 uuid 的皮肤 PNG（base64，头像用）：官方 sessionserver 失败自动回退
/// minotar——装饰性资源（账号列表 2D 头像）不硬依赖官方直连。
async fn fetch_skin_for_avatar(uuid: String) -> Result<String, String> {
    let (url, _model) = mojang::skin_profile_with_fallback(&uuid).await?;
    account::fetch_b64(&url, "下载皮肤").await
}

/// 拉取指定 uuid 的皮肤 PNG（base64，保存用）：仅官方直连，失败如实报错
/// （用户主动保存到本地，不应静默从第三方源取）。
async fn fetch_skin_official(uuid: String) -> Result<String, String> {
    let textures = mojang::profile_textures(&uuid).await?;
    let skin_url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    account::fetch_b64(skin_url, "下载皮肤").await
}

/// 拉取指定 uuid 正版玩家的皮肤 PNG（base64，供账号列表 2D 头像，前端截取头部）。
#[tauri::command]
pub async fn fetch_account_skin(uuid: String) -> Result<String, String> {
    fetch_skin_for_avatar(uuid).await
}

/// 正版玩家皮肤 + 模型类型（供 3D 头像渲染：一次 IPC 拿全，避免两次网络往返）。
#[derive(serde::Serialize)]
pub struct SkinProfile {
    /// 皮肤 PNG base64（64x32 旧格式或 64x64 新格式）。
    skin: String,
    /// 模型类型："classic"（Steve 宽）或 "slim"（Alex 窄）。
    model: String,
}

/// 拉取正版玩家皮肤 PNG + 模型类型（slim/classic），一次 IPC 供 3D 头像渲染。
/// 皮肤 texture 的 metadata.model 为 "slim" 时前端按纤细模型渲染。
/// 官方 sessionserver 直连失败（无代理被墙等）自动回退 minotar（装饰性资源不硬依赖官方）。
#[tauri::command]
pub async fn fetch_account_skin_profile(uuid: String) -> Result<SkinProfile, String> {
    let (url, model) = mojang::skin_profile_with_fallback(&uuid).await?;
    let b64 = account::fetch_b64(&url, "下载皮肤").await?;
    Ok(SkinProfile { skin: b64, model })
}

/// 拉取第三方账号（authlib-injector / 皮肤站）的皮肤 PNG（base64，供 3D 头像）。
#[tauri::command]
pub async fn fetch_external_skin(api_url: String, uuid: String) -> Result<String, String> {
    let (url, _model) = external::external_skin_info(&api_url, &uuid).await?;
    account::fetch_b64(&url, "下载皮肤").await
}

/// 拉取第三方账号（authlib-injector / 皮肤站）皮肤 PNG + 模型类型（slim/classic），
/// 一次 IPC 供 3D 头像渲染。
#[tauri::command]
pub async fn fetch_external_skin_profile(
    api_url: String,
    uuid: String,
) -> Result<SkinProfile, String> {
    let (url, model) = external::external_skin_info(&api_url, &uuid).await?;
    let b64 = account::fetch_b64(&url, "下载皮肤").await?;
    Ok(SkinProfile { skin: b64, model })
}

/// base64 皮肤字节 → 写文件（两个保存命令共用尾段）。
async fn write_b64_file(b64: &str, path: &str) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(common_err_ctx!("皮肤数据解码失败"))?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(common_err_ctx!("写入文件失败"))
}

/// 第三方账号皮肤下载：拉 skin.png → 写指定路径（base64 解出）。
#[tauri::command]
pub async fn save_external_skin(api_url: String, uuid: String, path: String) -> Result<(), String> {
    let b64 = fetch_external_skin(api_url, uuid).await?;
    write_b64_file(&b64, &path).await
}

/// 把当前皮肤保存到用户指定路径。
#[tauri::command]
pub async fn save_skin(uuid: String, path: String) -> Result<(), String> {
    let b64 = fetch_skin_official(uuid).await?;
    write_b64_file(&b64, &path).await
}

/// 上传本地皮肤到官方账号（POST multipart: variant=CLASSIC + file）。
/// token 由后端取（临近过期自动静默刷新），前端不再传可能已过期的缓存值。
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

/// 拉取官方披风列表（GET /minecraft/profile；token 由后端取并自动刷新）。
#[tauri::command]
pub async fn fetch_capes() -> Result<Vec<account::CapeInfo>, String> {
    let token = account::valid_access_token().await?;
    account::fetch_capes(&token).await
}

/// 切换当前启用的披风（PUT /minecraft/profile/capes/active + JSON capeId）。
#[tauri::command]
pub async fn set_active_cape(cape_id: String) -> Result<(), String> {
    let token = account::valid_access_token().await?;
    account::set_active_cape(&token, &cape_id).await
}

/// 移除当前启用的披风（DELETE /minecraft/profile/capes/active）。
#[tauri::command]
pub async fn disable_cape() -> Result<(), String> {
    let token = account::valid_access_token().await?;
    account::disable_cape(&token).await
}

/// 确保当前正版账号凭证可用（临近过期才刷新），返回账号本体供前端更新缓存。
/// 启动游戏前调用：未过期时零网络开销，避免每次都走完整微软登录链。
#[tauri::command]
pub async fn ensure_ms_account() -> Result<model::MsAccount, String> {
    account::ensure_valid_account().await
}

/// 刷新正版账号（静默，refresh_token 失效时返回错误）。
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

/// 第三方登录：验证服务器地址 + 账号密码 → authenticate → 保存账号 + 确保 jar。
#[tauri::command]
pub async fn external_login(
    api_url: String,
    username: String,
    password: String,
) -> Result<external::ExternalAccount, String> {
    let acc = external::login(&api_url, &username, &password).await?;
    external::save(&acc)?;
    // 登录成功即确保 authlib-injector.jar 就绪（启动注入用）；失败不阻塞登录
    if let Err(e) = external::ensure_jar().await {
        eprintln!("[external] authlib-injector 下载失败: {e}");
    }
    Ok(acc)
}

/// 第三方账号列表。
#[tauri::command]
pub fn get_external_accounts() -> Vec<external::ExternalAccount> {
    external::read_all()
}

/// 切换当前第三方账号（置为列表末尾 = 启用）。
#[tauri::command]
pub fn switch_external_account(api_url: String, uuid: String) -> Result<(), String> {
    external::switch(&api_url, &uuid)
}

/// 刷新第三方账号（refresh：新 token + 最新角色列表 + 角色变化同步）。
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

/// 移除第三方账号（api_url + uuid 定位）。
#[tauri::command]
pub fn remove_external_account(api_url: String, uuid: String) -> Result<(), String> {
    external::remove(&api_url, &uuid)
}

/// 读取第三方配置（无配置回默认内置预设）。
/// 模型与默认值在 auth/external.rs（命令层薄胶水）。
#[tauri::command]
pub fn get_external_config() -> external::ExternalConfig {
    external::ExternalConfig::load()
}

/// 保存第三方配置（预设列表）。
#[tauri::command]
pub fn save_external_config(config: external::ExternalConfig) -> Result<(), String> {
    persist::save(constants::SECT_EXTERNAL, &config).map_err(common_err_ctx!("保存失败"))
}
