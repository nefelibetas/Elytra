//! 文件元数据时间转换 + 时间戳格式化。

/// 文件最后修改时间（unix 秒），读取失败回退 0（不存在/无权限/时钟异常均不致命）。
/// version.rs / mod_updates(identify,scan) 三处共用，消除重复的 metadata 时间链。
pub fn modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 当前 UTC 时间戳（`YYYYMMDD-HHMMSS`）：备份 timepoint 与崩溃包文件名用。
pub fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    stamp_from_unix(secs)
}

/// unix 秒 → `YYYYMMDD-HHMMSS`（UTC）。
/// 不引入 chrono：民用日历换算用 Howard Hinnant 的 `civil_from_days` 算法（纯整数、无查表）。
pub fn stamp_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// 1970-01-01 起的天数 → (年, 月, 日)。见 Howard Hinnant《chrono-Compatible Low-Level Date Algorithms》。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_from_epoch() {
        assert_eq!(stamp_from_unix(0), "19700101-000000");
    }

    #[test]
    fn stamp_known_vectors() {
        // 2023-11-14T22:13:20Z
        assert_eq!(stamp_from_unix(1_700_000_000), "20231114-221320");
        // 2000-02-29T12:00:00Z（闰日）
        assert_eq!(stamp_from_unix(951_825_600), "20000229-120000");
        // 2026-10-12T00:00:00Z
        assert_eq!(stamp_from_unix(1_791_763_200), "20261012-000000");
    }

    #[test]
    fn stamp_sorts_lexicographically_with_time() {
        let a = stamp_from_unix(1_700_000_000);
        let b = stamp_from_unix(1_700_000_001);
        assert!(a < b, "时间戳字典序必须等于时间序（备份清理依赖此性质）");
    }
}
