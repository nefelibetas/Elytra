//! 联机命令：逻辑在 online/ 领域层，此处仅转发。
//! host 可先开联机再在游戏内"对局域网开放"（后台自动检测端口），guest 从房间码解析端口。
use crate::config::online::OnlineConfig;
use crate::online::{self, OnlineSnapshot};
use crate::persist;
use crate::task::EventSink;
use std::sync::Arc;

/// 开启联机（host）：立即返回 Waiting 快照（房间码为身份段）；检测到游戏端口后经 `online-state` 事件推送 Hosting + 完整房间码。
#[tauri::command]
pub async fn online_start_host(app: tauri::AppHandle) -> Result<OnlineSnapshot, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    online::start_host(sink).await
}

/// 加入联机（作为 guest）。返回本地转发端口（游戏内填 127.0.0.1:此端口）。
#[tauri::command]
pub async fn online_join(app: tauri::AppHandle, room_code: String) -> Result<u16, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    online::join(sink, &room_code).await
}

#[tauri::command]
pub async fn online_stop(app: tauri::AppHandle) -> Result<(), String> {
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    online::stop(sink.as_ref()).await
}

#[tauri::command]
pub async fn online_status() -> OnlineSnapshot {
    online::status().await
}

#[tauri::command]
pub fn get_online_config() -> OnlineConfig {
    persist::online_config()
}

#[tauri::command]
pub fn save_online_config(config: OnlineConfig) -> Result<(), String> {
    persist::save_online_config(&config).map_err(common_err_ctx_key!(
        "errors.config.write",
        "保存联机配置失败"
    ))
}

/// 节点检查结果（status 为固定枚举串，前端本地化；detail 只放技术细节）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeCheckResult {
    pub node: String,
    /// ok / unreachable / invalid / unchecked / not_node
    pub status: &'static str,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

fn node_failure(node: String, status: &'static str, detail: impl Into<String>) -> NodeCheckResult {
    NodeCheckResult {
        node,
        status,
        latency_ms: None,
        detail: Some(detail.into()),
    }
}

/// 检查协调节点：格式 + TCP 可达性；UDP/TXT 等无法在不建网的情况下可靠探测。
/// 保持输入顺序返回（并发检查，结果按原索引回填）。
#[tauri::command]
pub async fn check_online_nodes(nodes: Vec<String>) -> Vec<NodeCheckResult> {
    let mut results: Vec<Option<NodeCheckResult>> = vec![None; nodes.len()];
    let mut set = tokio::task::JoinSet::new();
    for (index, node) in nodes.into_iter().enumerate() {
        set.spawn(async move { (index, check_online_node(node).await) });
    }
    while let Some(joined) = set.join_next().await {
        if let Ok((index, result)) = joined {
            results[index] = Some(result);
        }
    }
    results.into_iter().flatten().collect()
}

async fn check_online_node(node: String) -> NodeCheckResult {
    let trimmed = node.trim();
    if trimmed.is_empty() {
        return node_failure(node, "invalid", "empty");
    }
    let Ok(url) = url::Url::parse(trimmed) else {
        return node_failure(node, "invalid", "invalid url");
    };
    let scheme = url.scheme();
    let Some(host) = url.host_str().filter(|h| !h.is_empty()) else {
        return node_failure(node, "invalid", "missing host");
    };
    match scheme {
        "tcp" | "udp" | "ring" | "wg" | "ws" | "wss" | "http" | "https" | "quic" | "faketcp"
        | "txt" => {}
        other => {
            return node_failure(node, "invalid", format!("unsupported scheme: {other}"));
        }
    }
    // Web 控制台入口不是 EasyTier 节点地址。
    if matches!(scheme, "http" | "https" | "ws" | "wss") && url.path().contains("webconsole") {
        return node_failure(node, "not_node", "looks like a web console");
    }
    let default_port = match scheme {
        "tcp" => 11010,
        "ws" | "http" => 80,
        "wss" | "https" => 443,
        _ => 0,
    };
    if !matches!(scheme, "tcp" | "ws" | "wss" | "http" | "https") {
        return node_failure(
            node,
            "unchecked",
            "protocol cannot be probed without an EasyTier handshake",
        );
    }
    let port = url.port().unwrap_or(default_port);
    let started = std::time::Instant::now();
    let target = (host.to_owned(), port);
    match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(target),
    )
    .await
    {
        Ok(Ok(_)) => NodeCheckResult {
            node,
            status: "ok",
            latency_ms: Some(started.elapsed().as_millis() as u64),
            detail: None,
        },
        Ok(Err(e)) => node_failure(node, "unreachable", e.to_string()),
        Err(_) => node_failure(node, "unreachable", "timeout"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_invalid_and_console_urls() {
        assert_eq!(
            check_online_node("not a url".into()).await.status,
            "invalid"
        );
        assert_eq!(
            check_online_node("ftp://example.com".into()).await.status,
            "invalid"
        );
        assert_eq!(
            check_online_node("https://example.com/webconsole/".into())
                .await
                .status,
            "not_node"
        );
        assert_eq!(
            check_online_node("udp://example.com:11010".into())
                .await
                .status,
            "unchecked"
        );
    }
}
