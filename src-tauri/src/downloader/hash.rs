use std::path::Path;

pub fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    let hash = sha1::Sha1::digest(data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

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

/// 分块流式 sha1 校验（与 file_sha1_matches 同语义，但避免整文件读内存）：
/// 本地运行时复用场景可能面对数百 MB 的 client jar / 数千个 assets，
/// 1MB 分块读取把峰值内存从文件大小降到常量（AGENTS.md 的整读取舍在
/// 复用路径不成立——一次校验多个大文件会爆内存）。
pub fn file_sha1_matches_streaming(path: &Path, expected: &str) -> bool {
    if expected.is_empty() {
        return path.exists();
    }
    use sha1::Digest;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    use std::io::Read;
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
    hash.iter().map(|b| format!("{:02x}", b)).collect::<String>() == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_hex() {
        // 标准测试向量（RFC 3174）：
        assert_eq!(
            sha1_hex(b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            sha1_hex(b""),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        // 长输入
        assert_eq!(
            sha1_hex(b"The quick brown fox jumps over the lazy dog"),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
    }
}
