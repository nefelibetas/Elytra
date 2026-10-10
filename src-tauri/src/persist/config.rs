//! 配置读写（TOML section 粒度）：全局锁串行化读改写 + 类型化 serde 往返。

use super::store;
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 串行化所有读改写，避免后台线程与命令写入互相覆盖（read-modify-write 竞态）。
static PERSIST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryNotice {
    pub backup_path: Option<String>,
    pub reason: String,
}
static RECOVERY: Mutex<Option<RecoveryNotice>> = Mutex::new(None);

/// 运行期推送出口（lib.rs setup 注入 AppHandle 的 emit；域层不依赖 AppHandle）。
type RecoveryHook = Arc<dyn Fn(&RecoveryNotice) + Send + Sync>;
static HOOK: Mutex<Option<RecoveryHook>> = Mutex::new(None);

/// 注入恢复事件出口（应用启动时传入，内部走 emit）。
pub fn set_recovery_hook(hook: impl Fn(&RecoveryNotice) + Send + Sync + 'static) {
    *HOOK.lock() = Some(Arc::new(hook));
}

/// 记录恢复事实：留存供启动期拉取，并推送运行期事件。
fn record_recovery(notice: RecoveryNotice) {
    *RECOVERY.lock() = Some(notice.clone());
    // 锁外调用 hook，避免持锁重入
    let hook = HOOK.lock().clone();
    if let Some(h) = hook {
        h(&notice);
    }
}

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

pub fn take_notice() -> Option<RecoveryNotice> {
    RECOVERY.lock().take()
}

fn load_root() -> Result<toml::Table, PersistError> {
    let content = match store::read_raw().map_err(PersistError::Io)? {
        Some(content) => content,
        None => {
            return Ok(toml::Table::new());
        }
    };
    if content.trim().is_empty() {
        return Ok(toml::Table::new());
    }

    match toml::from_str(&content) {
        Ok(root) => Ok(root),
        Err(e) => match store::backup_corrupt() {
            Some(path) => {
                record_recovery(RecoveryNotice {
                    backup_path: Some(path.to_string_lossy().into_owned()),
                    reason: e.to_string(),
                });
                Ok(toml::Table::new())
            }
            // 备份失败 → 绝不能返回空表（那会覆盖原文件丢数据），上抛
            None => Err(PersistError::Serde(e.to_string())),
        },
    }
}

pub fn read<T: DeserializeOwned>(key: &str) -> Result<T, PersistError> {
    let _guard = PERSIST_LOCK.lock();
    let root = load_root()?;
    let value = root.get(key).ok_or(PersistError::NotFound)?;
    value
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| PersistError::Serde(e.to_string()))
}

/// 删除某个 section。
pub fn remove(key: &str) -> Result<(), PersistError> {
    let _guard = PERSIST_LOCK.lock();

    let mut root = load_root()?;
    // 空文件无需删除
    if root.is_empty() {
        return Ok(());
    }
    root.remove(key);
    let output = toml::to_string_pretty(&root).map_err(|e| PersistError::Serde(e.to_string()))?;
    store::write_raw(&output).map_err(PersistError::Io)
}

pub fn save<T: Serialize>(key: &str, data: &T) -> Result<(), PersistError> {
    let _guard = PERSIST_LOCK.lock();
    let mut root = load_root()?;
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

    #[test]
    fn save_preserves_sibling_sections() {
        let _ = std::fs::remove_file(test_file());
        save(
            "alpha",
            &FakeConfig {
                name: "a".into(),
                count: 1,
            },
        )
        .unwrap();
        save(
            "beta",
            &FakeConfig {
                name: "b".into(),
                count: 2,
            },
        )
        .unwrap();
        save(
            "alpha",
            &FakeConfig {
                name: "a2".into(),
                count: 3,
            },
        )
        .unwrap();
        assert_eq!(
            read::<FakeConfig>("alpha").unwrap(),
            FakeConfig {
                name: "a2".into(),
                count: 3
            }
        );
        assert_eq!(
            read::<FakeConfig>("beta").unwrap(),
            FakeConfig {
                name: "b".into(),
                count: 2
            },
            "Alpha 未修改"
        );
        let _ = std::fs::remove_file(test_file());
    }

    /// 契约：配置文件损坏时，原内容完整备份到 .bak，且 save 仍可用（不卡死）。
    #[test]
    fn save_on_corrupted_file_backs_up_and_recovers() {
        let _ = std::fs::remove_file(test_file());
        let _ = std::fs::remove_file(bak_file());

        const CORRUPT: &str = "this is [[[ not valid toml";
        super::store::write_raw(CORRUPT).unwrap();

        // 损坏不再卡死：save 成功（以空表重建）
        save(
            "alpha",
            &FakeConfig {
                name: "a".into(),
                count: 1,
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(bak_file()).unwrap(),
            CORRUPT,
            "原文件必须完整备份到 .bak"
        );
        assert!(take_notice().is_some(), "恢复后应可拉取到通知");
        assert!(take_notice().is_none(), "通知取走后不应重复返回");
        assert_eq!(
            read::<FakeConfig>("alpha").unwrap(),
            FakeConfig {
                name: "a".into(),
                count: 1
            }
        );

        let _ = std::fs::remove_file(test_file());
        let _ = std::fs::remove_file(bak_file());
    }

    /// 备份文件路径（与 `store::backup_corrupt` 命名一致）。
    fn bak_file() -> PathBuf {
        let mut p = test_file();
        p.set_file_name(format!("{}.bak", crate::constants::SETTINGS_FILE));
        p
    }
}
