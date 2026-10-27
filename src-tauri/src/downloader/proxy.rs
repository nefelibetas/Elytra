//! 系统代理检测与应用：Windows 注册表 / Linux gsettings / 环境变量三路探测，按网络模式（Direct 直连 / Auto·System 应用系统代理）应用到 `ClientBuilder`。

#[cfg(target_os = "windows")]
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

use crate::config::downloader::NetworkMode;
use crate::persist::downloader_config;
use crate::util::NoWindow as _;

/// 解析 Windows 注册表 ProxyServer 值 → (http 代理, https 代理)：支持 `127.0.0.1:7890`（全协议）与 `http=...;https=...`（分协议，缺项 None）；连接统一用 http scheme（默认明文）。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))] // 生产调用仅 Windows；单测全平台跑
fn parse_proxy_server(raw: &str) -> (Option<String>, Option<String>) {
    let raw = raw.trim();
    let mut http = None;
    let mut https = None;
    if raw.contains('=') {
        for seg in raw.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let Some((scheme, addr)) = seg.split_once('=') else {
                continue;
            };
            let addr = addr.trim();
            if addr.is_empty() {
                continue;
            }
            match scheme.trim() {
                "http" => http = Some(format!("http://{addr}")),
                "https" => https = Some(format!("http://{addr}")),
                _ => {}
            }
        }
    } else if !raw.is_empty() {
        let full = format!("http://{raw}");
        http = Some(full.clone());
        https = Some(full);
    }
    (http, https)
}

/// 返回 (http 代理 URL, https 代理 URL)；检测顺序：Windows 注册表 Internet Settings 优先、环境变量兜底；Linux 环境变量优先、GNOME gsettings 兜底；其他平台仅环境变量。
fn system_proxy_urls() -> (Option<String>, Option<String>) {
    #[cfg(target_os = "windows")]
    {
        // 1. 注册表（系统代理写这里）
        if let Ok(settings) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        {
            let enabled: u32 = settings.get_value("ProxyEnable").unwrap_or(0);
            let server: Result<String, _> = settings.get_value("ProxyServer");
            if enabled == 1 {
                if let Ok(server) = server {
                    let (h, s) = parse_proxy_server(&server);
                    if h.is_some() || s.is_some() {
                        return (h, s);
                    }
                }
            }
        }
        // 2. 环境变量兜底
        env_proxy_urls()
    }
    #[cfg(target_os = "linux")]
    {
        // 1. 环境变量（终端/CI/容器常见）
        let env = env_proxy_urls();
        if env.0.is_some() || env.1.is_some() {
            return env;
        }
        // 2. GNOME 系统代理（gsettings org.gnome.system.proxy）
        linux_gsettings_proxy_urls()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        env_proxy_urls()
    }
}

/// 环境变量代理（http_proxy / https_proxy / all_proxy；no_proxy 由 NoProxy 处理）。
fn env_proxy_urls() -> (Option<String>, Option<String>) {
    let https = std::env::var("https_proxy")
        .or_else(|_| std::env::var("HTTPS_PROXY"))
        .ok();
    let http = std::env::var("http_proxy")
        .or_else(|_| std::env::var("HTTP_PROXY"))
        .ok()
        .or_else(|| std::env::var("all_proxy").ok())
        .or_else(|| std::env::var("ALL_PROXY").ok());
    (http, https)
}

/// GNOME 桌面系统代理（gsettings）；无 gsettings/非 manual 模式 → None。仅 Linux 调用，不加 cfg 以便 Windows 构建语法检查。
#[allow(dead_code)] // Windows 构建不调用
fn linux_gsettings_proxy_urls() -> (Option<String>, Option<String>) {
    fn gs(schema: &str, key: &str) -> Option<String> {
        let out = std::process::Command::new("gsettings")
            // 子进程无窗口装配
            .no_window()
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() || s == "''" {
            None
        } else {
            Some(s.trim_matches('\'').to_string())
        }
    }
    let Some(mode) = gs("org.gnome.system.proxy", "mode") else {
        return (None, None);
    };
    if mode != "manual" {
        return (None, None); // none / auto(PAC，不支持)
    }
    let http = gs("org.gnome.system.proxy.http", "host").map(|h| {
        let port = gs("org.gnome.system.proxy.http", "port").unwrap_or_else(|| "8080".into());
        format!("http://{h}:{port}")
    });
    let https = gs("org.gnome.system.proxy.https", "host").map(|h| {
        let port = gs("org.gnome.system.proxy.https", "port").unwrap_or_else(|| "8080".into());
        format!("http://{h}:{port}")
    });
    if http.is_none() && https.is_none() {
        return (None, None);
    }
    (http, https)
}

/// 当前网络代理模式（persist 配置；缺省 Auto）。
fn current_network_mode() -> NetworkMode {
    downloader_config().network_mode
}

/// 按网络模式给 ClientBuilder 应用代理：Direct 强制直连（TUN 由网络层接管）；Auto/System 检测并应用系统代理（无代理时直连）。
pub(crate) fn apply_system_proxy(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    match current_network_mode() {
        NetworkMode::Direct => builder.no_proxy(),
        NetworkMode::Auto | NetworkMode::System => {
            let (http_proxy, https_proxy) = system_proxy_urls();
            let mut b = builder;
            let no_proxy = reqwest::NoProxy::from_string("localhost,127.0.0.1,[::1]");
            if let Some(u) = http_proxy {
                if let Ok(p) = reqwest::Proxy::http(&u) {
                    eprintln!("[fetch] 应用系统代理 http: {u}");
                    b = b.proxy(p.no_proxy(no_proxy.clone()));
                }
            }
            if let Some(u) = https_proxy {
                if let Ok(p) = reqwest::Proxy::https(&u) {
                    eprintln!("[fetch] 应用系统代理 https: {u}");
                    b = b.proxy(p.no_proxy(no_proxy));
                }
            }
            b
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloader::fetcher::http;

    #[test]
    fn parse_proxy_server_variants() {
        let (h, s) = parse_proxy_server("127.0.0.1:7897");
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7897"));
        assert_eq!(s.as_deref(), Some("http://127.0.0.1:7897"));
        let (h, s) = parse_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891");
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(s.as_deref(), Some("http://127.0.0.1:7891"));
        let (h, s) = parse_proxy_server("http=127.0.0.1:7890");
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(s, None);
        let (h, s) = parse_proxy_server("");
        assert_eq!(h, None);
        assert_eq!(s, None);
        let (h, s) = parse_proxy_server("  ");
        assert_eq!(h, None);
        assert_eq!(s, None);
    }

    #[test]
    fn env_proxy_detects_http_and_https() {
        let prev_https = std::env::var("https_proxy").ok();
        let prev_http = std::env::var("http_proxy").ok();
        std::env::set_var("https_proxy", "http://127.0.0.1:7890");
        std::env::set_var("http_proxy", "http://127.0.0.1:7891");
        let (h, s) = env_proxy_urls();
        assert_eq!(h.as_deref(), Some("http://127.0.0.1:7891"));
        assert_eq!(s.as_deref(), Some("http://127.0.0.1:7890"));
        match prev_https {
            Some(v) => std::env::set_var("https_proxy", v),
            None => std::env::remove_var("https_proxy"),
        }
        match prev_http {
            Some(v) => std::env::set_var("http_proxy", v),
            None => std::env::remove_var("http_proxy"),
        }
    }

    #[tokio::test]
    async fn global_client_uses_system_proxy_when_enabled() {
        if current_network_mode() == NetworkMode::Direct {
            return;
        }
        let (h, s) = system_proxy_urls();
        if h.is_none() && s.is_none() {
            return; // 未检测到系统代理：不依赖代理环境
        }
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            http()
                .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
                .send(),
        )
        .await
        .expect("经系统代理访问官方应在 15s 内返回（代理不可达时检查代理配置）");
        let resp = res.expect("请求应成功");
        assert!(resp.status().is_success(), "官方应可达: {}", resp.status());
    }
}
