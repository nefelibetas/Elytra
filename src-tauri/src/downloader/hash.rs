use sha1::Digest as _;
use std::io::Read;
use std::path::Path;

/// sha512 十六进制（mrpack files[].hashes.sha512 要求；Modrinth 校验用）。
pub fn sha512_hex(data: &[u8]) -> String {
    let hash = sha2::Sha512::digest(data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn sha1_hex(data: &[u8]) -> String {
    let hash = sha1::Sha1::digest(data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 空内容的 sha1：清单里零字节对象就用它声明（Mojang runtime 的 `lib/security/trusted.libraries` 即此）。
pub const SHA1_EMPTY: &str = "da39a3ee5e6b4b0d3255bfef95601890afd80709";

pub fn file_sha1_matches(path: &Path, expected: &str) -> bool {
    if expected.is_empty() {
        return path.exists();
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    sha1_hex(&data) == expected
}

/// 分块流式 sha1 校验（与 file_sha1_matches 同语义）；1MB 分块读取，峰值内存为常量。
pub fn file_sha1_matches_streaming(path: &Path, expected: &str) -> bool {
    if expected.is_empty() {
        return path.exists();
    }
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => hasher.update(&buf[..n]),
        }
    }
    let hash = hasher.finalize();
    hash.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
        == expected
}

/// 任意读取器的流式 sha1（zip 成员等非文件来源用）；读失败返回 None。
pub fn reader_sha1_hex<R: std::io::Read>(mut reader: R) -> Option<String> {
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return None,
        }
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_hex() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            sha1_hex(b"The quick brown fox jumps over the lazy dog"),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
    }
}
