//! 账号域服务：登录回调处理、远程资源下载（base64）。
//! 命令层（commands/account.rs）只做薄封装，协议在 microsoft.rs，API 在 mojang.rs。

use crate::auth;
use crate::constants;
use crate::events;
use crate::task;

use base64::Engine;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 等待浏览器回调的超时（5 分钟）。
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// 统一 HTTP 客户端（Elytra UA + 可配超时；visage 等外部服务要求自定义 UA）。
fn http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(constants::UA_MOJANG_API)
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("初始化客户端失败\n{e}"))
}

/// token 刷新余量（秒）：距过期不足此值就提前刷新，避开「刚好过期」竞态。
const TOKEN_REFRESH_SKEW_SECS: u64 = 300;

/// 当前 unix 秒（系统时间早于纪元时退化为 0）。
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 取当前正版账号，并确保其凭证可用：临近过期时才用 refresh_token 静默刷新并回写列表。
///
/// 背景：`MsAccount.expires_at` 此前只在写入时设置、**从未被读取**，于是所有用 token 的功能
/// （披风 / 皮肤 / 启动）拿到的都可能是已过期的缓存值 —— MC token 只有约 24h 有效期，
/// 过期后表现为 401，而账号仍在列表里（就是「明明已登录却失败」的假象）。
///
/// 注意与 `refresh_ms_account` 的区别：后者**无条件**走完整微软登录链（MS token → Xbox Live
/// → XSTS → MC login，四五次网络往返），作为「启动游戏前每次跑一遍」太重；本函数只在
/// `expires_at` 已到/临近时才付这份代价。
pub async fn ensure_valid_account() -> Result<auth::model::MsAccount, String> {
    let account = auth::model::read().ok_or_else(|| "尚未登录正版账号".to_string())?;
    if account.expires_at > unix_now() + TOKEN_REFRESH_SKEW_SECS {
        return Ok(account);
    }
    let refreshed = auth::microsoft::refresh_login(&account)
        .await
        .map_err(|e| format!("登录凭证已过期，自动刷新失败（请重新登录）\n{e}"))?;
    auth::model::save(&refreshed)?;
    Ok(refreshed)
}

/// 只要 token 字符串的便捷入口（披风 / 皮肤等 API 调用与启动参数注入共用）。
pub async fn valid_access_token() -> Result<String, String> {
    ensure_valid_account().await.map(|a| a.access_token)
}

/// 下载远程资源并返回 base64（头像/纹理/皮肤/披风共用）。
pub async fn fetch_b64(url: &str, err_prefix: &str) -> Result<String, String> {
    let resp = http_client(15)?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("{err_prefix}失败\n{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{err_prefix}失败\nHTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("下载数据失败\n{e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 启动 Microsoft 登录的浏览器回调等待（含超时），完成后端口自动释放。
/// 结果经 `ms-login-complete` 事件推送。
pub async fn wait_callback(
    app: tauri::AppHandle,
    listener: tokio::net::TcpListener,
    state: &str,
    pkce: &auth::microsoft::Pkce,
) {
    match tokio::time::timeout(CALLBACK_TIMEOUT, listener.accept()).await {
        Ok(Ok((stream, _))) => {
            handle_callback(stream, state, pkce, &app).await;
        }
        Ok(Err(e)) => {
            let _ = emit_result(&app, false, None, Some(format!("回调服务错误\n{e}")));
        }
        Err(_) => {
            let _ = emit_result(&app, false, None, Some("登录超时，请重新尝试".into()));
        }
    }
}

/// 处理一次浏览器回调：解析 code + state，完成登录，emit 结果。
async fn handle_callback(
    mut stream: tokio::net::TcpStream,
    state: &str,
    pkce: &auth::microsoft::Pkce,
    app: &tauri::AppHandle,
) {
    // 从流中最多读取4096字节
    let mut buf = [0u8; 4096];
    let read = match stream.read(&mut buf).await {
        Ok(n) => n,
        Err(e) => {
            let _ = emit_result(app, false, None, Some(format!("读取回调失败\n{e}")));
            return;
        }
    };
    let request = String::from_utf8_lossy(&buf[..read]).to_string();
    // 提取首行: GET /xxx
    let first_line = request.lines().next().unwrap_or("").to_string();

    // 解析 GET 请求的 query
    let query = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.split('?').nth(1))
        .unwrap_or("");

    let params: HashMap<String, String> = query
        .split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?.to_string();
            let v = it.next().unwrap_or("").to_string();
            Some((k, url_decode(&v)))
        })
        .collect();
    // 解析, 获取关键数据
    let (ok, account, error, http_status) = if let Some(err) = params.get("error") {
        (false, None, Some(format!("授权被拒绝\n{err}")), 400)
    } else {
        let code = params.get("code").cloned();
        let cb_state = params.get("state").cloned().unwrap_or_default();
        if cb_state != state {
            (false, None, Some("state 校验失败，请重新登录".into()), 403)
        } else if let Some(code) = code {
            match auth::microsoft::login_with_code(&code, &pkce.verifier).await {
                Ok(account) => {
                    let _ = auth::model::save(&account);
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
    app: &tauri::AppHandle,
    ok: bool,
    account: Option<auth::model::MsAccount>,
    error: Option<String>,
) -> Result<(), tauri::Error> {
    // 队列线程派发：回调服务线程不因 webview 未消费而阻塞（与 task-progress 同机制）
    task::emit_event(
        app,
        events::MS_LOGIN_COMPLETE,
        serde_json::json!({ "ok": ok, "account": account, "error": error }),
    );
    Ok(())
}

/// 返回一个简单的浏览器可见页面。
async fn respond(mut stream: tokio::net::TcpStream, status: u16, ok: bool) -> std::io::Result<()> {
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
    let s = s.replace('+', " ");
    percent_encoding::percent_decode(s.as_bytes())
        .decode_utf8()
        .unwrap_or_default()
        .to_string()
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
pub async fn fetch_capes(access_token: &str) -> Result<Vec<CapeInfo>, String> {
    let resp = http_client(15)?
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("获取披风列表失败\n{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("获取披风列表失败\nHTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败\n{e}"))?;
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
pub async fn set_active_cape(access_token: &str, cape_id: &str) -> Result<(), String> {
    let resp = http_client(15)?
        .put("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(access_token)
        .json(&serde_json::json!({ "capeId": cape_id }))
        .send()
        .await
        .map_err(|e| format!("切换披风失败\n{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("切换披风失败\nHTTP {}", resp.status()));
    }
    Ok(())
}

/// 移除当前启用的披风（DELETE /minecraft/profile/capes/active）。
pub async fn disable_cape(access_token: &str) -> Result<(), String> {
    let resp = http_client(15)?
        .delete("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("移除披风失败\n{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("移除披风失败\nHTTP {}", resp.status()));
    }
    Ok(())
}

/// 上传本地皮肤到官方账号（POST multipart: variant=CLASSIC + file）。
/// 注意：披风没有公开上传端点（官方仅支持网页/游戏内上传），故只有皮肤走此通道。
pub async fn upload_skin(access_token: &str, file_path: &str, variant: &str) -> Result<(), String> {
    let bytes = tokio::fs::read(file_path)
        .await
        .map_err(|e| format!("读取文件失败\n{e}"))?;
    let form = reqwest::multipart::Form::new()
        .text("variant", variant.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(bytes)
                .file_name("image.png")
                .mime_str("image/png")
                .map_err(|e| format!("构造上传失败\n{e}"))?,
        );
    // 官方文档实测为 POST（PUT 在认证通过后返回 405 Method Not Allowed）
    let resp = http_client(30)?
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(access_token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("上传皮肤失败\n{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        return Err(format!(
            "上传皮肤失败\n{}",
            auth::microsoft::extract_api_error(&body, status)
        ));
    }
    Ok(())
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
