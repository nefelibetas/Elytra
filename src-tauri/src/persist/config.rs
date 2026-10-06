//! 配置读写（TOML section 粒度）：全局锁串行化读改写 + 类型化 serde 往返。

use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::store;

/// 串行化所有读改写，避免后台线程与命令写入互相覆盖（read-modify-write 竞态）。
static PERSIST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub enum PersistError {
    NotFound,
    Io(String),
    Serde(String),
}

impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistError::NotFound => write!(f, "config section not found"),
            PersistError::Io(e) => write!(f, "IO error: {}", e),
            PersistError::Serde(e) => write!(f, "serialization error: {}", e),
        }
    }
}

pub fn read<T: DeserializeOwned>(key: &str) -> Result<T, PersistError> {
    let _guard = PERSIST_LOCK.lock();
    let content = store::read_raw().map_err(PersistError::Io)?;
    if content.is_empty() {
        return Err(PersistError::NotFound);
    }
    let root: toml::Table =
        toml::from_str(&content).map_err(|e| PersistError::Serde(e.to_string()))?;
    let value = root.get(key).ok_or(PersistError::NotFound)?;
    // 直接反序列化值本身（String / Vec 等裸值也是合法 TOML value），
    // 不能先 to_string 再 from_str——那要求文档必须是 table。
    value
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| PersistError::Serde(e.to_string()))
}

/// 删除某个 section。
pub fn remove(key: &str) -> Result<(), PersistError> {
    let _guard = PERSIST_LOCK.lock();
    let content = store::read_raw().unwrap_or_default();
    let mut root: toml::Table = toml::from_str(&content).unwrap_or_default();
    root.remove(key);
    let output = toml::to_string_pretty(&root).map_err(|e| PersistError::Serde(e.to_string()))?;
    store::write_raw(&output).map_err(PersistError::Io)
}

pub fn save<T: Serialize>(key: &str, data: &T) -> Result<(), PersistError> {
    let _guard = PERSIST_LOCK.lock();
    let content = store::read_raw().unwrap_or_default();
    let mut root: toml::Table = toml::from_str(&content).unwrap_or_default();

    let value = toml::Value::try_from(data).map_err(|e| PersistError::Serde(e.to_string()))?;
    root.insert(key.to_string(), value);

    let output = toml::to_string_pretty(&root).map_err(|e| PersistError::Serde(e.to_string()))?;
    store::write_raw(&output).map_err(PersistError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct FakeConfig {
        name: String,
        count: i32,
    }

    /// 测试进程的 current_exe 在 target/debug/deps/ 下，配置文件独立于正式运行。
    fn test_file() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap()
            .join(crate::constants::CONFIG_DIR)
            .join(crate::constants::SETTINGS_FILE)
    }

    #[test]
    fn test_read_write_string() {
        let _ = std::fs::remove_file(test_file());
        save("last_offline_username", &"BridgeFishDev".to_string()).unwrap();
        let v: String = read("last_offline_username").unwrap();
        assert_eq!(v, "BridgeFishDev");
        let _ = std::fs::remove_file(test_file());
    }

    #[test]
    fn test_read_write_vec() {
        let _ = std::fs::remove_file(test_file());
        save("history", &vec!["a".to_string(), "b".to_string()]).unwrap();
        let v: Vec<String> = read("history").unwrap();
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);
        let _ = std::fs::remove_file(test_file());
    }

    #[test]
    fn test_read_write_struct() {
        let _ = std::fs::remove_file(test_file());
        let cfg = FakeConfig {
            name: "x".into(),
            count: 3,
        };
        save("fake", &cfg).unwrap();
        let v: FakeConfig = read("fake").unwrap();
        assert_eq!(v, cfg);
        let _ = std::fs::remove_file(test_file());
    }
}
