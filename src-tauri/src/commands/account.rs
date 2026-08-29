//! 账号与皮肤命令（薄胶水层）：逻辑在 auth/（microsoft=协议、mojang=API、account=服务）。

use tauri::AppHandle;

use crate::auth::account;
use crate::auth::microsoft::{self, CALLBACK_PORT};
use crate::auth::model::MsAccount;
use crate::auth::mojang;

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

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| format!("回调端口 {CALLBACK_PORT} 被占用\n{e}"))?;

    let auth_url = microsoft::build_auth_url(&state, &pkce).map_err(|e| e.to_string())?;

    // 后台等待一次回调（含超时），完成后端口自动释放
    let app_handle = app.clone();
    let handle = tokio::task::spawn(async move {
        account::wait_callback(app_handle, listener, &state, &pkce).await;
    });
    *LOGIN_TASK.lock() = Some(handle);

    // 打开系统浏览器
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(auth_url.clone(), None::<&str>)
        .map_err(|e| format!("打开浏览器失败\n{e}"))?;

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
pub fn get_ms_account() -> Option<MsAccount> {
    crate::auth::model::read()
}

/// 读取全部已保存的正版账号（登录时间升序，末尾为当前账号）。
#[tauri::command]
pub fn get_ms_accounts() -> Vec<MsAccount> {
    crate::auth::model::read_all()
}

/// 移除指定 uuid 的正版账号（连同其长期 refresh_token）。删除当前账号后自动回退到上一账号。
#[tauri::command]
pub fn remove_ms_account(uuid: String) -> Result<(), String> {
    crate::auth::model::remove(&uuid)
}

/// 切换当前正版账号：按 uuid 找到账号，refresh_token 静默刷新后置为当前。
/// refresh_token 失效时返回错误（该账号需重新走 OAuth 登录）。
#[tauri::command]
pub async fn switch_ms_account(uuid: String) -> Result<MsAccount, String> {
    let account = crate::auth::model::read_all()
        .into_iter()
        .find(|a| a.uuid == uuid)
        .ok_or("该账号不在列表中，请重新登录")?;
    let refreshed = microsoft::refresh_login(&account)
        .await
        .map_err(|e| e.to_string())?;
    crate::auth::model::save(&refreshed)?;
    Ok(refreshed)
}

/// 通过 visage 3D 渲染服务获取头部头像（正面视角、光照），返回 base64 PNG。
/// subject 支持 uuid、用户名或 Mojang texture ID（刷新皮肤后传新 texture ID 可绕过服务端缓存）。
#[tauri::command]
pub async fn fetch_avatar(subject: String) -> Result<String, String> {
    let url = format!("https://visage.surgeplay.com/head/{subject}.png");
    account::fetch_b64(&url, "请求头像服务").await
}

/// 当前皮肤的 Mojang texture ID（textures.minecraft.net 文件 hash），
/// 供 visage 按 ID 渲染——上传新皮肤后立即生效，不受服务端 uuid 缓存影响。
#[tauri::command]
pub async fn fetch_texture_id(uuid: String) -> Result<String, String> {
    let textures = mojang::profile_textures(&uuid).await?;
    let url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    crate::util::url_file_name(url)
        .map(str::to_string)
        .ok_or_else(|| "皮肤地址格式异常".to_string())
}

/// 提取游戏内置默认皮肤（经典蓝衬衫 Steve / Alex），返回 base64。
/// 纹理编译期内置（resources/*.png，源自客户端 jar 的 player/wide/steve.png 与 player/slim/alex.png）——
/// 不依赖已下载版本：新版 jar 纹理路径已改为 player/wide|slim 子目录，旧扫描逻辑必然失败。
#[tauri::command]
pub fn fetch_default_skin(skin: String) -> Result<String, String> {
    use base64::Engine;
    let data: &[u8] = match skin.as_str() {
        "steve" => include_bytes!("../../resources/steve.png"),
        "alex" => include_bytes!("../../resources/alex.png"),
        _ => return Err("未知皮肤类型".into()),
    };
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

/// 通过官方 API 拉取指定 uuid 的皮肤 PNG，返回 base64（供 save_skin 保存用）。
async fn fetch_skin(uuid: String) -> Result<String, String> {
    let textures = mojang::profile_textures(&uuid).await?;
    let skin_url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    account::fetch_b64(skin_url, "下载皮肤").await
}

/// 拉取指定 uuid 正版玩家的皮肤 PNG（base64，供账号列表 2D 头像，前端截取头部）。
#[tauri::command]
pub async fn fetch_account_skin(uuid: String) -> Result<String, String> {
    fetch_skin(uuid).await
}

/// 拉取第三方账号（authlib-injector / 皮肤站）的皮肤 PNG（base64，供 3D 头像）。
/// 标准 Yggdrasil：GET {api}/sessionserver/session/minecraft/profile/{uuid} → properties[0].value
/// （base64 JSON 含 textures.skin.url）；Ely.by 非标准路径 /session/profile/{uuid}。
#[tauri::command]
pub async fn fetch_external_skin(api_url: String, uuid: String) -> Result<String, String> {
    let api = api_url.trim().trim_end_matches('/');
    if api.is_empty() {
        return Err("验证服务器地址为空".into());
    }
    let is_ely = crate::auth::external::is_ely_by(api);
    let url = if is_ely {
        format!("{api}/session/profile/{uuid}")
    } else {
        format!("{api}/sessionserver/session/minecraft/profile/{uuid}")
    };
    let resp = crate::downloader::fetcher::http()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("请求皮肤失败\n{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("该账号未设置皮肤\nHTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("响应解析失败\n{e}"))?;
    // properties[0].value = base64 的 {textures:{SKIN:{url}}}（标准格式）
    let prop = json["properties"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["value"].as_str())
        .ok_or("该账号未设置皮肤")?;
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(prop)
        .map_err(|e| format!("皮肤数据解码失败\n{e}"))?;
    let textures: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|e| format!("皮肤信息解析失败\n{e}"))?;
    let skin_url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    crate::auth::account::fetch_b64(skin_url, "下载皮肤").await
}


/// 第三方账号皮肤下载：拉 skin.png → 写指定路径（base64 解出）。
#[tauri::command]
pub async fn save_external_skin(api_url: String, uuid: String, path: String) -> Result<(), String> {
    let b64 = fetch_external_skin(api_url, uuid).await?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("皮肤数据解码失败\n{e}"))?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| format!("写入文件失败\n{e}"))
}

/// 把当前皮肤保存到用户指定路径。
#[tauri::command]
pub async fn save_skin(uuid: String, path: String) -> Result<(), String> {
    use base64::Engine;
    let b64 = fetch_skin(uuid).await?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("皮肤数据解码失败\n{e}"))?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| format!("写入文件失败\n{e}"))
}

/// 上传本地皮肤到官方账号（POST multipart: variant=CLASSIC + file）。
#[tauri::command]
pub async fn upload_skin_file(
    access_token: String,
    file_path: String,
    variant: String,
) -> Result<(), String> {
    if !matches!(variant.as_str(), "CLASSIC" | "SLIM") {
        return Err(format!(
            "未知皮肤模型
{variant}"
        ));
    }
    account::upload_skin(&access_token, &file_path, &variant).await
}

/// 拉取官方披风列表（GET /minecraft/profile，需 Bearer token）。
#[tauri::command]
pub async fn fetch_capes(access_token: String) -> Result<Vec<account::CapeInfo>, String> {
    account::fetch_capes(&access_token).await
}

/// 切换当前启用的披风（PUT /minecraft/profile/capes/active + JSON capeId）。
#[tauri::command]
pub async fn set_active_cape(access_token: String, cape_id: String) -> Result<(), String> {
    account::set_active_cape(&access_token, &cape_id).await
}

/// 移除当前启用的披风（DELETE /minecraft/profile/capes/active）。
#[tauri::command]
pub async fn disable_cape(access_token: String) -> Result<(), String> {
    account::disable_cape(&access_token).await
}

/// 刷新正版账号（静默，refresh_token 失效时返回错误）。
#[tauri::command]
pub async fn refresh_ms_account() -> Result<MsAccount, String> {
    let account = crate::auth::model::read().ok_or("未登录 Microsoft 账号")?;
    let refreshed = microsoft::refresh_login(&account)
        .await
        .map_err(|e| e.to_string())?;
    crate::auth::model::save(&refreshed)?;
    Ok(refreshed)
}

// ── 第三方登录（authlib-injector / 皮肤站 / Yggdrasil 兼容服务器）──

/// 第三方登录：验证服务器地址 + 账号密码 → authenticate → 保存账号 + 确保 jar。
#[tauri::command]
pub async fn external_login(
    api_url: String,
    username: String,
    password: String,
) -> Result<crate::auth::external::ExternalAccount, String> {
    let acc = crate::auth::external::login(&api_url, &username, &password).await?;
    crate::auth::external::save(&acc)?;
    // 登录成功即确保 authlib-injector.jar 就绪（启动注入用）；失败不阻塞登录
    if let Err(e) = crate::auth::external::ensure_jar().await {
        eprintln!("[external] authlib-injector 下载失败: {e}");
    }
    Ok(acc)
}

/// 第三方账号列表。
#[tauri::command]
pub fn get_external_accounts() -> Vec<crate::auth::external::ExternalAccount> {
    crate::auth::external::read_all()
}

/// 切换当前第三方账号（置为列表末尾 = 启用）。
#[tauri::command]
pub fn switch_external_account(api_url: String, uuid: String) -> Result<(), String> {
    crate::auth::external::switch(&api_url, &uuid)
}

/// 刷新第三方账号（refresh：新 token + 最新角色列表 + 角色变化同步）。
#[tauri::command]
pub async fn refresh_external_account(
    api_url: String,
    uuid: String,
) -> Result<crate::auth::external::ExternalAccount, String> {
    crate::auth::external::refresh_account(&api_url, &uuid).await
}

/// 切换第三方账号角色（refresh + selectedProfile，无密码）。
#[tauri::command]
pub async fn switch_external_role(
    api_url: String,
    uuid: String,
    profile_id: String,
    profile_name: String,
) -> Result<crate::auth::external::ExternalAccount, String> {
    crate::auth::external::switch_role(&api_url, &uuid, &profile_id, &profile_name).await
}

/// 移除第三方账号（api_url + uuid 定位）。
#[tauri::command]
pub fn remove_external_account(api_url: String, uuid: String) -> Result<(), String> {
    crate::auth::external::remove(&api_url, &uuid)
}

/// 预设验证服务器（label 展示名 + value API 地址）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThirdPartyServer {
    pub label: String,
    pub value: String,
}

/// 第三方登录配置（预设验证服务器列表，label + value）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ThirdPartyConfig {
    /// 常用验证服务器（皮肤站）预设列表
    #[serde(default)]
    pub servers: Vec<ThirdPartyServer>,
}

/// 默认内置预设（知名 Yggdrasil 兼容站）。
pub fn default_third_party_servers() -> Vec<ThirdPartyServer> {
    vec![
        ThirdPartyServer {
            label: "LittleSkin".into(),
            value: "https://littleskin.cn/api/yggdrasil".into(),
        },
        ThirdPartyServer {
            label: "Ely.by".into(),
            value: "https://authserver.ely.by".into(),
        },
    ]
}

/// 读取第三方配置（无配置回默认内置预设）。
#[tauri::command]
pub fn get_third_party_config() -> ThirdPartyConfig {
    let cfg: ThirdPartyConfig =
        crate::persist::read(crate::constants::SECT_THIRD_PARTY).unwrap_or_default();
    if cfg.servers.is_empty() {
        return ThirdPartyConfig {
            servers: default_third_party_servers(),
        };
    }
    cfg
}

/// 保存第三方配置（预设列表）。
#[tauri::command]
pub fn save_third_party_config(config: ThirdPartyConfig) -> Result<(), String> {
    crate::persist::save(crate::constants::SECT_THIRD_PARTY, &config)
        .map_err(|e| format!("保存失败\n{e}"))
}
