//! 自动任务：`pre_launch` / `on_crash` / `post_exit` 三个时点，动作为内置（Rust，跨平台）或自定义（`cmd /C` / `sh -c`）。
//! 执行序固定「内置在前、自定义在后」，同类内保持用户序（见 [`model::ordered_steps`]）。

pub mod backup;
pub mod crashpack;
pub mod ctx;
pub mod model;
pub mod record;
pub mod redact;
pub mod runner;
