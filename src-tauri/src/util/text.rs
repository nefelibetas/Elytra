//! 字符串处理与稳定哈希。

/// 取文本尾部最多 200 行（lines().rev().take(200) 后反转，保留原顺序）。
pub fn tail_lines(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().rev().take(200).collect();
    lines.reverse();
    lines.join("\n")
}

/// FNV-1a 64 位散列（跨进程稳定，非加密）：离线皮肤哈希 / 缓存键派生共用。
pub fn fnv1a64(data: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
