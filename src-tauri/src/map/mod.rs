//! 存档世界地图：只读 region 数据，渲染 top-down / 固定 Y 切面瓦片。
//! 领域层不依赖 tauri；命令层负责版本/存档路径解析与 IPC。

pub mod chunk_pos;
pub mod folder;
mod lru_cache;
pub mod mod_color;
pub mod region_io;
pub mod render;
pub mod search;
pub mod tile_cache;
pub mod world_info;

#[cfg(test)]
pub(crate) mod test_support;
