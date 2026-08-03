use crate::auth::microsoft::{self, Pkce, MS_CLIENT_ID};
use crate::auth::model::MsAccount;
use serde_json::json;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// 回调端口：固定，Azure 注册的 redirect_uri 必须一致。
const CALLBACK_PORT: u16 = 27205;
/// 等待浏览器回调的超时（5 分钟）。
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// 启动 Microsoft 登录：起本地回调服务、打开浏览器，返回授权 URL。
/// 结果通过 `ms-login-complete` 事件推送。
#[tauri::command]
pub async fn start_ms_login(app: AppHandle) -> Result<String, String> {
    if MS_CLIENT_ID.starts_with("PASTE_") {
        return Err("Azure 客户端 ID 未配置，请先在 auth/microsoft.rs 中填入".into());
    }

    let state = microsoft::generate_state();
    let pkce = microsoft::generate_pkce();

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| format!("回调端口 {CALLBACK_PORT} 被占用：{e}"))?;

    let auth_url = microsoft::build_auth_url(&state, &pkce);

    // 后台等待一次回调（含超时），完成后端口自动释放
    let app_handle = app.clone();
    tokio::task::spawn(async move {
        match tokio::time::timeout(CALLBACK_TIMEOUT, listener.accept()).await {
            Ok(Ok((stream, _))) => {
                handle_callback(stream, &state, &pkce, &app_handle).await;
            }
            Ok(Err(e)) => {
                let _ = emit_result(&app_handle, false, None, Some(format!("回调服务错误：{e}")));
            }
            Err(_) => {
                let _ = emit_result(&app_handle, false, None, Some("登录超时，请重新尝试".into()));
            }
        }
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

/// 官方会话服务：取 profile → 解码 textures 字段（base64 NBT → JSON）。
async fn fetch_profile_textures(uuid: &str) -> Result<serde_json::Value, String> {
    use base64::Engine;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let profile_url =
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}");
    let resp = client
        .get(&profile_url)
        .send()
        .await
        .map_err(|e| format!("获取角色资料失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("获取角色资料失败({})", resp.status()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let texture_value = json["properties"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|p| p["name"] == "textures")
                .and_then(|p| p["value"].as_str())
        })
        .ok_or("该账号未设置皮肤")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(texture_value)
        .map_err(|e| format!("皮肤数据解码失败：{e}"))?;
    serde_json::from_slice(&decoded).map_err(|e| format!("皮肤数据解析失败：{e}"))
}

/// 通过 visage 3D 渲染服务获取头部头像（正面视角、光照），返回 base64 PNG。
/// subject 支持 uuid、用户名或 Mojang texture ID（刷新皮肤后传新 texture ID 可绕过服务端缓存）。
/// visage 要求自定义 User-Agent（拒绝浏览器 UA），故走后端代理。
#[tauri::command]
pub async fn fetch_avatar(subject: String) -> Result<String, String> {
    use base64::Engine;
    let client = reqwest::Client::builder()
        .user_agent("ElytraLauncher/0.1.0 (+https://github.com/nefelibetas/Elytra)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let url = format!("https://visage.surgeplay.com/head/{subject}.png");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("请求头像服务失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("头像服务返回 {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 当前皮肤的 Mojang texture ID（textures.minecraft.net 文件 hash），
/// 供 visage 按 ID 渲染——上传新皮肤后立即生效，不受服务端 uuid 缓存影响。
#[tauri::command]
pub async fn fetch_texture_id(uuid: String) -> Result<String, String> {
    let textures = fetch_profile_textures(&uuid).await?;
    let url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    url.rsplit('/')
        .next()
        .map(str::to_string)
        .ok_or_else(|| "皮肤地址格式异常".to_string())
}

/// 下载 Mojang 纹理文件（textures.minecraft.net）原始 PNG，返回 base64。
/// 用于离线默认皮肤（Steve/Alex）头像：前端截取头部区域，避免 visage 渲染的帽子层。
#[tauri::command]
pub async fn fetch_skin_texture(texture_id: String) -> Result<String, String> {
    use base64::Engine;
    let client = reqwest::Client::builder()
        .user_agent("ElytraLauncher/0.1.0 (+https://github.com/nefelibetas/Elytra)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let url = format!("https://textures.minecraft.net/texture/{texture_id}");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("请求纹理失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("纹理服务返回 {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 提取游戏内置默认皮肤（经典蓝衬衫 Steve / Alex），返回 base64。
/// 从任一已下载版本 jar 的 assets/minecraft/textures/entity/ 提取（各版本相同）。
#[tauri::command]
pub fn fetch_default_skin(skin: String) -> Result<String, String> {
    use base64::Engine;
    use std::io::Read;

    let cfg: crate::config::downloader::model::DownloaderConfig =
        crate::persist::read("downloader").unwrap_or_default();
    let mc = std::path::Path::new(&cfg.minecraft_dir);
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
                                        return Ok(base64::engine::general_purpose::STANDARD
                                            .encode(&buf));
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
    let client = reqwest::Client::builder()
        .user_agent("ElytraLauncher/0.1.0 (+https://github.com/nefelibetas/Elytra)")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;

    // 1. 用户名 → UUID
    let resp = client
        .get(format!(
            "https://api.mojang.com/users/profiles/minecraft/{}",
            player_name.trim()
        ))
        .send()
        .await
        .map_err(|e| format!("查询玩家失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("查询玩家失败：HTTP {}", resp.status()));
    }
    let uuid = resp
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("玩家不存在")?
        .to_string();

    // 2. sessionserver → textures property
    let resp = client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}?unsigned=false"
        ))
        .send()
        .await
        .map_err(|e| format!("获取档案失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("获取档案失败：HTTP {}", resp.status()));
    }
    let profile = resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    let props = profile
        .get("properties")
        .and_then(|v| v.as_array())
        .ok_or("档案属性缺失")?;
    let textures_prop = props
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))
        .ok_or("该玩家未设置皮肤")?;
    let value_b64 = textures_prop
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or("纹理数据缺失")?;

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
    let resp = client
        .get(skin_url)
        .send()
        .await
        .map_err(|e| format!("下载皮肤失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载皮肤失败：HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 通过官方 API 拉取指定 uuid 的皮肤 PNG，返回 base64（前端本地渲染头像）。
/// 绕开浏览器跨域/混合内容限制，走 Rust 下载链路。
#[tauri::command]
pub async fn fetch_skin(uuid: String) -> Result<String, String> {
    use base64::Engine;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let textures = fetch_profile_textures(&uuid).await?;
    let skin_url = textures["textures"]["SKIN"]["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?;
    let skin_resp = client
        .get(skin_url)
        .send()
        .await
        .map_err(|e| format!("下载皮肤失败：{e}"))?;
    if !skin_resp.status().is_success() {
        return Err(format!("下载皮肤失败({})", skin_resp.status()));
    }
    let bytes = skin_resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 当前披风 PNG（base64），无披风返回 None。
#[tauri::command]
pub async fn fetch_cape(uuid: String) -> Result<Option<String>, String> {
    use base64::Engine;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let textures = fetch_profile_textures(&uuid).await?;
    let Some(cape_url) = textures["textures"]["CAPE"]["url"].as_str() else {
        return Ok(None);
    };
    let resp = client
        .get(cape_url)
        .send()
        .await
        .map_err(|e| format!("下载披风失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载披风失败({})", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(Some(base64::engine::general_purpose::STANDARD.encode(&bytes)))
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
/// 注意：披风没有公开上传端点（官方仅支持网页/游戏内上传），故只有皮肤走此通道。
#[tauri::command]
pub async fn upload_skin_file(
    access_token: String,
    file_path: String,
) -> Result<(), String> {
    let bytes = tokio::fs::read(file_path)
        .await
        .map_err(|e| format!("读取文件失败：{e}"))?;
    let form = reqwest::multipart::Form::new()
        .text("variant", "CLASSIC".to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(bytes)
                .file_name("image.png")
                .mime_str("image/png")
                .map_err(|e| format!("构造上传失败：{e}"))?,
        );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    // 官方文档实测为 POST（PUT 在认证通过后返回 405 Method Not Allowed）
    let resp = client
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(&access_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("上传皮肤失败：{e}"))?;
    if !resp.status().is_success() {
        // 官方 API 错误体是 {"path":…,"errorMessage":…}
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let msg = body["errorMessage"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(format!("上传皮肤失败：{msg}"));
    }
    Ok(())
}

/// 账号拥有的披风列表（官方 profile API），active = 当前启用。
#[derive(serde::Serialize)]
pub struct CapeInfo {
    pub id: String,
    pub alias: String,
    pub url: String,
    pub active: bool,
}

/// 拉取官方披风列表（GET /minecraft/profile，需 Bearer token）。
#[tauri::command]
pub async fn fetch_capes(access_token: String) -> Result<Vec<CapeInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| format!("获取披风列表失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("获取披风列表失败({})", resp.status()));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let capes = json["capes"].as_array().cloned().unwrap_or_default();
    Ok(capes
        .iter()
        .map(|c| CapeInfo {
            id: c["id"].as_str().unwrap_or("").to_string(),
            alias: c["alias"].as_str().unwrap_or("").to_string(),
            url: c["url"].as_str().unwrap_or("").to_string(),
            active: c["state"].as_str() == Some("ACTIVE"),
        })
        .collect())
}

/// 切换当前启用的披风（PUT /minecraft/profile/capes/active + JSON capeId）。
#[tauri::command]
pub async fn set_active_cape(
    access_token: String,
    cape_id: String,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let resp = client
        .put("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(&access_token)
        .json(&serde_json::json!({ "capeId": cape_id }))
        .send()
        .await
        .map_err(|e| format!("切换披风失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("切换披风失败({})", resp.status()));
    }
    Ok(())
}

/// 移除当前启用的披风（DELETE /minecraft/profile/capes/active）。
#[tauri::command]
pub async fn disable_cape(access_token: String) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let resp = client
        .delete("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| format!("移除披风失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("移除披风失败({})", resp.status()));
    }
    Ok(())
}

/// 读取离线皮肤配置（PCL2 同款：随机/Steve/Alex/正版玩家/自定义）。
#[tauri::command]
pub fn get_offline_skin() -> crate::config::offline_skin::OfflineSkinConfig {
    crate::persist::read("offline_skin").unwrap_or_default()
}

/// 保存离线皮肤配置。
#[tauri::command]
pub fn set_offline_skin(config: crate::config::offline_skin::OfflineSkinConfig) -> Result<(), String> {
    crate::persist::save("offline_skin", &config).map_err(|e| e.to_string())
}

/// 玩家名 → Mojang UUID（api.mojang.com，未找到返回错误）。
async fn mojang_name_to_uuid(name: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))?;
    let url = format!("https://api.mojang.com/users/profiles/minecraft/{}", name);
    let resp = client
        .get(&url)
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

/// 校验正版玩家皮肤信息（刷新用）：返回 uuid + 当前 texture id。
#[tauri::command]
pub async fn fetch_player_skin(player_name: String) -> Result<serde_json::Value, String> {
    let uuid = mojang_name_to_uuid(&player_name).await?;
    let textures = fetch_profile_textures(&uuid).await?;
    let texture_id = textures["textures"]["SKIN"]["url"]
        .as_str()
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or("");
    Ok(serde_json::json!({ "uuid": uuid, "texture_id": texture_id }))
}

/// 保存正版玩家的皮肤到用户指定位置（玩家名 → UUID → 皮肤文件）。
#[tauri::command]
pub async fn save_player_skin(player_name: String, path: String) -> Result<(), String> {
    let uuid = mojang_name_to_uuid(&player_name).await?;
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

/// 处理一次浏览器回调：解析 code + state，完成登录，emit 结果。
async fn handle_callback(
    mut stream: tokio::net::TcpStream,
    state: &str,
    pkce: &Pkce,
    app: &AppHandle,
) {
    use tokio::io::AsyncReadExt;

    let mut buf = [0u8; 4096];
    let read = match stream.read(&mut buf).await {
        Ok(n) => n,
        Err(e) => {
            let _ = emit_result(app, false, None, Some(format!("读取回调失败：{e}")));
            return;
        }
    };
    let request = String::from_utf8_lossy(&buf[..read]).to_string();
    let first_line = request.lines().next().unwrap_or("").to_string();

    // 解析 GET 请求的 query
    let query = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.split('?').nth(1))
        .unwrap_or("");

    let params: std::collections::HashMap<String, String> = query
        .split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?.to_string();
            let v = it.next().unwrap_or("").to_string();
            Some((k, url_decode(&v)))
        })
        .collect();

    let (ok, account, error, http_status) = if let Some(err) = params.get("error") {
        (false, None, Some(format!("授权被拒绝：{err}")), 400)
    } else {
        let code = params.get("code").cloned();
        let cb_state = params.get("state").cloned().unwrap_or_default();
        if cb_state != state {
            (false, None, Some("state 校验失败，请重新登录".into()), 403)
        } else if let Some(code) = code {
            match microsoft::login_with_code(&code, &pkce.verifier).await {
                Ok(account) => {
                    let _ = crate::auth::model::save(&account);
                    (true, Some(account), None, 200)
                }
                Err(e) => (false, None, Some(e.to_string()), 500),
            }
        } else {
            (false, None, Some("回调缺少授权码".into()), 400)
        }
    };

    let _ = emit_result(app, ok, account, error);
    let _ = respond(stream, http_status, ok).await;
}

fn emit_result(
    app: &AppHandle,
    ok: bool,
    account: Option<MsAccount>,
    error: Option<String>,
) -> Result<(), tauri::Error> {
    app.emit(
        crate::events::MS_LOGIN_COMPLETE,
        json!({ "ok": ok, "account": account, "error": error }),
    )
}

/// 返回一个简单的浏览器可见页面。
async fn respond(mut stream: tokio::net::TcpStream, status: u16, ok: bool) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    let body = if ok {
        "<!DOCTYPE html><html lang=\"zh\"><meta charset=\"utf-8\"><body style=\"font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:90vh;color:#059669;\"><h2>登录成功，可以关闭此窗口回到启动器</h2></body></html>"
    } else {
        "<!DOCTYPE html><html lang=\"zh\"><meta charset=\"utf-8\"><body style=\"font-family:sans-serif;display:flex;align-items:center;justify-content:center;height:90vh;color:#dc2626;\"><h2>登录失败，请关闭此窗口并重试</h2></body></html>"
    };
    let reason = if status == 200 { "OK" } else { "Error" };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("a%20b+c"), "a b c");
        assert_eq!(url_decode("%E4%B8%AD"), "中");
        assert_eq!(url_decode("plain"), "plain");
    }
}
