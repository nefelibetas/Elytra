//! Scaffolding 房间码：`U/XXXX-XXXX-XXXX-XXXX`（34 字符集 + 模 7 校验），网络名/密钥由房间码**明文映射**（非 HMAC 派生）。
//!
//! 字符集 0-9、A-H、J-N、P-Z（剔 I/O）共 34 字符、映射 [0, 33]；按"小端序"读得的整型须能被 7 整除（防手输错/串台的校验码）；
//! 前 8 字符（`NNNN-NNNN`）→ 网络名 `scaffolding-mc-NNNN-NNNN`，后 8 字符（`SSSS-SSSS`）→ 网络密钥；
//! 生成 = 随机 [0, 34^16) 减到能被 7 整除再 base34 编码；字符 I/O 容错解析。

use getrandom::getrandom;

const CODE_CHARS: usize = 16;
const SEGMENT_LEN: usize = 4;

/// 字符集：34 字符（0-9、A-H、J-N、P-Z），映射 [0, 33]。
const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";

const NETWORK_PREFIX: &str = "scaffolding-mc-";

/// 生成新房间码（密码学随机 → 模 7 对齐）。随机源不可用时返回错误：
/// 静默失败会让 bytes 全零，房间码/网络密钥退化为可预测值。
pub fn create_room() -> Result<RoomCredential, String> {
    // 随机 16 字符 base34 值，减到能被 7 整除（保证校验通过）。
    let mut bytes = [0u8; 16];
    getrandom(&mut bytes).map_err(common_err_ctx!("系统随机源不可用"))?;
    let value = u128::from_be_bytes(bytes) % 34u128.pow(CODE_CHARS as u32);
    let value = value - value % 7;
    let (code, network_name, network_secret) = from_value(value);
    Ok(RoomCredential {
        code,
        network_name,
        network_secret,
    })
}

/// 解析房间码文本。非法格式 / 校验失败 / 非 `U/` 前缀 → None。
pub fn parse_room_code(code: &str) -> Option<RoomCredential> {
    let code = code.trim();
    let code: Vec<char> = code.to_ascii_uppercase().chars().collect();
    // 要求精确长度 `U/XXXX-XXXX-XXXX-XXXX`（Elytra 只接受完整码，不做 Terracotta 式滑动窗口）。
    if code.len() != 2 + CODE_CHARS + (CODE_CHARS / SEGMENT_LEN - 1) {
        return None;
    }
    if code[0] != 'U' || code[1] != '/' {
        return None;
    }
    let body = &code[2..];
    // 小端序（协议定义）：从最后一位字符开始，`value = value * 34 + v`。
    let mut value: u128 = 0;
    for (i, &ch) in body.iter().enumerate().rev() {
        // 段间分隔符位置：4、9、14
        if i == SEGMENT_LEN || i == 2 * SEGMENT_LEN + 1 || i == 3 * SEGMENT_LEN + 2 {
            if ch != '-' {
                return None;
            }
            continue;
        }
        let v = char_value(ch)? as u128;
        value = value * 34 + v;
    }
    if !value.is_multiple_of(7) {
        return None;
    }
    let (canonical, network_name, network_secret) = from_value(value);
    Some(RoomCredential {
        code: canonical,
        network_name,
        network_secret,
    })
}

/// 单字符 → [0,33] 值。I→1、O→0 容错（协议允许）。
fn char_value(c: char) -> Option<u8> {
    let c = match c {
        'I' => '1',
        'O' => '0',
        _ => c,
    };
    ALPHABET
        .iter()
        .position(|&a| a as char == c)
        .map(|p| p as u8)
}

/// value（< 34^16，能被 7 整除）→ 房间码 / 网络名（前 8 字符）/ 密钥（后 8 字符）。
fn from_value(mut value: u128) -> (String, String, String) {
    let mut code = String::with_capacity(2 + CODE_CHARS + 3);
    code.push_str("U/");
    let mut network_name = String::with_capacity(NETWORK_PREFIX.len() + 8 + 1);
    network_name.push_str(NETWORK_PREFIX);
    let mut network_secret = String::with_capacity(8 + 1);
    for i in 0..CODE_CHARS {
        let v = ALPHABET[(value % 34) as usize] as char;
        value /= 34;
        // 分隔符插在字符索引 4/8/12 之前 → 位置 4/9/14（与解析一致）
        if i == SEGMENT_LEN || i == 2 * SEGMENT_LEN || i == 3 * SEGMENT_LEN {
            code.push('-');
        }
        code.push(v);
        if i < 8 {
            if i == SEGMENT_LEN {
                network_name.push('-');
            }
            network_name.push(v);
        } else {
            if i == 12 {
                network_secret.push('-');
            }
            network_secret.push(v);
        }
    }
    debug_assert_eq!(value, 0, "value 应被完全消费");
    (code, network_name, network_secret)
}

/// 房间凭据：房间码 + 网络名/密钥（供 easytier 建网）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomCredential {
    pub code: String,
    pub network_name: String,
    pub network_secret: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_parses_roundtrip() {
        let room = create_room().unwrap();
        let parsed = parse_room_code(&room.code).expect("生成的房间码应可解析");
        assert_eq!(parsed.code, room.code, "重编码应一致");
        assert_eq!(parsed.network_name, room.network_name);
        assert_eq!(parsed.network_secret, room.network_secret);
    }

    #[test]
    fn code_format_is_valid() {
        let room = create_room().unwrap();
        let body = room.code.strip_prefix("U/").unwrap();
        assert_eq!(body.len(), CODE_CHARS + 3, "4 段 × 4 字符 + 3 分隔符");
        let segs: Vec<&str> = body.split('-').collect();
        assert_eq!(segs.len(), 4);
        assert!(segs.iter().all(|s| s.len() == SEGMENT_LEN));
        assert_eq!(
            room.network_name,
            format!("scaffolding-mc-{}-{}", segs[0], segs[1])
        );
        assert_eq!(room.network_secret, format!("{}-{}", segs[2], segs[3]));
    }

    #[test]
    fn modulus_seven_check() {
        let room = create_room().unwrap();
        let value = parse_code_value(&room.code).unwrap();
        assert_eq!(value % 7, 0, "房间码值应能被 7 整除");
    }

    #[test]
    fn invalid_codes_rejected() {
        assert!(parse_room_code("").is_none());
        assert!(parse_room_code("U/1234").is_none());
        assert!(
            parse_room_code("XXXX-XXXX-XXXX-XXXX").is_none(),
            "缺 U/ 前缀"
        );
        assert!(
            parse_room_code("U/ABCD-EFGH-JKMN-PQRS").is_none(),
            "长度不符"
        );
        assert!(
            parse_room_code("U/ABCD-EFGH-JKMN-PQRS:123").is_none(),
            "不允许附加段"
        );
        assert!(
            parse_room_code("U/ABCD-EFGH-JKMN-PQ^S").is_none(),
            "非法字符"
        );
        // 构造一个模 7 不为 0 的码：取合法码把首字符替换（大概率破坏校验）
        let mut bad = create_room().unwrap().code;
        let new = if bad.as_bytes()[2] == b'0' { "1" } else { "0" };
        // 替换首字符（码为 ASCII，索引 2..3 必在字符边界），重建字符串避免 unsafe
        bad.replace_range(2..3, new);
        // 替换后若仍合法则跳过（极小概率），否则应拒绝
        if !parse_room_value(&bad).is_multiple_of(7) {
            assert!(parse_room_code(&bad).is_none(), "模 7 校验应拒绝: {bad}");
        }
    }

    #[test]
    fn ambiguous_char_tolerance() {
        let room = create_room().unwrap();
        let tolerant = room.code.replace('O', "0").replace('I', "1");
        if tolerant != room.code {
            let parsed = parse_room_code(&tolerant).expect("容错字符应可解析");
            assert_eq!(parsed.code, room.code);
        }
    }

    #[test]
    fn lower_case_accepted() {
        let room = create_room().unwrap();
        let lower = room.code.to_ascii_lowercase();
        let parsed = parse_room_code(&lower).expect("小写应可解析");
        assert_eq!(parsed.code, room.code);
    }

    fn parse_code_value(code: &str) -> Option<u128> {
        let code: Vec<char> = code.trim().to_ascii_uppercase().chars().collect();
        if code.len() != 2 + CODE_CHARS + 3 || code[0] != 'U' || code[1] != '/' {
            return None;
        }
        let body = &code[2..];
        let mut value: u128 = 0;
        for (i, &ch) in body.iter().enumerate().rev() {
            if i == 4 || i == 9 || i == 14 {
                if ch != '-' {
                    return None;
                }
                continue;
            }
            value = value * 34 + char_value(ch)? as u128;
        }
        Some(value)
    }

    fn parse_room_value(code: &str) -> u128 {
        parse_code_value(code).expect("测试码应可解析")
    }
}
