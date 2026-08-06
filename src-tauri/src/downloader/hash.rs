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
