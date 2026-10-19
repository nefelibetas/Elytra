//! 跨域公共小工具（纯函数，无状态）：按职责拆分子模块，公共符号在此重导出。
//! - rt 同步上下文驱动异步任务；time 文件时间转换；path 路径解析与安全校验、目录枚举
//! - dirs 配置 / 默认游戏目录基准；version_name 版本目录名与 Java 版本解析
//! - text 字符串与哈希；icon 图片嗅探 / base64 / IPC 载荷；log 日志落盘；net 社区下载多源；process 无窗口装配

mod dirs;
mod icon;
mod log;
mod net;
mod path;
mod process;
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
#[cfg(target_os = "windows")]
pub use process::CREATE_NO_WINDOW;
pub use process::{ensure_hidden_console, NoWindow};
pub use rt::block_on;
pub use text::{fnv1a64, tail_lines};
pub use time::{modified_secs, utc_stamp};
pub use version_name::{
    infer_loader_from_json, loader_from_dir, mc_version_from_dir, parse_java_major,
};
