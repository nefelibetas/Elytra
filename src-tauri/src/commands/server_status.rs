//! 服务器状态检测命令（薄胶水）：探测 Java/基岩版 Minecraft 服务器的在线状态。
//! 协议实现在 crate::server_status（SLP / RakNet），此处仅解析参数 + 错误展平。

use crate::server_status::{
    self, BEDROCK_DEFAULT_PORT, JAVA_DEFAULT_PORT, ServerStatus,
};

/// 探测服务器状态。
///
/// `edition`：`"java"` / `"bedrock"` / 空（自动：先试 Java，失败再试基岩版）。
/// 自动模式下：地址带显式端口时两版共用该端口，否则用各自默认端口
/// （Java 25565 / Bedrock 19132）。
#[tauri::command]
pub async fn ping_server(address: String, edition: Option<String>) -> Result<ServerStatus, String> {
    let edition = edition.unwrap_or_default();
    match edition.as_str() {
        "bedrock" => {
            let (host, port) = server_status::parse_address(&address, BEDROCK_DEFAULT_PORT)?;
            server_status::ping_bedrock(&host, port).await
        }
        "java" => {
            let (host, port) = server_status::parse_address(&address, JAVA_DEFAULT_PORT)?;
            server_status::ping_java(&host, port).await
        }
        _ => {
            // 自动：先 Java 后基岩版。显式端口两版共用，默认端口各自。
            let explicit_port = parse_explicit_port(&address);
            let (host, _) = server_status::parse_address(&address, JAVA_DEFAULT_PORT)?;
            let java_port = explicit_port.unwrap_or(JAVA_DEFAULT_PORT);
            match server_status::ping_java(&host, java_port).await {
                Ok(status) => Ok(status),
                Err(java_err) => {
                    let bedrock_port = explicit_port.unwrap_or(BEDROCK_DEFAULT_PORT);
                    match server_status::ping_bedrock(&host, bedrock_port).await {
                        Ok(status) => Ok(status),
                        Err(bedrock_err) => Err(format!(
                            "服务器无响应\nJava 版：{java_err}\n基岩版：{bedrock_err}"
                        )),
                    }
                }
            }
        }
    }
}

/// 地址是否带显式端口（`host:port` / `[v6]:port`）。
fn parse_explicit_port(address: &str) -> Option<u16> {
    let address = address.trim();
    if let Some(rest) = address.strip_prefix('[') {
        return rest
            .split_once(']')
            .and_then(|(_, port)| port.strip_prefix(':'))
            .and_then(|p| p.parse::<u16>().ok());
    }
    if address.matches(':').count() > 1 {
        return None; // 裸 IPv6，无端口
    }
    address
        .rsplit_once(':')
        .and_then(|(host, p)| (!host.is_empty()).then_some(p))
        .and_then(|p| p.parse::<u16>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_port_detection() {
        assert_eq!(parse_explicit_port("1.2.3.4:25565"), Some(25565));
        assert_eq!(parse_explicit_port("1.2.3.4"), None);
        assert_eq!(parse_explicit_port("[::1]:19132"), Some(19132));
        assert_eq!(parse_explicit_port("[::1]"), None);
        assert_eq!(parse_explicit_port("2001:db8::1"), None);
        assert_eq!(parse_explicit_port("host:abc"), None);
    }
}
