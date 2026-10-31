//! region 读写与 chunk 解析（组合层）。
//!
//! 压缩格式事实（源码级核实）：
//! - region 压缩字节：1=GZip 2=Zlib 3=Raw 4=LZ4（24w04a 起 server.properties 可选）
//!   127=自定义（24w05a 起，三方服务器用；字节后跟 u16 长度前缀的算法 id 字符串）
//! - 官方客户端始终写 Zlib；1.21.5+ 单机存档仍为 Zlib。mca 与 fastanvil 的
//!   `Compression::Lz4`/`Scheme::Lz4`(byte 4) 均用 lz4_java_wrc（Java LZ4Block 格式）解压。
//! - mca 对 byte≥127 走 `CustomDecompression` trait——为兜底三方服务器 region，
//!   本模块提供 `JavaWrcDecompression`（id 含 "lz4" 时走 lz4_java_wrc）。
//!
//! 组合方案：mca 读 region（全版本统一 region 层）→ fastnbt 按本模块 `ModernChunk`
//! 解析 1.18+ 区块（含 26.x 混合 palette），失败回退 fastanvil `JavaChunk`
//! （Post18/Pre18/Pre13 三级回退）以保留旧存档预览。

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::UNIX_EPOCH;

use fastanvil::biome::Biome;
use fastanvil::{BiomeData, BlockData, Chunk, JavaChunk, SectionLike, SectionTower};
use mca::{CompressionError, CustomDecompression, RegionReader};
use parking_lot::Mutex;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::chunk_pos::ChunkPos;
use super::lru_cache::{CacheKey, LruCache};

/// 区块 palette 中的方块状态。1.18–1.21 为 `{Name, Properties}`；26.x 起无属性
/// 方块用纯字符串，同 section 一旦出现带属性方块，无属性条目退化为无名子键复合
/// `{"": "minecraft:xxx"}`，带属性条目为 `{id, properties}`。
#[derive(Debug, Clone)]
pub(crate) struct MapBlock {
    name: String,
    snowy: bool,
}

impl MapBlock {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn snowy(&self) -> bool {
        self.snowy
    }
}

impl<'de> Deserialize<'de> for MapBlock {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BlockVisitor;

        impl<'de> Visitor<'de> for BlockVisitor {
            type Value = MapBlock;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("block state string or compound")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<MapBlock, E> {
                Ok(MapBlock {
                    name: v.to_string(),
                    snowy: false,
                })
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<MapBlock, E> {
                Ok(MapBlock {
                    name: v,
                    snowy: false,
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<MapBlock, A::Error> {
                let mut name = String::new();
                let mut snowy = false;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "" | "id" | "Name" => name = map.next_value::<String>()?,
                        "properties" | "Properties" => {
                            let props: HashMap<String, String> = map.next_value()?;
                            snowy = props.get("snowy").is_some_and(|v| v == "true");
                        }
                        _ => {
                            let _: fastnbt::Value = map.next_value()?;
                        }
                    }
                }
                Ok(MapBlock { name, snowy })
            }
        }

        deserializer.deserialize_any(BlockVisitor)
    }
}

#[derive(Debug, Deserialize)]
struct ModernSection {
    #[serde(rename = "Y")]
    y: i8,
    #[serde(default)]
    block_states: BlockData<MapBlock>,
    #[serde(default)]
    biomes: BiomeData<Biome>,
}

impl SectionLike for ModernSection {
    fn is_terminator(&self) -> bool {
        false
    }

    fn y(&self) -> i8 {
        self.y
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModernChunk {
    /// 仅用于区分 1.18+ 与旧格式（旧存档无顶层 Status），不参与渲染。
    #[serde(rename = "Status")]
    _status: String,
    sections: Option<SectionTower<ModernSection>>,
}

impl ModernChunk {
    fn y_range(&self) -> Range<isize> {
        self.sections
            .as_ref()
            .map(|s| s.y_min()..s.y_max())
            .unwrap_or(0..0)
    }
}

/// 已解析区块：1.18+ / 26.x 走自解析，旧格式回退 fastanvil。
pub(crate) enum ChunkData {
    Modern(ModernChunk),
    /// JavaChunk 约 680 字节，装箱避免 1024 槽位的 region 数组整体膨胀。
    Legacy(Box<JavaChunk>),
}

/// 跨两代解析器的方块视图（渲染只需名称与 snowy）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockRef<'a> {
    name: &'a str,
    snowy: bool,
}

impl<'a> BlockRef<'a> {
    pub(crate) fn name(self) -> &'a str {
        self.name
    }

    pub(crate) fn snowy(self) -> bool {
        self.snowy
    }
}

impl ChunkData {
    pub(crate) fn y_range(&self) -> Range<isize> {
        match self {
            Self::Modern(c) => c.y_range(),
            Self::Legacy(c) => c.y_range(),
        }
    }

    pub(crate) fn block(&self, x: usize, y: isize, z: usize) -> Option<BlockRef<'_>> {
        match self {
            Self::Modern(c) => {
                let sec = c.sections.as_ref()?.get_section_for_y(y)?;
                // 空 palette 会让 fastanvil 的位宽计算下溢（debug 下 panic）。
                if sec.block_states.palette().is_empty() {
                    return None;
                }
                let in_sec = (y - sec.y as isize * 16) as usize;
                sec.block_states.at(x, in_sec, z).map(|b| BlockRef {
                    name: b.name(),
                    snowy: b.snowy(),
                })
            }
            Self::Legacy(c) => c.block(x, y, z).map(|b| BlockRef {
                name: b.name(),
                snowy: b.snowy(),
            }),
        }
    }

    pub(crate) fn biome(&self, x: usize, y: isize, z: usize) -> Option<Biome> {
        match self {
            Self::Modern(c) => {
                let sec = c.sections.as_ref()?.get_section_for_y(y)?;
                if sec.biomes.palette().is_empty() {
                    return None;
                }
                let in_sec = (y - sec.y as isize * 16) as usize;
                sec.biomes.at(x, in_sec, z).copied()
            }
            Self::Legacy(c) => c.biome(x, y, z),
        }
    }
}

/// 悬停 chunk 缓存 key：path + mtime + chunk 坐标。
#[derive(PartialEq)]
struct ChunkKey {
    path: PathBuf,
    mtime: u64,
    cx: u8,
    cz: u8,
}

impl CacheKey for ChunkKey {
    fn same_slot(&self, other: &Self) -> bool {
        self.path == other.path && self.cx == other.cx && self.cz == other.cz
    }
}

/// 8MB ≈ 16–32 个解压 chunk（按 raw_nbt × 2 估算）；只由悬停填充，
/// 不从 parse_region 批量写入，避免渲染一次撑爆。
const MAX_CACHE_CHUNK_BYTES: usize = 8 * 1024 * 1024;
static CHUNK_CACHE: LazyLock<Mutex<LruCache<ChunkKey, Arc<ParsedChunk>>>> =
    LazyLock::new(|| Mutex::new(LruCache::new(MAX_CACHE_CHUNK_BYTES)));

fn chunk_cache() -> &'static Mutex<LruCache<ChunkKey, Arc<ParsedChunk>>> {
    &CHUNK_CACHE
}

/// region 文件内单 chunk 解析结果。
pub(crate) struct ParsedChunk {
    pub(crate) chunk: ChunkData,
    /// 原始 NBT 字节；仅搜索/悬停路径保留，渲染路径为 None（避免整 region 复制）。
    pub(crate) raw_nbt: Option<Vec<u8>>,
}

impl ParsedChunk {
    /// 区块中心在 `max_y` 以下的表面群系原始 id。
    /// 每层先查 fastanvil（原版群系无需二次 NBT 解析），Unknown（新版本/Mod 群系）
    /// 再回退 raw palette；不能先整体查 fastanvil，否则会跳过上层的 Mod 群系。
    pub(crate) fn surface_biome(&self, max_y: isize) -> Option<String> {
        let range = self.chunk.y_range();
        if range.is_empty() {
            return None;
        }
        let top = (range.end - 1).min(max_y);
        let raw = self.raw_nbt.as_deref();
        for y in [top, top.min(63), range.start] {
            if y < range.start {
                continue;
            }
            if let Some(id) = self.chunk.biome(8, y, 8).and_then(legacy_biome_id) {
                return Some(id);
            }
            if let Some(id) = raw.and_then(|raw| biome_id_at(raw, 8, 8, y)) {
                return Some(id);
            }
        }
        None
    }
}

/// fastanvil 旧版群系枚举 → `minecraft:<snake_case>`（仅用于表面检索）。
fn legacy_biome_id(biome: Biome) -> Option<String> {
    let debug = format!("{biome:?}");
    if debug == "Unknown" {
        return None;
    }
    let mut out = String::new();
    for (i, ch) in debug.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    Some(format!("minecraft:{out}"))
}

/// 三方服务器自定义压缩兜底：id 含 "lz4" 走 Java LZ4Block 格式解压。
pub struct JavaWrcDecompression;

impl CustomDecompression for JavaWrcDecompression {
    fn decompress(
        &self,
        data: &[u8],
        algorithm: &str,
        out: &mut Vec<u8>,
    ) -> Result<usize, CompressionError> {
        if algorithm.to_ascii_lowercase().contains("lz4") {
            use std::io::Read;
            let mut decoder = lz4_java_wrc::Lz4BlockInput::new(data);
            decoder.read_to_end(out).map_err(CompressionError::LZ4)?;
            Ok(out.len())
        } else {
            Err(CompressionError::Unsupported)
        }
    }
}

/// 维度目录解析：空串=主世界（26.x 存档在 dimensions/minecraft/overworld，
/// 旧布局在世界根）；DIM-1/DIM1 同理带 fallback。
/// 自定义维度 id 为 `dimensions/<namespace>/<path>`（见 [`list_dimensions`]）。
/// region 文件/缓存键/列表必须共用此解析——否则列表有 region 而瓦片查不到文件。
pub fn dimension_dir(world_path: &Path, dimension: &str) -> PathBuf {
    let base = world_path.to_path_buf();
    if let Some(rel) = dimension.strip_prefix("dimensions/") {
        if let Ok(rel) = crate::util::safe_relative_path(rel) {
            return base.join("dimensions").join(rel);
        }
    }
    if let Some((ns, path)) = dimension.split_once(':') {
        if crate::util::is_simple_name(ns) && crate::util::is_simple_name(path) {
            return base.join("dimensions").join(ns).join(path);
        }
    }
    match dimension {
        "" => {
            let new_layout = base.join("dimensions/minecraft/overworld");
            if new_layout.join("region").exists() {
                new_layout
            } else {
                base
            }
        }
        d @ ("DIM-1" | "DIM1") => {
            let ns = base.join("dimensions/minecraft").join(if d == "DIM-1" {
                "the_nether"
            } else {
                "the_end"
            });
            if ns.join("region").exists() {
                ns
            } else {
                base.join(d)
            }
        }
        other => base.join(other),
    }
}

/// 列出可预览维度：主世界、下界、末地 + `dimensions/**/region` 下的自定义维度。
/// 返回的 `""` / `DIM-1` / `DIM1` 是 UI 的稳定语义 id；自定义维度返回
/// `dimensions/<namespace>/<path>`。
pub fn list_dimensions(world_path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |id: String| {
        if seen.insert(id.clone()) {
            out.push(id);
        }
    };

    if world_path.join("region").is_dir()
        || world_path
            .join("dimensions/minecraft/overworld/region")
            .is_dir()
    {
        push(String::new());
    }
    if world_path.join("DIM-1/region").is_dir()
        || world_path
            .join("dimensions/minecraft/the_nether/region")
            .is_dir()
    {
        push("DIM-1".to_string());
    }
    if world_path.join("DIM1/region").is_dir()
        || world_path
            .join("dimensions/minecraft/the_end/region")
            .is_dir()
    {
        push("DIM1".to_string());
    }

    fn walk(world: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
        if depth > 8 {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.join("region").is_dir() {
                let rel = match path.strip_prefix(world) {
                    Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                    Err(_) => continue,
                };
                // 原版三维度已有稳定 id，不重复列为自定义维度。
                if rel != "dimensions/minecraft/overworld"
                    && rel != "dimensions/minecraft/the_nether"
                    && rel != "dimensions/minecraft/the_end"
                {
                    out.push(rel);
                }
            }
            walk(world, &path, depth + 1, out);
        }
    }

    let dims = world_path.join("dimensions");
    let mut custom = Vec::new();
    if dims.is_dir() {
        walk(world_path, &dims, 0, &mut custom);
    }
    custom.sort();
    custom.dedup();
    for id in custom {
        push(id);
    }
    out
}

/// region 概要（前端视口定位 + 变更检测）。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "map/")]
pub struct MapRegionInfo {
    pub x: i32,
    pub z: i32,
    /// region 文件最后修改（unix 秒）；前端据此检测变更并增量重取瓦片。
    #[ts(type = "number")]
    pub mtime: u64,
}

/// 列出世界维度目录下全部 region（按坐标排序）。
/// `dimension_dir`：主世界=世界根，下界=DIM-1，末地=DIM1。
pub fn list_region_files(dimension_dir: &Path) -> Vec<MapRegionInfo> {
    let region_dir = dimension_dir.join("region");
    let Ok(entries) = fs::read_dir(&region_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some((x, z)) = parse_region_name(&name) else {
            continue;
        };
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(MapRegionInfo { x, z, mtime });
    }
    out.sort_by_key(|r| (r.x, r.z));
    out
}

/// 解析 "r.X.Z.mca" 文件名 → region 坐标（容忍 .mcr 旧扩展）。
fn parse_region_name(name: &str) -> Option<(i32, i32)> {
    let base = name
        .strip_suffix(".mca")
        .or_else(|| name.strip_suffix(".mcr"))?;
    let rest = base.strip_prefix('r')?;
    let rest = rest.strip_prefix('.')?;
    let (x, z) = rest.split_once('.')?;
    let x: i32 = x.parse().ok()?;
    let z: i32 = z.parse().ok()?;
    Some((x, z))
}

/// 读取单个 region 文件并解析全部已生成 chunk（渲染路径，不保留 raw NBT）。
/// 单 chunk 损坏只跳过该 chunk（返回 None），不影响其余——渲染容错优先。
pub fn parse_region(path: &Path) -> Vec<Option<ParsedChunk>> {
    parse_region_inner(path, false)
}

/// 同 [`parse_region`]，但保留每个 chunk 的 raw NBT（搜索/群系扫描需要）。
pub fn parse_region_with_raw(path: &Path) -> Vec<Option<ParsedChunk>> {
    parse_region_inner(path, true)
}

fn parse_region_inner(path: &Path, keep_raw: bool) -> Vec<Option<ParsedChunk>> {
    let Some(data) = cached_region_bytes(path) else {
        return Vec::new();
    };
    // 并行解析：1024 个 chunk 的解压+NBT 解析互不依赖（各自独立字节区间），
    // 每个工作线程持自己的 RegionReader（同一份只读数据）。8 核下整 region
    // 解析提速约 4-6x——地图首屏每瓦片亚秒级的关键。
    let n = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 16))
        .unwrap_or(4);
    let mut out: Vec<Option<ParsedChunk>> = Vec::with_capacity(1024);
    if n <= 1 {
        let Ok(mut reader) = RegionReader::new_with_decompression(&data, JavaWrcDecompression)
        else {
            return Vec::new();
        };
        for i in 0..1024u16 {
            let (cx, cz) = ChunkPos::from_index(i).coords_in_region();
            out.push(parse_one(&mut reader, cx, cz, keep_raw));
        }
        return out;
    }
    // 输出占位 + 按区间并行填充
    out.resize_with(1024, || None);
    std::thread::scope(|s| {
        let data_ref = data.as_slice();
        let mut handles = Vec::with_capacity(n);
        for t in 0..n {
            let start = (1024 * t) / n;
            let end = (1024 * (t + 1)) / n;
            if start == end {
                continue;
            }
            handles.push(s.spawn(move || {
                let Ok(mut reader) =
                    RegionReader::new_with_decompression(data_ref, JavaWrcDecompression)
                else {
                    return Vec::new();
                };
                let mut part = Vec::with_capacity(end - start);
                for i in start..end {
                    let (cx, cz) = ChunkPos::from_index(i as u16).coords_in_region();
                    part.push((i, parse_one(&mut reader, cx, cz, keep_raw)));
                }
                part
            }));
        }
        for h in handles {
            if let Ok(part) = h.join() {
                for (i, v) in part {
                    out[i] = v;
                }
            }
        }
    });
    out
}

/// 解析 region 内单个 chunk：解压 → NBT 原始字节 + fastanvil 三代回退解析。
#[cfg(test)]
static PARSE_ONE_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn parse_one(
    reader: &mut RegionReader<'_, JavaWrcDecompression>,
    x: u8,
    z: u8,
    keep_raw: bool,
) -> Option<ParsedChunk> {
    #[cfg(test)]
    PARSE_ONE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let raw = reader.chunk(x, z).ok()??;
    // fastanvil 0.32 的 Block 反序列化只认 `{Name, ...}`，26.x 的字符串 palette
    // 一定解析失败，故先按本模块模型解析；失败再回退旧格式三级回退。
    // 两者皆失败（未生成/损坏 chunk）返回 None，不影响 region 内其余 chunk。
    let chunk = match fastnbt::from_bytes::<ModernChunk>(raw) {
        Ok(c) => ChunkData::Modern(c),
        Err(_) => ChunkData::Legacy(Box::new(JavaChunk::from_bytes(raw).ok()?)),
    };
    Some(ParsedChunk {
        chunk,
        raw_nbt: keep_raw.then(|| raw.to_vec()),
    })
}

/// 世界方块坐标 → 所属 region 文件路径（不含验证；跨 dimension_dir 布局）。
pub fn region_file_for(world_path: &Path, dimension: &str, bx: i32, bz: i32) -> PathBuf {
    let rx = bx.div_euclid(512);
    let rz = bz.div_euclid(512);
    dimension_dir(world_path, dimension)
        .join("region")
        .join(format!("r.{rx}.{rz}.mca"))
}

/// 按 (region 路径, mtime, chunk 坐标) 缓存解析结果；锁外解析并双检。
fn cached_parsed_chunk(region_file: &Path, cx: u8, cz: u8) -> Option<Arc<ParsedChunk>> {
    let meta = fs::metadata(region_file).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let key = ChunkKey {
        path: region_file.to_path_buf(),
        mtime,
        cx,
        cz,
    };

    let hit = chunk_cache().lock().get(&key);
    if let Some(chunk) = hit {
        return Some(chunk);
    }

    let data = cached_region_bytes(region_file)?;
    let mut reader =
        RegionReader::new_with_decompression(data.as_slice(), JavaWrcDecompression).ok()?;
    let parsed = Arc::new(parse_one(&mut reader, cx, cz, true)?);

    let mut cache = chunk_cache().lock();
    if let Some(existing) = cache.get(&key) {
        return Some(existing);
    }
    let bytes = parsed
        .raw_nbt
        .as_ref()
        .map_or(0, |raw| raw.len().saturating_mul(2));
    cache.insert(key, Arc::clone(&parsed), bytes);
    Some(parsed)
}

/// 指定方块坐标的表面方块（自 `max_y` 向下首个非空气名）+ 所在群系原始 id。
/// 群系优先 fastanvil 枚举（原版群系无额外解析），Unknown（新版本/Mod 群系）
/// 再回退 chunk NBT 的 biome palette 直读。region 缺失/区块未生成/损坏 → None。
pub fn surface_info(
    region_file: &Path,
    bx: i32,
    bz: i32,
    max_y: isize,
) -> Option<(String, Option<String>, i32)> {
    let cx = bx.rem_euclid(512) / 16;
    let cz = bz.rem_euclid(512) / 16;
    let parsed = cached_parsed_chunk(region_file, cx as u8, cz as u8)?;

    let y_range = parsed.chunk.y_range();
    if y_range.is_empty() {
        return None;
    }
    let top = (y_range.end - 1).min(max_y);
    if top < y_range.start {
        return None;
    }
    let x = bx.rem_euclid(16) as usize;
    let z = bz.rem_euclid(16) as usize;
    for y in (y_range.start..=top).rev() {
        let Some(block) = parsed.chunk.block(x, y, z) else {
            continue;
        };
        let name = block.name();
        if name != "minecraft:air" && name != "minecraft:cave_air" {
            let biome = parsed
                .chunk
                .biome(x, y, z)
                .and_then(legacy_biome_id)
                .or_else(|| {
                    parsed
                        .raw_nbt
                        .as_deref()
                        .and_then(|raw| biome_id_at(raw, x, z, y))
                });
            return Some((name.to_string(), biome, y as i32));
        }
    }
    None
}

/// 群系回退探针：只反序列化 `sections[].Y/biomes`，`block_states` 由 serde 跳过。
#[derive(Deserialize)]
struct BiomeProbeChunk {
    #[serde(default)]
    sections: Vec<BiomeProbeSection>,
}

#[derive(Deserialize)]
struct BiomeProbeSection {
    #[serde(rename = "Y")]
    y: i8,
    biomes: Option<BiomePalette>,
}

#[derive(Deserialize)]
struct BiomePalette {
    #[serde(default)]
    palette: Vec<String>,
    data: Option<fastnbt::LongArray>,
}

/// 从 chunk 原始 NBT 读 (x,z) 在高度 y 处的群系 id（biome 4×4×4 网格）。
fn biome_id_at(raw: &[u8], x: usize, z: usize, y: isize) -> Option<String> {
    let chunk: BiomeProbeChunk = fastnbt::from_bytes(raw).ok()?;
    let section = chunk.sections.iter().find(|s| s.y == (y >> 4) as i8)?;
    palette_biome(section.biomes.as_ref()?, x, z, y)
}

fn palette_biome(palette: &BiomePalette, x: usize, z: usize, y: isize) -> Option<String> {
    let names = &palette.palette;
    if names.is_empty() {
        return None;
    }
    // biome 单元：4×4×4 网格，索引 = x4 + z4*4 + y4*16
    let idx = ((x >> 2) & 3) + ((z >> 2) & 3) * 4 + (((y as usize) >> 2) & 3) * 16;
    if names.len() == 1 {
        return names.first().cloned();
    }
    let bits = (names.len() as f64).log2().ceil().max(1.0) as usize;
    if bits == 0 || bits > 64 {
        return None;
    }
    // 调色板按 64 位字打包，值不跨字：word = idx / (64/bits), shift = (idx % per) * bits。
    let per = 64 / bits;
    let word = idx / per;
    let shift = (idx % per) * bits;
    let value = *palette.data.as_ref()?.get(word)?;
    let mask = (1i64 << bits.min(63)) - 1;
    let v = ((value >> shift) & mask) as usize;
    names.get(v).cloned()
}

/// region 原始字节缓存 key：path + mtime + len。
#[derive(PartialEq)]
struct RegionKey {
    path: PathBuf,
    mtime: u64,
    len: u64,
}

impl CacheKey for RegionKey {
    fn same_slot(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

/// 64MB ≈ 1–6 个典型 region（10–50MB）；按字节而非条目数。
const MAX_CACHED_REGION_BYTES: usize = 64 * 1024 * 1024;
static REGION_CACHE: LazyLock<Mutex<LruCache<RegionKey, Arc<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(LruCache::new(MAX_CACHED_REGION_BYTES)));

#[cfg(test)]
static REGION_DISK_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn region_cache() -> &'static Mutex<LruCache<RegionKey, Arc<Vec<u8>>>> {
    &REGION_CACHE
}

/// 按路径、mtime、长度缓存region原始字节；锁外读盘，读失败不缓存。
/// 返回 Arc 以便调用方在锁外安全使用，淘汰不影响已持有的 Arc。
fn cached_region_bytes(path: &Path) -> Option<Arc<Vec<u8>>> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let len = meta.len();
    let key = RegionKey {
        path: path.to_path_buf(),
        mtime,
        len,
    };

    let hit = region_cache().lock().get(&key);
    if let Some(data) = hit {
        return Some(data);
    }

    let data = Arc::new(fs::read(path).ok()?);
    #[cfg(test)]
    {
        REGION_DISK_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    // 双检
    let mut cache = region_cache().lock();
    if let Some(existing) = cache.get(&key) {
        return Some(existing);
    }
    cache.insert(key, Arc::clone(&data), data.len());
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_name_parses_negative_coords() {
        assert_eq!(parse_region_name("r.0.0.mca"), Some((0, 0)));
        assert_eq!(parse_region_name("r.-3.12.mca"), Some((-3, 12)));
        assert_eq!(parse_region_name("r.5.-19.mcr"), Some((5, -19)));
        assert_eq!(parse_region_name("r.x.z.mca"), None);
        assert_eq!(parse_region_name("other.mca"), None);
    }

    #[test]
    fn dimension_dir_prefers_new_layout() {
        // 26.x 布局：dimensions/minecraft/overworld
        let dir = std::env::temp_dir().join(format!("elytra_map_dim_{}", std::process::id()));
        let world = dir.join("w");
        std::fs::create_dir_all(world.join("dimensions/minecraft/overworld/region")).unwrap();
        let got = dimension_dir(&world, "");
        assert_eq!(got, world.join("dimensions/minecraft/overworld"));
        // 下界/末地：26.x 命名空间布局优先
        std::fs::create_dir_all(world.join("dimensions/minecraft/the_nether/region")).unwrap();
        assert_eq!(
            dimension_dir(&world, "DIM-1"),
            world.join("dimensions/minecraft/the_nether")
        );
        // 旧布局 fallback：世界根 / DIM-1
        assert_eq!(dimension_dir(&dir.join("nowhere"), ""), dir.join("nowhere"));
        assert_eq!(
            dimension_dir(&dir.join("nowhere"), "DIM-1"),
            dir.join("nowhere").join("DIM-1")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dimensions_include_custom_namespace() {
        let dir = std::env::temp_dir().join(format!("elytra_map_dims_{}", std::process::id()));
        let world = dir.join("w");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::create_dir_all(world.join("dimensions/minecraft/the_nether/region")).unwrap();
        std::fs::create_dir_all(world.join("dimensions/mymod/caves/region")).unwrap();
        let dims = list_dimensions(&world);
        assert!(dims.iter().any(|d| d.is_empty()));
        assert!(dims.iter().any(|d| d == "DIM-1"));
        assert!(dims.iter().any(|d| d == "dimensions/mymod/caves"));
        assert_eq!(
            dimension_dir(&world, "dimensions/mymod/caves"),
            world.join("dimensions/mymod/caves")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn region_list_skips_non_region_files() {
        let dir = std::env::temp_dir().join(format!("elytra_map_regions_{}", std::process::id()));
        let region = dir.join("region");
        fs::create_dir_all(&region).unwrap();
        fs::write(region.join("r.0.0.mca"), b"x").unwrap();
        fs::write(region.join("r.1.2.mca"), b"x").unwrap();
        fs::write(region.join("r.bad.mca"), b"x").unwrap();
        let got = list_region_files(&dir);
        assert_eq!(got.len(), 2, "只应识别合法 region 名");
        assert_eq!(got[0].x, 0, "按 (x,z) 排序");
        assert!(got[0].mtime > 0, "region mtime 应为有效 unix 秒");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn region_cache_invalidates_on_mtime_change() {
        let mut cache = LruCache::new(1024);
        let key = |mtime: u64, len: u64| RegionKey {
            path: PathBuf::from("r.0.0.mca"),
            mtime,
            len,
        };

        cache.insert(key(100, 4), Arc::new(vec![1; 4]), 4);
        assert!(cache.get(&key(100, 4)).is_some());
        assert!(cache.get(&key(101, 4)).is_none(), "mtime 变化必须 miss");
        assert!(cache.get(&key(101, 5)).is_none(), "长度变化也必须 miss");

        cache.insert(key(101, 4), Arc::new(vec![2; 4]), 4);
        assert_eq!(cache.get(&key(101, 4)).unwrap().as_slice(), &[2; 4]);
        assert!(
            cache.get(&key(100, 4)).is_none(),
            "同路径旧版本必须被替换，不能堆积"
        );
    }

    #[test]
    fn cached_region_bytes_reads_disk_once_per_version() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("r.0.0.mca");
        fs::write(&p, b"region-v1").unwrap();
        let t1 = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        set_mtime(&p, t1);

        let before = REGION_DISK_READS.load(std::sync::atomic::Ordering::Relaxed);
        let a = cached_region_bytes(&p).unwrap();
        let b = cached_region_bytes(&p).unwrap();

        assert_eq!(a.as_slice(), b"region-v1");
        assert!(Arc::ptr_eq(&a, &b), "缓存命中应复用同一个Arc");
        assert_eq!(
            REGION_DISK_READS.load(std::sync::atomic::Ordering::Relaxed) - before,
            1,
            "同一(path, mtime, len)应只读一次盘"
        );

        fs::write(&p, b"region-v2").unwrap();
        set_mtime(&p, t1 + std::time::Duration::from_secs(1));
        let c = cached_region_bytes(&p).unwrap();

        assert_eq!(c.as_slice(), b"region-v2");
        assert_eq!(
            REGION_DISK_READS.load(std::sync::atomic::Ordering::Relaxed) - before,
            2,
            "mtime 变化后应重新读盘"
        )
    }

    fn set_mtime(path: &Path, t: std::time::SystemTime) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    fn dummy_chunk(bytes: usize) -> Arc<ParsedChunk> {
        Arc::new(ParsedChunk {
            chunk: ChunkData::Modern(ModernChunk {
                _status: "full".into(),
                sections: None,
            }),
            raw_nbt: Some(vec![0u8; bytes]),
        })
    }

    fn parse_one_calls() -> usize {
        PARSE_ONE_CALLS.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[test]
    fn chunk_cache_keys_by_mtime_and_coords() {
        let mut cache = LruCache::new(1024);
        let key = |mtime: u64, cx: u8, cz: u8| ChunkKey {
            path: PathBuf::from("r.0.0.mca"),
            mtime,
            cx,
            cz,
        };

        cache.insert(key(1, 0, 0), dummy_chunk(4), 8);
        assert!(cache.get(&key(1, 0, 0)).is_some());
        assert!(cache.get(&key(2, 0, 0)).is_none(), "mtime 变化必须 miss");
        assert!(cache.get(&key(1, 1, 0)).is_none(), "cx 变化必须 miss");
        assert!(cache.get(&key(1, 0, 1)).is_none(), "cz 变化必须 miss");

        // 同 path+chunk 的新 mtime 应替换旧条目，不能堆积
        cache.insert(key(2, 0, 0), dummy_chunk(4), 8);
        assert!(cache.get(&key(1, 0, 0)).is_none());
        assert!(cache.get(&key(2, 0, 0)).is_some());
    }

    #[test]
    fn parse_region_keeps_raw_only_on_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::map::test_support::write_synthetic_region(dir.path());

        let light = parse_region(&path);
        assert!(
            light[0].as_ref().unwrap().raw_nbt.is_none(),
            "渲染路径不应复制 raw NBT"
        );

        let raw = parse_region_with_raw(&path);
        assert!(
            raw[0].as_ref().unwrap().raw_nbt.is_some(),
            "搜索路径必须保留 raw NBT"
        );
    }

    #[test]
    fn surface_biome_falls_back_to_raw_per_height() {
        // 顶层是 fastanvil 不认识的 Mod 群系，Y=63 是原版 plains。
        // 回退必须逐层进行，否则会错误返回下层的 plains。
        let chunk = chunk_with_biomes(&[(19, "custom:modded_biome"), (3, "minecraft:plains")]);
        assert_eq!(
            chunk.surface_biome(319).as_deref(),
            Some("custom:modded_biome")
        );
    }

    #[test]
    fn biome_probe_reads_packed_palette() {
        // 2 项 palette → 1 bit/值；索引 10 = x4(2) + z4(2)*4 + y4(0)*16
        let biomes = fastnbt::Value::Compound(HashMap::from([
            (
                "palette".to_string(),
                fastnbt::Value::List(vec![
                    fastnbt::Value::String("minecraft:plains".into()),
                    fastnbt::Value::String("minecraft:desert".into()),
                ]),
            ),
            (
                "data".to_string(),
                fastnbt::Value::LongArray(fastnbt::LongArray::new(vec![1i64 << 10])),
            ),
        ]));
        let section = fastnbt::Value::Compound(HashMap::from([
            ("Y".to_string(), fastnbt::Value::Byte(0)),
            ("biomes".to_string(), biomes),
        ]));
        let raw = fastnbt::to_bytes(&fastnbt::Value::Compound(HashMap::from([(
            "sections".to_string(),
            fastnbt::Value::List(vec![section]),
        )])))
        .unwrap();

        assert_eq!(
            biome_id_at(&raw, 8, 8, 0).as_deref(),
            Some("minecraft:desert")
        );
        assert_eq!(
            biome_id_at(&raw, 0, 0, 0).as_deref(),
            Some("minecraft:plains")
        );
    }

    /// 由 (section Y, biome id) 构造已解析 chunk；raw NBT 保留供回退路径使用。
    fn chunk_with_biomes(sections: &[(i8, &str)]) -> ParsedChunk {
        let sections = sections
            .iter()
            .map(|(y, biome)| {
                let biomes = fastnbt::Value::Compound(HashMap::from([(
                    "palette".to_string(),
                    fastnbt::Value::List(vec![fastnbt::Value::String((*biome).into())]),
                )]));
                fastnbt::Value::Compound(HashMap::from([
                    ("Y".to_string(), fastnbt::Value::Byte(*y)),
                    ("biomes".to_string(), biomes),
                ]))
            })
            .collect();
        let root = fastnbt::Value::Compound(HashMap::from([
            (
                "Status".to_string(),
                fastnbt::Value::String("minecraft:full".into()),
            ),
            ("sections".to_string(), fastnbt::Value::List(sections)),
        ]));
        let raw = fastnbt::to_bytes(&root).unwrap();
        let chunk = fastnbt::from_bytes::<ModernChunk>(&raw).unwrap();
        ParsedChunk {
            chunk: ChunkData::Modern(chunk),
            raw_nbt: Some(raw),
        }
    }

    #[test]
    fn surface_info_reuses_parsed_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::map::test_support::write_synthetic_region(dir.path());
        let before = parse_one_calls();

        let a = surface_info(&path, 0, 0, 319).expect("surface");
        assert_eq!(a.0, "minecraft:oak_planks");
        assert_eq!(a.1.as_deref(), Some("minecraft:plains"));
        assert_eq!(a.2, 79);
        assert_eq!(parse_one_calls(), before + 1, "首次应解析一次");

        let b = surface_info(&path, 0, 0, 319).expect("surface");
        assert_eq!(b, a);
        assert_eq!(parse_one_calls(), before + 1, "同 chunk 命中缓存");

        let c = surface_info(&path, 1, 1, 319).expect("surface");
        assert_eq!(c.0, "minecraft:oak_planks");
        assert_eq!(parse_one_calls(), before + 1, "同 chunk 换方块仍命中");

        let d = surface_info(&path, 0, 0, 0).expect("surface");
        assert_eq!(d.0, "minecraft:stone");
        assert_eq!(parse_one_calls(), before + 1, "max_y 不参与 chunk 缓存 key");
    }

    #[test]
    fn surface_info_invalidates_on_mtime_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::map::test_support::write_synthetic_region(dir.path());
        surface_info(&path, 0, 0, 319).expect("surface");
        let before = parse_one_calls();

        set_mtime(
            &path,
            std::time::SystemTime::now() + std::time::Duration::from_secs(2),
        );
        surface_info(&path, 0, 0, 319).expect("surface");
        assert_eq!(parse_one_calls(), before + 1, "mtime 变化应重新解析");
    }

    #[test]
    fn surface_info_misses_other_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let path = crate::map::test_support::write_synthetic_region(dir.path());
        surface_info(&path, 0, 0, 319).expect("surface");
        let before = parse_one_calls();

        // chunk (1,0) 未生成：解析一次，返回 None，不写缓存
        assert!(surface_info(&path, 16, 0, 319).is_none());
        assert_eq!(parse_one_calls(), before + 1);
    }
}
