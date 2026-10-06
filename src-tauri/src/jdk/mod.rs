//! JDK 域：扫描 / 配置持久化 / managed runtime 下载与解压。

pub mod config;
pub mod download;
pub mod model;
pub mod scanner;

pub use config::{init, read, register_scanned, save, SUPPORTED_JAVA_MAJORS};
