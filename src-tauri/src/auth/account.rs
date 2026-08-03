//! 账号域服务：登录回调处理、远程资源下载（base64）。
//! 命令层（commands/account.rs）只做薄封装，协议在 microsoft.rs，API 在 mojang.rs。

use std::time::Duration;

use base64::Engine;
use tauri::{AppHandle, Emitter};

use crate::auth::microsoft::{self, Pkce};
use crate::auth::model::MsAccount;

/// 等待浏览器回调的超时（5 分钟）。
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);

/// 统一 HTTP 客户端（Elytra UA + 可配超时；visage 等外部服务要求自定义 UA）。
fn http_client(timeout_secs: u64) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("ElytraLauncher/0.1.0 (+https://github.com/nefelibetas/Elytra)")
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("初始化客户端失败：{e}"))
}

/// 下载远程资源并返回 base64（头像/纹理/皮肤/披风共用）。
pub async fn fetch_b64(url: &str, err_prefix: &str) -> Result<String, String> {
    let resp = http_client(15)?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("{err_prefix}失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("{err_prefix}失败({})", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 启动 Microsoft 登录的浏览器回调等待（含超时），完成后端口自动释放。
/// 结果经 `ms-login-complete` 事件推送。
pub async fn wait_callback(
    app: AppHandle,
    listener: tokio::net::TcpListener,
    state: &str,
    pkce: &Pkce,
) {
    match tokio::time::timeout(CALLBACK_TIMEOUT, listener.accept()).await {
        Ok(Ok((stream, _))) => {
            handle_callback(stream, state, pkce, &app).await;
        }
        Ok(Err(e)) => {
            let _ = emit_result(&app, false, None, Some(format!("回调服务错误：{e}")));
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
        serde_json::json!({ "ok": ok, "account": account, "error": error }),
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
pub async fn set_active_cape(access_token: &str, cape_id: &str) -> Result<(), String> {
    let resp = http_client(15)?
        .put("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(access_token)
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
pub async fn disable_cape(access_token: &str) -> Result<(), String> {
    let resp = http_client(15)?
        .delete("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("移除披风失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("移除披风失败({})", resp.status()));
    }
    Ok(())
}

/// 上传本地皮肤到官方账号（POST multipart: variant=CLASSIC + file）。
/// 注意：披风没有公开上传端点（官方仅支持网页/游戏内上传），故只有皮肤走此通道。
pub async fn upload_skin(access_token: &str, file_path: &str) -> Result<(), String> {
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
    // 官方文档实测为 POST（PUT 在认证通过后返回 405 Method Not Allowed）
    let resp = http_client(30)?
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(access_token)
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
