use std::path::Path;

pub fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    let hash = sha1::Sha1::digest(data);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn file_sha1_matches(path: &Path, expected: &str) -> bool {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    sha1_hex(&data) == expected
}
