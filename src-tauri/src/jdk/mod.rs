//! JDK 域：扫描 / 配置持久化 / managed runtime 下载与解压。

pub mod config;
pub mod download;
pub mod model;
pub mod scanner;

pub use config::{init, read, read_cache, register_scanned, save, SUPPORTED_JAVA_MAJORS};
