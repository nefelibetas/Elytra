//! 账号与皮肤命令（薄胶水层）：逻辑在 auth/（microsoft=协议、mojang=API、account=服务）。

use tauri::AppHandle;

use crate::auth::account;
use crate::auth::microsoft::{self, CALLBACK_PORT};
use crate::auth::model::MsAccount;
use crate::auth::mojang;
use crate::constants;

/// 启动 Microsoft 登录：起本地回调服务、打开浏览器，返回授权 URL。
/// 结果通过 `ms-login-complete` 事件推送。
#[tauri::command]
pub async fn start_ms_login(app: AppHandle) -> Result<String, String> {
    let state = microsoft::generate_state();
    let pkce = microsoft::generate_pkce();

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| format!("回调端口 {CALLBACK_PORT} 被占用：{e}"))?;

    let auth_url = microsoft::build_auth_url(&state, &pkce).map_err(|e| e.to_string())?;

    // 后台等待一次回调（含超时），完成后端口自动释放
    let app_handle = app.clone();
    tokio::task::spawn(async move {
        account::wait_callback(app_handle, listener, &state, &pkce).await;
    });

    // 打开系统浏览器
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(auth_url.clone(), None::<&str>)
        .map_err(|e| format!("打开浏览器失败：{e}"))?;

    Ok(auth_url)
}

/// 读取已保存的正版账号（无则 None）。
#[tauri::command]
pub fn get_ms_account() -> Option<MsAccount> {
    crate::auth::model::read()
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
/// 从任一已下载版本 jar 的 assets/minecraft/textures/entity/ 提取（各版本相同）。
#[tauri::command]
pub fn fetch_default_skin(skin: String) -> Result<String, String> {
    use base64::Engine;
    use std::io::Read;

    let cfg: crate::config::downloader::model::DownloaderConfig =
        crate::persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let mc = cfg.minecraft_dir_abs();
    let name = format!("assets/minecraft/textures/entity/{}.png", skin);
    let versions_dir = mc.join("versions");
    let rd = std::fs::read_dir(&versions_dir).map_err(|e| e.to_string())?;
    // jar 位于 versions/<ver>/<ver>.jar（子目录），递归一层扫描
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&path) {
                for f in sub.flatten() {
                    let p = f.path();
                    if p.extension().map(|e| e == "jar").unwrap_or(false) {
                        if let Ok(file) = std::fs::File::open(&p) {
                            if let Ok(mut archive) = zip::ZipArchive::new(file) {
                                if let Ok(mut f) = archive.by_name(&name) {
                                    let mut buf = Vec::new();
                                    if f.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                                        return Ok(
                                            base64::engine::general_purpose::STANDARD.encode(&buf)
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Err("未找到游戏内置皮肤文件".into())
}

/// 获取正版玩家的皮肤纹理（原始 PNG），返回 base64。
/// api.mojang.com 查 UUID → sessionserver 取 textures property → 解码 payload → 下载纹理。
#[tauri::command]
pub async fn fetch_player_skin_texture(player_name: String) -> Result<String, String> {
    use base64::Engine;

    // 1. 用户名 → UUID
    let uuid = mojang::name_to_uuid(player_name.trim()).await?;

    // 2. sessionserver → textures property（base64 payload）
    let value_b64 = mojang::texture_value(&uuid).await?;

    // 3. 解码 payload → SKIN.url → 下载纹理
    let payload_bytes = base64::engine::general_purpose::STANDARD
        .decode(value_b64)
        .map_err(|e| e.to_string())?;
    let payload: serde_json::Value =
        serde_json::from_slice(&payload_bytes).map_err(|e| e.to_string())?;
    let skin_url = payload
        .pointer("/textures/SKIN/url")
        .and_then(|v| v.as_str())
        .ok_or("皮肤地址缺失")?;
    account::fetch_b64(skin_url, "下载皮肤").await
}

/// 通过官方 API 拉取指定 uuid 的皮肤 PNG，返回 base64（供 save_skin 保存用）。
async fn fetch_skin(uuid: String) -> Result<String, String> {
    let textures = mojang::profile_textures(&uuid).await?;
    let skin_url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    account::fetch_b64(skin_url, "下载皮肤").await
}

/// 把当前皮肤保存到用户指定路径。
#[tauri::command]
pub async fn save_skin(uuid: String, path: String) -> Result<(), String> {
    use base64::Engine;
    let b64 = fetch_skin(uuid).await?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("皮肤数据解码失败：{e}"))?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| format!("写入文件失败：{e}"))
}

/// 上传本地皮肤到官方账号（POST multipart: variant=CLASSIC + file）。
#[tauri::command]
pub async fn upload_skin_file(access_token: String, file_path: String) -> Result<(), String> {
    account::upload_skin(&access_token, &file_path).await
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

/// 读取离线皮肤配置（PCL2 同款：随机/Steve/Alex/正版玩家/自定义）。
#[tauri::command]
pub fn get_offline_skin() -> crate::config::offline_skin::OfflineSkinConfig {
    crate::persist::read(constants::SECT_OFFLINE_SKIN).unwrap_or_default()
}

/// 保存离线皮肤配置。
#[tauri::command]
pub fn set_offline_skin(
    config: crate::config::offline_skin::OfflineSkinConfig,
) -> Result<(), String> {
    crate::persist::save(constants::SECT_OFFLINE_SKIN, &config).map_err(|e| e.to_string())
}

/// 校验正版玩家皮肤信息（刷新用）：返回 uuid + 当前 texture id。
#[tauri::command]
pub async fn fetch_player_skin(player_name: String) -> Result<serde_json::Value, String> {
    let uuid = mojang::name_to_uuid(&player_name).await?;
    let textures = mojang::profile_textures(&uuid).await?;
    let texture_id = textures["textures"]["SKIN"]["url"]
        .as_str()
        .and_then(crate::util::url_file_name)
        .unwrap_or("");
    Ok(serde_json::json!({ "uuid": uuid, "texture_id": texture_id }))
}

/// 保存正版玩家的皮肤到用户指定位置（玩家名 → UUID → 皮肤文件）。
#[tauri::command]
pub async fn save_player_skin(player_name: String, path: String) -> Result<(), String> {
    let uuid = mojang::name_to_uuid(&player_name).await?;
    save_skin(uuid, path).await
}

/// 读取本地皮肤 PNG 文件，返回 base64（供离线自定义皮肤 2D 正面渲染）。
#[tauri::command]
pub async fn read_skin_file(path: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("读取皮肤文件失败：{e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 退出登录：清除保存的账号。
#[tauri::command]
pub fn clear_ms_account() -> Result<(), String> {
    crate::auth::model::clear()
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
