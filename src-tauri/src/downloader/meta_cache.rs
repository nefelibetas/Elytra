//! 元数据落盘缓存：内存热度层由 `game/version/source.rs` 的 list_cache 承担，本模块是磁盘
//! 持久层（进程重启后版本清单/索引免网络）；目录 `config_dir()/cache/meta/`，键 = `sha1(url)`，
//! 条目 `{url, fetched_ms, body}`，TTL 由读端按 URL 分域（稳定数据 24h / 快变清单 10min）。

use crate::downloader::hash::sha1_hex;
use crate::util::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// 快变元数据 TTL（loader 版本列表 / maven-metadata.xml，新版本当天就可能发布）
pub const SHORT_TTL_SECS: u64 = 600;
/// 稳定元数据 TTL（version json / asset index / JDK 索引等，当天不会变）
pub const DAY_TTL_SECS: u64 = 24 * 3600;
/// 条目数与总字节上限（assets index 单件可达 ~10MB，超限按 fetched 时间从旧到新驱逐）
const MAX_ENTRIES: usize = 64;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

/// 按 URL 分域的 TTL：loader 版本列表与 maven 元 XML 当天内会新增版本，其余视为稳定数据。
pub fn ttl_for(url: &str) -> u64 {
    let fast = url.contains("maven-metadata.xml")
        || url.contains("versions/loader")
        || url.contains("piston-meta.mojang.com/mc/game");
    if fast {
        SHORT_TTL_SECS
    } else {
        DAY_TTL_SECS
    }
}

#[derive(Serialize, Deserialize)]
struct Entry {
    url: String,
    #[serde(rename = "fetched_ms")]
    fetched_ms: u64,
    body: String,
}

static FILES_COUNTER: AtomicU64 = AtomicU64::new(0);

fn cache_dir() -> PathBuf {
    config_dir().join("cache").join("meta")
}

fn path_for(url: &str) -> PathBuf {
    cache_dir().join(format!("{}.json", sha1_hex(url.as_bytes())))
}

/// 读取：`max_age_secs: None` 表示允许陈旧（离线兜底），`Some(n)` 要求新鲜。
/// 文件损坏/读失败静默清除并返回 None（缓存缺失允许，占用下行网络即可）。
pub fn get(url: &str, max_age_secs: Option<u64>) -> Option<String> {
    let path = path_for(url);
    let raw = std::fs::read_to_string(&path).ok()?;
    let entry: Entry = match serde_json::from_str(&raw) {
        Ok(e) => e,
        // envelope 损坏（半个 JSON）：删掉重建
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            return None;
        }
    };
    if entry.url != url {
        // hash 理论同栏常项：不命中就不当命中（防伪造/撞键出现脏数据）
        let _ = std::fs::remove_file(&path);
        return None;
    }
    if let Some(age) = max_age_secs {
        let elapsed = (util_ms_now() as i64 - entry.fetched_ms as i64) / 1000;
        if elapsed > age as i64 {
            return None;
        }
    }
    Some(entry.body)
}

/// 写入：原子（独立 tmp + rename），成功后按容量上限从旧到新驱逐。
/// IO 失败静默丢弃（缓存层数据无可靠性要求）。
pub fn put(url: &str, body: &str) {
    let dir = cache_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let entry = Entry {
        url: url.to_string(),
        fetched_ms: util_ms_now(),
        body: body.to_string(),
    };
    let Ok(json) = serde_json::to_string(&entry) else {
        return;
    };
    let tmp = dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        FILES_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&tmp, json).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    if std::fs::rename(&tmp, path_for(url)).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    evict();
}

/// 清空整个磁盘缓存（测试隔离用；生产不调用）。
pub fn clear_all() {
    let _ = std::fs::remove_dir_all(cache_dir());
}

/// 超容量时按 fetched_ms 从旧到新删条目（read_dir 后内存排序，总件数 ≤64，成本可忽略）。
fn evict() {
    let Ok(read_dir) = std::fs::read_dir(cache_dir()) else {
        return;
    };
    let mut items: Vec<(PathBuf, u64, u64)> = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "json") {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let fetched = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        items.push((path, fetched, size));
    }
    let total: u64 = items.iter().map(|(_, _, s)| s).sum();
    if items.len() <= MAX_ENTRIES && total <= MAX_TOTAL_BYTES {
        return;
    }
    items.sort_by_key(|(_, fetched, _)| *fetched);
    let mut removed_entries = 0usize;
    let mut removed_bytes = 0u64;
    let remaining = items.len();
    for (path, _, size) in items {
        if remaining - removed_entries <= MAX_ENTRIES && total - removed_bytes <= MAX_TOTAL_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed_entries += 1;
            removed_bytes += size;
        }
    }
}

fn util_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 独立测试 key（避免与其他用例/真实数据互相冲突）。
    fn clean_disk() {
        clear_all();
    }

    fn key(name: &str) -> String {
        format!(
            "test-meta-cache/{}/{}/{}.json",
            std::process::id(),
            name,
            name
        )
    }

    #[test]
    fn roundtrip_and_ttl_domains() {
        clean_disk();
        let u = key("roundtrip");
        assert!(get(&u, None).is_none(), "未写入时不应命中");
        put(&u, "body-v1");
        assert_eq!(get(&u, None).as_deref(), Some("body-v1"));
        assert_eq!(get(&u, Some(DAY_TTL_SECS)).as_deref(), Some("body-v1"));
        // 分域：清单类短 TTL
        assert_eq!(
            ttl_for("https://bmclapi2.bangbang93.com/optifine/1.20.1"),
            DAY_TTL_SECS
        );
        assert_eq!(
            ttl_for("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml"),
            SHORT_TTL_SECS
        );
    }

    /// 文件损坏（半个 JSON）：清除并返回 None，不影响后续访问。
    #[test]
    fn corrupt_entry_is_removed() {
        clean_disk();
        let u = key("corrupt");
        let dir = cache_dir();
        std::fs::create_dir_all(&dir).unwrap();
        // 手用 put 的键写半个 JSON
        let path = path_for(&u);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{\"url\":").unwrap();
        assert!(serde_json::from_str::<Entry>(&std::fs::read_to_string(&path).unwrap()).is_err());
        assert!(get(&u, None).is_none(), "损坏条目应被清除");
        assert!(!path.exists(), "损坏条目文件应删除");
    }

    /// 超容量驱逐：写入超过上限的条目，最旧的被清，目录回到限额内。
    #[test]
    fn evict_removes_oldest() {
        clean_disk();
        let prefix = key("evict");
        let count = MAX_ENTRIES + 10;
        for i in 0..count {
            let u = format!("{prefix}/{i}");
            put(&u, "x");
        }
        // 键即含完整 URL，读一遍统计
        let mut survives = 0usize;
        for i in 0..count {
            let u = format!("{prefix}/{i}");
            if get(&u, None).is_some() {
                survives += 1;
            }
        }
        assert!(
            survives < count,
            "超容量应有旧条目被驱逐（保留 {survives}/{count}）"
        );
        // 驱逐后目录条目数应回到上限内
        let nav = std::fs::read_dir(cache_dir()).unwrap().count();
        assert!(nav <= MAX_ENTRIES, "驱逐后应 ≤ {MAX_ENTRIES}，实际 {nav}");
    }
}
