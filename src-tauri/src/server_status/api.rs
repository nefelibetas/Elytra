//! SLP / RakNet 协议探测实现（公共接口由父模块 `mod.rs` 重导出）。

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::select_ok;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

/// 单次探测总超时（连接 + 读写）。
const PING_TIMEOUT: Duration = Duration::from_secs(6);
/// TCP 建连 / DNS 解析超时。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// 基岩版单地址等待 PONG 的超时（多地址时逐个尝试）。
const BEDROCK_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);
/// status 响应 JSON 上限（含 favicon base64，256KB 足够，防恶意超大响应）。
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
/// 协议文本字段（地址等）最大长度。
const MAX_STRING: usize = 32767;
/// 玩家样例最多返回条数（防超大列表）。
const MAX_SAMPLE: usize = 12;
/// Java SLP 默认端口。
pub const JAVA_DEFAULT_PORT: u16 = 25565;
/// Bedrock 默认端口。
pub const BEDROCK_DEFAULT_PORT: u16 = 19132;
/// RakNet offline message magic（协议固定值）。
const RAKNET_MAGIC: [u8; 16] = [
    0x00, 0xFF, 0xFF, 0x00, 0xFE, 0xFE, 0xFE, 0xFE, 0xFD, 0xFD, 0xFD, 0xFD, 0x12, 0x34, 0x56, 0x78,
];
/// UNCONNECTED_PING 包 id。
const RAKNET_PING_ID: u8 = 0x01;
/// UNCONNECTED_PONG 包 id。
const RAKNET_PONG_ID: u8 = 0x1C;

/// 服务器状态（前端展示；serde 直出 JSON 给 JS）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerStatus {
    /// 版本类型："java" | "bedrock"。
    pub edition: String,
    /// 版本名（如 "1.21.4" / "1.21.44"）。
    pub version_name: String,
    /// Java：SLP 协议号；Bedrock：无可得 → None。
    pub protocol: Option<i32>,
    /// 在线玩家数。
    pub online_players: u32,
    /// 最大玩家数。
    pub max_players: u32,
    /// MOTD（§ 格式原始文本）。
    pub motd: String,
    /// 在线玩家样例（Bedrock 无此信息 → 空）。
    pub player_sample: Vec<String>,
    /// 服务器图标（"data:image/png;base64,..."，服务器未提供 → None）。
    pub favicon: Option<String>,
}

impl ServerStatus {
    fn java(
        version_name: String,
        protocol: Option<i32>,
        online: u32,
        max: u32,
        motd: String,
        sample: Vec<String>,
        favicon: Option<String>,
    ) -> Self {
        Self {
            edition: "java".to_string(),
            version_name,
            protocol,
            online_players: online,
            max_players: max,
            motd,
            player_sample: sample,
            favicon,
        }
    }

    fn bedrock(
        motd: String,
        protocol: Option<i32>,
        version_name: String,
        online: u32,
        max: u32,
    ) -> Self {
        Self {
            edition: "bedrock".to_string(),
            version_name,
            protocol,
            online_players: online,
            max_players: max,
            motd,
            player_sample: Vec::new(),
            favicon: None,
        }
    }
}

/// 解析 `host[:port]` / `[v6]:port`；无端口时用 `default_port`。
pub fn parse_address(input: &str, default_port: u16) -> Result<(String, u16), String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("地址不能为空".to_string());
    }
    // IPv6 字面量：[::1]:25565
    if let Some(rest) = input.strip_prefix('[') {
        let (host, port) = rest
            .split_once(']')
            .ok_or_else(|| format!("地址格式错误\n{input}"))?;
        if host.is_empty() {
            return Err("地址格式错误\nIPv6 主机段为空".to_string());
        }
        let port = match port.strip_prefix(':') {
            Some(p) => parse_port(p)?,
            None => default_port,
        };
        return Ok((host.to_string(), port));
    }
    // 裸 IPv6（含多个冒号）→ 整体当主机，默认端口
    if input.matches(':').count() > 1 {
        return Ok((input.to_string(), default_port));
    }
    match input.rsplit_once(':') {
        Some((host, p)) if !host.is_empty() => {
            let port = parse_port(p)?;
            Ok((host.to_string(), port))
        }
        Some((_, p)) => Err(format!("端口无效\n{p}")),
        None => Ok((input.to_string(), default_port)),
    }
}

fn parse_port(p: &str) -> Result<u16, String> {
    let port = p.parse::<u16>().map_err(|_| format!("端口无效\n{p}"))?;
    if port == 0 {
        return Err("端口无效\n0 不可用".to_string());
    }
    Ok(port)
}

/// 探测 Java 版服务器（SLP 1.7+）。失败返回中文错误（首行 + 技术细节）。
pub async fn ping_java(host: &str, port: u16) -> Result<ServerStatus, String> {
    ping_java_with_timeout(host, port, PING_TIMEOUT).await
}

async fn ping_java_with_timeout(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<ServerStatus, String> {
    tokio::time::timeout(timeout, async {
        let addrs = resolve_all(host, port).await?;
        let mut stream = connect_any(&addrs).await?;
        // 握手：外层 VarInt 长度 + packet 0x00 + protocol(-1) + host + port + next_state(1)
        let mut body = Vec::with_capacity(64);
        write_varint(0, &mut body);
        write_varint(-1, &mut body);
        write_string(host, &mut body)?;
        body.extend_from_slice(&port.to_be_bytes());
        write_varint(1, &mut body);
        let mut handshake = Vec::with_capacity(body.len() + 5);
        write_varint(body.len() as i32, &mut handshake);
        handshake.extend_from_slice(&body);
        stream
            .write_all(&handshake)
            .await
            .map_err(common_err_ctx!("发送握手失败"))?;
        // status 请求：长度 1 + packet id 0
        stream
            .write_all(&[0x01, 0x00])
            .await
            .map_err(common_err_ctx!("发送状态请求失败"))?;
        // 响应：外层包长度 + packet id + JSON 字符串
        let packet_len = read_varint(&mut stream).await?;
        if packet_len <= 0 || packet_len as usize > MAX_RESPONSE_BYTES + 16 {
            bail!("服务器响应异常\n包长度 {packet_len}");
        }
        let packet_id = read_varint(&mut stream).await?;
        if packet_id != 0 {
            bail!("服务器响应异常\n未知包 id {packet_id}");
        }
        let json = read_string(&mut stream, MAX_RESPONSE_BYTES).await?;
        parse_java_status(&json)
    })
    .await
    .map_err(|_| {
        "连接超时\n服务器无响应（检查地址与端口，服务器可能未开启或拒绝探测）".to_string()
    })?
}

/// 把 host 解析为全部地址（支持域名）。
async fn resolve_all(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    // 字面量 IP 直接构造，避免依赖系统解析
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(vec![SocketAddr::new(IpAddr::V4(ip), port)]);
    }
    if let Ok(ip) = host.parse::<Ipv6Addr>() {
        return Ok(vec![SocketAddr::new(IpAddr::V6(ip), port)]);
    }
    tokio::time::timeout(CONNECT_TIMEOUT, async {
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(common_err_ctx!("域名解析失败"))?
            .collect();
        if addrs.is_empty() {
            Err("域名解析失败\n无可用地址".to_string())
        } else {
            Ok(addrs)
        }
    })
    .await
    .map_err(|_| "域名解析超时".to_string())?
}

/// 并发尝试全部地址，取首个成功连接（首个地址不可达时不拖死整个探测）。
async fn connect_any(addrs: &[SocketAddr]) -> Result<TcpStream, String> {
    let attempts = addrs.iter().map(|addr| {
        Box::pin(TcpStream::connect(*addr))
            as Pin<Box<dyn Future<Output = std::io::Result<TcpStream>> + Send>>
    });
    select_ok(attempts)
        .await
        .map(|(stream, _)| stream)
        .map_err(common_err_ctx!("连接失败"))
}

/// 解析 SLP status JSON（宽松：description 兼容字符串/组件对象）。
fn parse_java_status(json: &str) -> Result<ServerStatus, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(common_err_ctx!("响应解析失败"))?;
    let version = value.get("version").and_then(|v| v.as_object());
    let version_name = version
        .and_then(|v| v.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("未知版本")
        .to_string();
    let protocol = version
        .and_then(|v| v.get("protocol"))
        .and_then(|p| p.as_i64())
        .map(|p| p.clamp(0, i32::MAX as i64) as i32);
    let players = value.get("players").and_then(|p| p.as_object());
    let online = players
        .and_then(|p| p.get("online"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0)
        .max(0) as u32;
    let max = players
        .and_then(|p| p.get("max"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0)
        .max(0) as u32;
    let motd = value
        .get("description")
        .map(|d| {
            let mut out = String::new();
            flatten_component(d, &mut out);
            out
        })
        .unwrap_or_default();
    let mut sample = Vec::new();
    if let Some(list) = players
        .and_then(|p| p.get("sample"))
        .and_then(|s| s.as_array())
    {
        for item in list.iter().take(MAX_SAMPLE) {
            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                sample.push(name.to_string());
            }
        }
    }
    let favicon = value
        .get("favicon")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());
    Ok(ServerStatus::java(
        version_name,
        protocol,
        online,
        max,
        motd,
        sample,
        favicon,
    ))
}

/// 递归提取聊天组件文本（description 可能是纯字符串或 {text, extra:[...]}）。
fn flatten_component(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Object(m) => {
            if let Some(text) = m.get("text").and_then(|t| t.as_str()) {
                out.push_str(text);
            }
            if let Some(extra) = m.get("extra").and_then(|e| e.as_array()) {
                for item in extra {
                    flatten_component(item, out);
                }
            }
        }
        _ => {}
    }
}

/// 探测基岩版服务器（RakNet UNCONNECTED_PING，UDP 一发一收）。
pub async fn ping_bedrock(host: &str, port: u16) -> Result<ServerStatus, String> {
    ping_bedrock_with_timeout(host, port, PING_TIMEOUT).await
}

async fn ping_bedrock_with_timeout(
    host: &str,
    port: u16,
    timeout: Duration,
) -> Result<ServerStatus, String> {
    tokio::time::timeout(timeout, async {
        let addrs = resolve_all(host, port).await?;
        let mut last_err = "连接超时\n服务器无响应（检查地址与端口，服务器可能未开启）".to_string();
        for addr in addrs {
            let bind = if addr.is_ipv4() {
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
            } else {
                SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)
            };
            let socket = UdpSocket::bind(bind)
                .await
                .map_err(common_err_ctx!("UDP 套接字创建失败"))?;
            if let Err(e) = socket.connect(addr).await {
                last_err = format!("连接失败\n{e}");
                continue;
            }
            // UNCONNECTED_PING：id + 时间戳 + magic + client GUID(0)
            let mut payload = Vec::with_capacity(32);
            payload.push(RAKNET_PING_ID);
            payload.extend_from_slice(&now_millis().to_be_bytes());
            payload.extend_from_slice(&RAKNET_MAGIC);
            payload.extend_from_slice(&0u64.to_be_bytes());
            socket
                .send(&payload)
                .await
                .map_err(common_err_ctx!("发送探测包失败"))?;
            let mut buf = [0u8; 2048];
            match tokio::time::timeout(BEDROCK_ATTEMPT_TIMEOUT, socket.recv(&mut buf)).await {
                Ok(Ok(n)) => return parse_raknet_pong(&buf[..n]),
                Ok(Err(e)) => last_err = format!("接收响应失败\n{e}"),
                Err(_) => last_err = "连接超时\n服务器无响应（检查地址与端口，服务器可能未开启）".to_string(),
            }
        }
        Err(last_err)
    })
    .await
    .map_err(|_| "连接超时\n服务器无响应（检查地址与端口，服务器可能未开启）".to_string())?
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 解析 UNCONNECTED_PONG 载荷：id(1) + time(8) + server_guid(8) + magic(16) + strlen(2 BE) + 字符串；
/// 字符串字段分号分隔：MOTD;protocol;version;online;max;server_id;level;...
fn parse_raknet_pong(data: &[u8]) -> Result<ServerStatus, String> {
    if data.len() < 1 + 8 + 8 + 16 + 2 {
        return Err("响应格式错误\n数据过短".to_string());
    }
    if data[0] != RAKNET_PONG_ID {
        bail!("响应格式错误\n未知包 id {}", data[0]);
    }
    let magic_start = 1 + 8 + 8;
    if data[magic_start..magic_start + 16] != RAKNET_MAGIC {
        return Err("响应格式错误\nmagic 不匹配（非 RakNet 服务器）".to_string());
    }
    let len_start = magic_start + 16;
    let len = u16::from_be_bytes([data[len_start], data[len_start + 1]]) as usize;
    let text_start = len_start + 2;
    if data.len() < text_start + len {
        return Err("响应格式错误\n字符串长度越界".to_string());
    }
    let text = String::from_utf8_lossy(&data[text_start..text_start + len]);
    let fields: Vec<&str> = text.split(';').collect();
    let motd = fields.first().copied().unwrap_or("").to_string();
    let protocol = fields.get(1).and_then(|f| f.parse::<i32>().ok());
    let version = fields.get(2).copied().unwrap_or("未知版本").to_string();
    let online = fields
        .get(3)
        .and_then(|f| f.parse::<u32>().ok())
        .unwrap_or(0);
    let max = fields
        .get(4)
        .and_then(|f| f.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(ServerStatus::bedrock(motd, protocol, version, online, max))
}

// ── VarInt / 字符串编解码（MC 协议）──

fn write_varint(v: i32, out: &mut Vec<u8>) {
    // 转 u32 逻辑右移：i32 算术右移对负数（如 -1）永不归零
    let mut v = v as u32;
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            break;
        }
    }
}

async fn read_varint<R: AsyncRead + Unpin>(r: &mut R) -> Result<i32, String> {
    let mut result: i32 = 0;
    for i in 0..5 {
        let mut byte = [0u8; 1];
        r.read_exact(&mut byte)
            .await
            .map_err(common_err_ctx!("读取数据失败"))?;
        let b = byte[0];
        result |= ((b & 0x7F) as i32) << (i * 7);
        if b & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err("读取数据失败\nVarInt 超过 5 字节".to_string())
}

fn write_string(s: &str, out: &mut Vec<u8>) -> Result<(), String> {
    if s.len() > MAX_STRING {
        bail!("文本过长\n{} 字节", s.len());
    }
    write_varint(s.len() as i32, out);
    out.extend_from_slice(s.as_bytes());
    Ok(())
}

async fn read_string<R: AsyncRead + Unpin>(r: &mut R, max: usize) -> Result<String, String> {
    let len = read_varint(r).await?;
    if len < 0 || len as usize > max {
        bail!("响应格式错误\n字符串长度异常 {len}");
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)
        .await
        .map_err(common_err_ctx!("读取数据失败"))?;
    String::from_utf8(buf).map_err(|e| format!("响应格式错误\n非 UTF-8 文本：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, UdpSocket};

    #[test]
    fn varint_roundtrip() {
        for v in [0, 1, 127, 128, 255, 300, 32767, i32::MAX, -1, -100, i32::MIN] {
            let mut buf = Vec::new();
            write_varint(v, &mut buf);
            // 手工解码校验（不依赖 read_varint 自身）
            let mut result: i32 = 0;
            for (i, &b) in buf.iter().enumerate() {
                result |= ((b & 0x7F) as i32) << (i * 7);
                if b & 0x80 == 0 {
                    break;
                }
            }
            assert_eq!(result, v, "varint 编解码不一致: {v}");
        }
    }

    #[tokio::test]
    async fn varint_read_stream() {
        let mut buf = Vec::new();
        write_varint(300, &mut buf);
        write_varint(-1, &mut buf);
        let mut cursor = &buf[..];
        assert_eq!(read_varint(&mut cursor).await.unwrap(), 300);
        assert_eq!(read_varint(&mut cursor).await.unwrap(), -1);
    }

    #[test]
    fn parse_address_variants() {
        assert_eq!(
            parse_address("play.example.com", 25565).unwrap(),
            ("play.example.com".to_string(), 25565)
        );
        assert_eq!(
            parse_address("1.2.3.4", 25565).unwrap(),
            ("1.2.3.4".to_string(), 25565)
        );
        assert_eq!(
            parse_address("1.2.3.4:19132", 25565).unwrap(),
            ("1.2.3.4".to_string(), 19132)
        );
        assert_eq!(
            parse_address("[::1]:25565", 25565).unwrap(),
            ("::1".to_string(), 25565)
        );
        assert_eq!(
            parse_address("[::1]", 25565).unwrap(),
            ("::1".to_string(), 25565)
        );
        assert_eq!(
            parse_address("2001:db8::1", 25565).unwrap(),
            ("2001:db8::1".to_string(), 25565)
        );
        assert!(parse_address("", 25565).is_err(), "空地址应报错");
        assert!(parse_address("1.2.3.4:abc", 25565).is_err(), "非法端口应报错");
        assert!(parse_address("1.2.3.4:0", 25565).is_err(), "端口 0 应报错");
        assert!(parse_address("[::1", 25565).is_err(), "未闭合 IPv6 应报错");
    }

    #[test]
    fn flatten_component_handles_all_forms() {
        // 纯字符串
        let v: serde_json::Value = serde_json::from_str("\"Hello §6World\"").unwrap();
        let mut out = String::new();
        flatten_component(&v, &mut out);
        assert_eq!(out, "Hello §6World");
        // 组件对象 + extra 嵌套
        let v: serde_json::Value =
            serde_json::from_str(r#"{"text":"A","extra":[{"text":"B"},{"text":"C","extra":[{"text":"D"}]}]}"#)
                .unwrap();
        let mut out = String::new();
        flatten_component(&v, &mut out);
        assert_eq!(out, "ABCD");
        // 非文本节点忽略
        let v: serde_json::Value = serde_json::from_str("42").unwrap();
        let mut out = String::new();
        flatten_component(&v, &mut out);
        assert_eq!(out, "");
    }

    #[test]
    fn parse_java_status_full() {
        let json = r#"{
            "version": {"name": "1.21.4", "protocol": 769},
            "players": {"max": 100, "online": 3, "sample": [{"name": "Alice", "id": "a"}, {"name": "Bob", "id": "b"}]},
            "description": {"text": "§6欢迎", "extra": [{"text": "!"}]},
            "favicon": "data:image/png;base64,AAAA"
        }"#;
        let s = parse_java_status(json).unwrap();
        assert_eq!(s.edition, "java");
        assert_eq!(s.version_name, "1.21.4");
        assert_eq!(s.protocol, Some(769));
        assert_eq!(s.online_players, 3);
        assert_eq!(s.max_players, 100);
        assert_eq!(s.motd, "§6欢迎!");
        assert_eq!(s.player_sample, vec!["Alice".to_string(), "Bob".to_string()]);
        assert_eq!(s.favicon.as_deref(), Some("data:image/png;base64,AAAA"));
    }

    #[test]
    fn parse_java_status_missing_fields() {
        let s = parse_java_status(r#"{"version": {}, "players": {}}"#).unwrap();
        assert_eq!(s.version_name, "未知版本");
        assert_eq!(s.protocol, None);
        assert_eq!(s.online_players, 0);
        assert_eq!(s.max_players, 0);
        assert_eq!(s.motd, "");
        assert!(s.player_sample.is_empty());
        assert_eq!(s.favicon, None);
    }

    #[test]
    fn parse_java_status_bad_json() {
        assert!(parse_java_status("not json").is_err());
    }

    #[test]
    fn parse_raknet_pong_full() {
        let mut data = Vec::new();
        data.push(RAKNET_PONG_ID);
        data.extend_from_slice(&123u64.to_be_bytes());
        data.extend_from_slice(&456u64.to_be_bytes());
        data.extend_from_slice(&RAKNET_MAGIC);
        let text = "Elytra 联机;671;1.21.44;2;20;server_id;world;Survival;1;19132;19133";
        data.extend_from_slice(&(text.len() as u16).to_be_bytes());
        data.extend_from_slice(text.as_bytes());
        let s = parse_raknet_pong(&data).unwrap();
        assert_eq!(s.edition, "bedrock");
        assert_eq!(s.motd, "Elytra 联机");
        assert_eq!(s.protocol, Some(671));
        assert_eq!(s.version_name, "1.21.44");
        assert_eq!(s.online_players, 2);
        assert_eq!(s.max_players, 20);
        assert!(s.player_sample.is_empty());
    }

    #[test]
    fn parse_raknet_pong_short_field_fallback() {
        let mut data = Vec::new();
        data.push(RAKNET_PONG_ID);
        data.extend_from_slice(&0u64.to_be_bytes());
        data.extend_from_slice(&0u64.to_be_bytes());
        data.extend_from_slice(&RAKNET_MAGIC);
        let text = "motd";
        data.extend_from_slice(&(text.len() as u16).to_be_bytes());
        data.extend_from_slice(text.as_bytes());
        let s = parse_raknet_pong(&data).unwrap();
        assert_eq!(s.motd, "motd");
        assert_eq!(s.version_name, "未知版本");
        assert_eq!(s.online_players, 0);
        assert_eq!(s.max_players, 0);
    }

    #[test]
    fn parse_raknet_pong_rejects_bad_magic() {
        let mut data = Vec::new();
        data.push(RAKNET_PONG_ID);
        data.extend_from_slice(&0u64.to_be_bytes());
        data.extend_from_slice(&0u64.to_be_bytes());
        data.extend_from_slice(&[0u8; 16]);
        let text = "x";
        data.extend_from_slice(&(text.len() as u16).to_be_bytes());
        data.extend_from_slice(text.as_bytes());
        assert!(parse_raknet_pong(&data).is_err(), "magic 不匹配应报错");
    }

    /// 假 Java 服务器全链路：本地 listener 按 SLP 回话，验证 ping_java 解析。
    #[tokio::test]
    async fn ping_java_end_to_end_with_fake_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // 握手：外层长度 + packet 0x00 + 协议版本/地址/端口/next state
            let handshake_len = read_varint(&mut stream).await.unwrap();
            let mut handshake = vec![0u8; handshake_len as usize];
            stream.read_exact(&mut handshake).await.unwrap();
            let mut body = &handshake[..];
            assert_eq!(read_varint(&mut body).await.unwrap(), 0);
            let _protocol = read_varint(&mut body).await.unwrap();
            let _host = read_string(&mut body, MAX_STRING).await.unwrap();
            let mut port_buf = [0u8; 2];
            body.read_exact(&mut port_buf).await.unwrap();
            assert_eq!(read_varint(&mut body).await.unwrap(), 1, "next state 应为 1");
            assert!(body.is_empty(), "握手包长度应与内容一致");
            // status 请求：长度 1 + packet id 0
            assert_eq!(read_varint(&mut stream).await.unwrap(), 1);
            assert_eq!(read_varint(&mut stream).await.unwrap(), 0);
            // 响应：外层长度 + packet id + JSON 字符串
            let json = r#"{"version":{"name":"1.21.4","protocol":769},"players":{"max":10,"online":1,"sample":[{"name":"Tester","id":"t"}]},"description":{"text":"§a测试服"}}"#;
            let mut payload = Vec::new();
            write_varint(0, &mut payload);
            write_varint(json.len() as i32, &mut payload);
            payload.extend_from_slice(json.as_bytes());
            let mut out = Vec::new();
            write_varint(payload.len() as i32, &mut out);
            out.extend_from_slice(&payload);
            stream.write_all(&out).await.unwrap();
        });
        let status = ping_java("127.0.0.1", addr.port()).await.unwrap();
        server.await.unwrap();
        assert_eq!(status.edition, "java");
        assert_eq!(status.version_name, "1.21.4");
        assert_eq!(status.online_players, 1);
        assert_eq!(status.max_players, 10);
        assert_eq!(status.motd, "§a测试服");
        assert_eq!(status.player_sample, vec!["Tester".to_string()]);
    }

    /// 假基岩版服务器全链路：UDP 回 PONG，验证 ping_bedrock 解析。
    #[tokio::test]
    async fn ping_bedrock_end_to_end_with_fake_server() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let (_, peer) = socket.recv_from(&mut buf).await.unwrap();
            assert_eq!(buf[0], RAKNET_PING_ID, "收到 UNCONNECTED_PING");
            assert_eq!(&buf[9..25], &RAKNET_MAGIC, "magic 校验");
            let time = &buf[1..9];
            let text = "Bedrock 测试服;671;1.21.44;5;50;sid;world;Survival;1;19132;19133";
            let mut out = Vec::new();
            out.push(RAKNET_PONG_ID);
            out.extend_from_slice(time);
            out.extend_from_slice(&7u64.to_be_bytes());
            out.extend_from_slice(&RAKNET_MAGIC);
            out.extend_from_slice(&(text.len() as u16).to_be_bytes());
            out.extend_from_slice(text.as_bytes());
            let _ = socket.send_to(&out, peer).await.unwrap();
        });
        let status = ping_bedrock("127.0.0.1", addr.port()).await.unwrap();
        server.await.unwrap();
        assert_eq!(status.edition, "bedrock");
        assert_eq!(status.motd, "Bedrock 测试服");
        assert_eq!(status.version_name, "1.21.44");
        assert_eq!(status.online_players, 5);
        assert_eq!(status.max_players, 50);
    }

    /// 无监听端口 → 连接失败（快速失败路径）。
    #[tokio::test]
    async fn ping_java_refuses_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // 端口释放后无人监听
        let err = ping_java("127.0.0.1", addr.port()).await.unwrap_err();
        assert!(err.contains("连接失败"), "应报连接失败，实际: {err}");
    }

    /// 端口无人应答（UDP 静默丢弃）→ 短超时快速返回错误。
    #[tokio::test]
    async fn ping_bedrock_no_response_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = ping_bedrock_with_timeout("127.0.0.1", addr.port(), Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(
            err.contains("超时") || err.contains("失败"),
            "应超时/失败，实际: {err}"
        );
    }
}
