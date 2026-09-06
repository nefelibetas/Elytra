//! 世界地图域模块：地图编辑器的数据层（三级树、后续渲染/区块编辑）。
//! 只做纯领域逻辑，不碰 tauri::command / IPC。

pub mod biomegen;
pub mod chunk_pos;
pub mod mod_color;
pub mod render;
pub mod region_io;
pub mod tile_cache;
pub mod tree;
