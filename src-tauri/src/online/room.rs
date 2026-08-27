//! 房间码：UUID → Base32 短码，HMAC 派生网络名与密钥，可携带入口端口与 host 局域网 IP。
//!
//! 设计：房间码即虚拟局域网的身份凭据，分享给朋友即可加入同一虚拟网。
//! - 码本身是 UUID 前 80bit 的 Base32 短码（`U/XXXX-XXXX-XXXX-XXXX`，
//!   16 字符 × 5bit = 80bit，剔易混字符），80bit 随机碰撞概率足够低；
//! - 网络名与密钥由 **HMAC(固定盐, 身份段)** 派生（端口/IP 不参与派生，
//!   host 可先建网、检测到游戏端口后再补全房间码展示）；
//! - 可选端口段 `:PORT`：host 开联机时把入口端口编入房间码，
//!   guest 解析房间码即可获得连接端口，无需手动填写；
//! - 可选局域网段 `@IP`：host 的局域网 IPv4（同网直连用，绕开协调节点；
//!   easytier 官方公共节点已下线，同网场景必须靠直连互发现）；
//! - 非法字符/格式本地拒绝，避免无效连接。

use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

/// 码字符数（不含 `U/` 与分隔符）：16 字符 × 5bit = 80bit。
const CODE_CHARS: usize = 16;
const SEGMENT_LEN: usize = 4;
/// 字符集：Crockford Base32（恰好 32 字符，剔 I/L/O/U；保留 0/1 靠 char_value 容错）。
const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 派生盐（HMAC 密钥用）。固定串，仅作区分"net"与"secret"两类派生的域分隔。
const NET_SALT: &[u8] = b"elytra-online-net-v1";
const SECRET_SALT: &[u8] = b"elytra-online-secret-v1";

/// 生成新房间码（纯身份，无端口段）。
pub fn create_room() -> RoomCredential {
    RoomCredential::derive(Uuid::new_v4(), None, None)
}

/// 生成新房间码并携带入口端口（仅测试用；正式流程 host 用 create_room + with_port）。
#[cfg(test)]
pub fn create_room_with_port(port: u16) -> RoomCredential {
    RoomCredential::derive(Uuid::new_v4(), Some(port), None)
}

/// 解析房间码文本（`U/XXXX-XXXX-XXXX-XXXX`、带 `:PORT` 或带 `@IP`）。非法/校验失败返回 None。
pub fn parse_room_code(code: &str) -> Option<RoomCredential> {
    let code = code.trim().to_ascii_uppercase();
    let body = code.strip_prefix("U/")?;
    // 分离可选局域网段：`@IP` 之后的部分必须是合法 IPv4
    let (port_part, lan_ip) = match body.split_once('@') {
        Some((rest, ip)) => {
            let ip = ip.parse::<std::net::Ipv4Addr>().ok()?.to_string();
            (rest, Some(ip))
        }
        None => (body, None),
    };
    // 分离可选端口段：有端口段但解析失败 → 整体非法
    let (identity, port) = match port_part.split_once(':') {
        Some((id, port_str)) => (id, Some(port_str.parse::<u16>().ok()?)),
        None => (port_part, None),
    };
    // 期望格式：4 段 × 4 字符，段间 '-'
    let mut bytes = [0u8; 10];
    let mut bit_pos: usize = 0;
    for (i, ch) in identity.chars().enumerate() {
        // 跳过段间分隔符
        if i > 0 && i % (SEGMENT_LEN + 1) == SEGMENT_LEN {
            if ch != '-' {
                return None;
            }
            continue;
        }
        let v = char_value(ch)? as usize;
        for b in 0..5 {
            let bit = (v >> (4 - b)) & 1;
            let byte_idx = bit_pos / 8;
            if byte_idx >= 10 {
                return None;
            }
            let shift = 7 - (bit_pos % 8);
            bytes[byte_idx] |= (bit as u8) << shift;
            bit_pos += 1;
        }
    }
    if bit_pos < CODE_CHARS * 5 {
        return None;
    }
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes[..10].copy_from_slice(&bytes);
    let uuid = Uuid::from_bytes(uuid_bytes);
    // 往返校验：重编码身份部分应一致
    let expect = encode_uuid_identity(&uuid);
    if port_part.split(':').next().unwrap_or("") != expect {
        return None;
    }
    Some(RoomCredential::derive(uuid, port, lan_ip))
}

/// 把 UUID 前 80bit 编码为身份短码（无端口段）。
fn encode_uuid_identity(uuid: &Uuid) -> String {
    let bytes = uuid.as_bytes();
    let mut out = String::with_capacity(CODE_CHARS + (CODE_CHARS / SEGMENT_LEN - 1));
    let mut bit_pos: usize = 0;
    let mut emitted = 0usize;
    while emitted < CODE_CHARS {
        let mut v: u8 = 0;
        for b in 0..5 {
            let bit_idx = bit_pos + b;
            let byte = bit_idx / 8;
            let shift = 7 - (bit_idx % 8);
            let bit = if byte < 10 {
                (bytes[byte] >> shift) & 1
            } else {
                0
            };
            v = (v << 1) | bit;
        }
        if emitted > 0 && emitted.is_multiple_of(SEGMENT_LEN) {
            out.push('-');
        }
        out.push(ALPHABET[v as usize] as char);
        bit_pos += 5;
        emitted += 1;
    }
    out
}

/// 单字符 → 5bit 值。Crockford 容错：0/O/o → 值0，1/I/L/l → 值1。
fn char_value(c: char) -> Option<u8> {
    match c {
        '0' | 'O' | 'o' => Some(0),
        '1' | 'I' | 'L' | 'l' => Some(1),
        c => ALPHABET
            .iter()
            .position(|&a| a as char == c)
            .map(|p| p as u8),
    }
}

/// HMAC-SHA256 派生助手：取前 16 字节 → hex 32 字符。
fn derive(tag: &[u8], code: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(tag).expect("hmac key ok");
    mac.update(code.as_bytes());
    let result = mac.finalize().into_bytes();
    result[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// 房间凭据：房间码 + 派生的网络名与密钥（供 easytier 建网）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomCredential {
    pub code: String,
    pub network_name: String,
    pub network_secret: String,
    /// host 入口端口（guest 连 host虚拟IP:此端口；编入房间码）。
    /// 与游戏端口解耦：host 侧由 mod.rs 探测空闲端口，避免与游戏监听冲突。
    pub port: Option<u16>,
    /// host 局域网 IPv4（同网直连用；编入房间码可选段 `@IP`）。
    /// easytier 官方公共协调节点已下线，同网场景靠 guest 直连 host 互发现。
    pub lan_ip: Option<String>,
}

impl RoomCredential {
    fn derive(uuid: Uuid, port: Option<u16>, lan_ip: Option<String>) -> Self {
        let identity = encode_uuid_identity(&uuid);
        // HMAC 只基于身份段（不含端口/IP）：host 可先建网后补端口/IP，
        // 网名/密钥在等待端口期间保持不变（实例无需换身份）。
        let mut code = format!("U/{identity}");
        if let Some(p) = port {
            code.push_str(&format!(":{p}"));
        }
        if let Some(ip) = &lan_ip {
            code.push_str(&format!("@{ip}"));
        }
        Self {
            network_name: derive(NET_SALT, &identity),
            network_secret: derive(SECRET_SALT, &identity),
            code,
            port,
            lan_ip,
        }
    }
    /// 在已有凭据上补端口：仅更新展示码与 port 字段；
    /// 网名/密钥不变（派生只吃身份段），运行中的虚拟网身份不受影响。
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self.code = Self::encode_code(&self);
        self
    }
    /// 在已有凭据上补局域网 IP：仅更新展示码与 lan_ip 字段；
    /// 网名/密钥不变（派生只吃身份段），运行中的虚拟网身份不受影响。
    pub fn with_lan_ip(mut self, lan_ip: String) -> Self {
        self.lan_ip = Some(lan_ip);
        self.code = Self::encode_code(&self);
        self
    }
    /// 按当前身份/端口/IP 重建展示码（身份段从 code 中剥离）。
    fn encode_code(room: &Self) -> String {
        let body = room.code.trim_start_matches("U/");
        let identity = body.split_once(":").map(|(id, _)| id).unwrap_or(body);
        let identity = identity.split_once("@").map(|(id, _)| id).unwrap_or(identity);
        let mut code = format!("U/{identity}");
        if let Some(p) = room.port {
            code.push_str(&format!(":{p}"));
        }
        if let Some(ip) = &room.lan_ip {
            code.push_str(&format!("@{ip}"));
        }
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_roundtrip_generates_parseable_code() {
        let room = create_room();
        let parsed = parse_room_code(&room.code).expect("生成的房间码应可解析");
        assert_eq!(parsed.code, room.code, "重编码应一致");
        assert_eq!(parsed.network_name, room.network_name, "网络名派生应一致");
        assert_eq!(parsed.network_secret, room.network_secret, "密钥派生应一致");
        assert_eq!(parsed.port, None, "无端口场景");
        assert_eq!(parsed.lan_ip, None, "无局域网 IP 场景");
    }

    #[test]
    fn room_with_port_roundtrip() {
        let room = create_room_with_port(25565);
        let parsed = parse_room_code(&room.code).expect("带端口房间码应可解析");
        assert_eq!(parsed.code, room.code);
        assert_eq!(parsed.port, Some(25565), "端口应解析出来");
        assert_eq!(parsed.network_name, room.network_name);
        assert_eq!(parsed.network_secret, room.network_secret);
    }

    #[test]
    fn room_with_lan_ip_roundtrip() {
        let room = create_room_with_port(25565).with_lan_ip("192.168.1.2".to_string());
        assert!(room.code.contains("@192.168.1.2"), "局域网段应存在");
        let parsed = parse_room_code(&room.code).expect("带局域网段房间码应可解析");
        assert_eq!(parsed.code, room.code, "重编码应一致");
        assert_eq!(parsed.port, Some(25565), "端口应保留");
        assert_eq!(
            parsed.lan_ip.as_deref(),
            Some("192.168.1.2"),
            "局域网 IP 应解析出来"
        );
        assert_eq!(parsed.network_name, room.network_name, "网名派生不受 IP 影响");
        assert_eq!(parsed.network_secret, room.network_secret);
    }

    #[test]
    fn room_with_lan_ip_only_roundtrip() {
        let room = create_room().with_lan_ip("10.0.0.5".to_string());
        let parsed = parse_room_code(&room.code).expect("仅局域网段房间码应可解析");
        assert_eq!(parsed.code, room.code);
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.lan_ip.as_deref(), Some("10.0.0.5"));
    }

    #[test]
    fn room_code_format_and_no_ambiguous_chars() {
        let room = create_room_with_port(50000);
        let body = room.code.strip_prefix("U/").unwrap();
        let identity = body.split(':').next().unwrap();
        assert_eq!(
            identity.len(),
            CODE_CHARS + (CODE_CHARS / SEGMENT_LEN - 1),
            "4 段 × 4 字符 + 3 分隔符"
        );
        assert!(room.code.contains(":50000"), "端口段应存在");
        for ch in identity.chars().filter(|c| *c != '-') {
            assert!(
                !matches!(ch, 'I' | 'L' | 'O' | 'U'),
                "生成码含易混字符 {ch}"
            );
        }
        let segs: Vec<&str> = identity.split('-').collect();
        assert_eq!(segs.len(), 4);
        assert!(segs.iter().all(|s| s.len() == 4));
    }

    #[test]
    fn invalid_codes_rejected() {
        assert!(parse_room_code("").is_none());
        assert!(parse_room_code("XXXX").is_none());
        assert!(parse_room_code("U/1234").is_none());
        assert!(parse_room_code("bad/XXXX-XXXX-XXXX-XXXX").is_none());
        assert!(parse_room_code("U/ABCD-EFGH-JKMN-PQRSX").is_none());
        assert!(parse_room_code("U/ABCD-EFGH-JKMN-PQ^S").is_none());
        // 非法端口段
        assert!(parse_room_code("U/ABCD-EFGH-JKMN-PQRS:abc").is_none());
        assert!(parse_room_code("U/ABCD-EFGH-JKMN-PQRS:99999").is_none());
        // 非法局域网段
        assert!(parse_room_code("U/ABCD-EFGH-JKMN-PQRS@999.1.1.1").is_none());
        assert!(parse_room_code("U/ABCD-EFGH-JKMN-PQRS@not-an-ip").is_none());
    }

    #[test]
    fn ambiguous_char_tolerance() {
        let room = create_room_with_port(25565);
        let tolerant = room.code.replace('O', "0").replace('I', "1");
        if tolerant != room.code {
            let parsed = parse_room_code(&tolerant).expect("容错字符应可解析");
            assert_eq!(parsed.code, room.code);
        }
    }
}
