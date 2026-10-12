//! 统一持久化：Settings.toml 的 section 读写（`config`）+ 低层文件 IO（`store`）
//! + 密钥存储（`secrets`）。路径由 `util::config_dir()` 决定。
mod accessors;
pub mod config;
pub mod instance;
pub mod secrets;
pub(crate) mod store;

pub use accessors::*;
pub use config::PersistError;
pub use config::{read, remove, save, set_recovery_hook};
