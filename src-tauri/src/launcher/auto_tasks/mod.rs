//! 自动任务：`pre_launch` / `on_crash` / `post_exit` 三个时点的内置动作与自定义命令。
//!
//! 动作分两类：内置（Rust 实现，跨平台零依赖）与自定义（系统命令行 `cmd /C` / `sh -c`）。
//! 执行序固定为「内置在前、自定义在后」，同类内保持用户序（见 [`model::ordered_steps`]）。

pub mod backup;
pub mod crashpack;
pub mod ctx;
pub mod model;
pub mod record;
pub mod redact;
pub mod runner;
