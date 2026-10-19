//! machine_id：跨会话稳定的设备唯一标识（协议 c:player_ping / c:player_profiles_list 用）。
//!
//! 实现为首次生成后持久化的 16 字节随机 hex（满足"对同一设备稳定"，规避跨平台硬件指纹复杂度）；
//! 存于 exe 旁 `Elytra/machine-id`。崩溃/重启/重装保留 → 不变；删除文件 → 重新生成（视为新设备，协议允许）。

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use getrandom::getrandom;

/// machine-id 文件名（exe 旁 Elytra 配置目录下）。
const MACHINE_ID_FILE: &str = "machine-id";

pub fn get_machine_id() -> String {
    let dir = crate::util::config_dir();
    let path = dir.join(MACHINE_ID_FILE);
    load_or_create(&path)
}

/// 读取 / 创建 machine-id 文件（单测可注入路径）。
fn load_or_create(path: &PathBuf) -> String {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[online] machine-id 文件打开失败: {e}");
            return generate();
        }
    };
    let mut buf = [0u8; 16];
    match file.read(&mut buf) {
        Ok(16) => return hex_encode(&buf),
        Ok(n) => {
            // 文件损坏/不足 → 重新生成
            if n != 0 {
                eprintln!("[online] machine-id 文件损坏（{n} 字节），重新生成");
            }
        }
        Err(e) => {
            eprintln!("[online] machine-id 读取失败: {e}");
            return generate();
        }
    }
    // 生成并写回（seek 到开头，保留旧文件截断）
    let bytes = generate_bytes();
    if let Err(e) = file
        .seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(&bytes))
    {
        eprintln!("[online] machine-id 写入失败: {e}");
    }
    hex_encode(&bytes)
}

fn generate_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if getrandom(&mut bytes).is_err() {
        // 系统随机源不可用（极端环境）：回退时间戳填充（仅保证长度，不保证强随机）
        eprintln!("[online] 系统随机源不可用，machine_id 回退时间戳");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&now.to_be_bytes());
    }
    bytes
}

fn generate() -> String {
    hex_encode(&generate_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_32_hex() {
        let id = generate();
        assert_eq!(id.len(), 32, "16 字节 → 32 hex");
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn machine_id_stable_across_calls() {
        // 临时文件：两次读取应一致（首次生成后持久化）
        let dir = std::env::temp_dir().join(format!("elytra-mid-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(MACHINE_ID_FILE);
        let first = load_or_create(&path);
        let second = load_or_create(&path);
        assert_eq!(first, second, "同一文件多次读取应稳定");
        assert_eq!(first.len(), 32);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
