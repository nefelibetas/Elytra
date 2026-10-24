//! 种子预览：cubiomes 主世界群系预测与结构候选。
//! 版本/种子解析在 version/seed；瓦片与结构在 tile；缓存复用 mapcache。

mod cache;
mod ffi;
mod model;
mod seed;
mod tile;
mod version;

pub use cache::{read_tile, tile_cache_path, write_tile_png};
pub use model::{
    ResolvedVersion, SeedMapBlock, SeedMapSlimeChunk, SeedMapStructure, SeedMapVersion,
};
pub use seed::parse_seed;
pub use tile::{
    biome_at, collect_structures, find_slime_chunks, find_slime_chunks_seed, find_structures,
    render_tile, slime_chunks, slime_chunks_seed,
};
pub use version::{resolve_version, supported_versions};
