//! 跨域公共小工具（纯函数，无状态）。
//!
//! 按职责拆分为子模块，公共符号在 [`mod.rs`](self) 重导出，
//! `crate::util::xxx` 的调用路径保持不变：
//! - [`rt`]：在同步上下文驱动异步任务的 runtime 兜底
//! - [`time`]：文件元数据时间转换
//! - [`path`]：路径 / 文件名解析与安全校验、目录枚举
//! - [`dirs`]：配置 / 默认游戏目录基准（平台感知）
//! - [`version_name`]：版本目录名与 Java 版本字符串解析
//! - [`text`]：字符串处理与哈希
//! - [`icon`]：图片魔数嗅探 / base64 / 图标 IPC 载荷
//! - [`log`]：启动器日志落盘
//! - [`net`]：社区资源下载多源候选

mod dirs;
mod icon;
mod log;
mod net;
mod path;
mod rt;
mod text;
mod time;
mod version_name;

pub use dirs::{config_dir, default_game_dir_base, exe_dir};
pub use icon::{icon_payload, image_mime, is_supported_image, to_base64, IconData};
pub use log::init_file_logging;
pub use net::community_download_urls;
pub use path::{
    dir_entries, is_simple_name, java_bin_path, safe_relative_path, sanitize_fs_name, sha1_sub_dir,
    unique_child_dir, url_file_name,
};
pub use rt::block_on;
pub use text::{fnv1a64, tail_lines};
pub use time::modified_secs;
pub use version_name::{
    infer_loader_from_json, loader_from_dir, mc_version_from_dir, parse_java_major,
};
