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
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use fastanvil::biome::Biome;
use fastanvil::{BiomeData, BlockData, Chunk, JavaChunk, SectionLike, SectionTower};
use mca::{CompressionError, CustomDecompression, RegionReader};
use parking_lot::Mutex;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::chunk_pos::ChunkPos;

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

/// region 文件内单 chunk 解析结果。
pub(crate) struct ParsedChunk {
    pub(crate) chunk: ChunkData,
    /// 原始 NBT 字节（未解析快照；区块复制通道直接用，避免二次解压）
    pub(crate) raw_nbt: Vec<u8>,
}

impl ParsedChunk {
    /// 区块中心在 `max_y` 以下的表面群系原始 id（1.18+ biome palette 直读）。
    pub(crate) fn surface_biome(&self, max_y: isize) -> Option<String> {
        let range = self.chunk.y_range();
        if range.is_empty() {
            return None;
        }
        let top = (range.end - 1).min(max_y);
        for y in [top, top.min(63), range.start] {
            if y < range.start {
                continue;
            }
            if let Some(id) = biome_id_at(&self.raw_nbt, 8, 8, y) {
                return Some(id);
            }
        }
        for y in [top, top.min(63), range.start] {
            if y < range.start {
                continue;
            }
            if let Some(id) = self.chunk.biome(8, y, 8).and_then(legacy_biome_id) {
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
    /// location 表非零条目 == 1024；仅作诊断，不参与预测/空洞判定。
    pub full: bool,
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
        let path = entry.path();
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let full = region_fully_generated(&path);
        out.push(MapRegionInfo { x, z, mtime, full });
    }
    out.sort_by_key(|r| (r.x, r.z));
    out
}

/// region 头部 location 表（4KiB，每 chunk 4B 高字节非零=有数据）读一遍统计。
fn region_fully_generated(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut reader = std::io::BufReader::new(file);
    let mut header = [0u8; 4096];
    if reader.read_exact(&mut header).is_err() {
        return false;
    }
    let count = header
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|c| c[0] != 0)
        .count();
    count == 1024
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

/// 读取单个 region 文件并解析全部已生成 chunk。
/// 单 chunk 损坏只跳过该 chunk（返回 None），不影响其余——渲染容错优先。
pub fn parse_region(path: &Path) -> Vec<Option<ParsedChunk>> {
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
            out.push(parse_one(&mut reader, cx, cz));
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
                    part.push((i, parse_one(&mut reader, cx, cz)));
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
fn parse_one(
    reader: &mut RegionReader<'_, JavaWrcDecompression>,
    x: u8,
    z: u8,
) -> Option<ParsedChunk> {
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
        raw_nbt: raw.to_vec(),
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

/// 指定方块坐标的表面方块（自 `max_y` 向下首个非空气名）+ 所在群系原始 id。
/// 群系经 chunk NBT biome palette 直读（fastanvil 会把 26.x/Mod 群系折叠为
/// Unknown，这里绕开）：返回完整 id 形如 "minecraft:plains"。region 缺失/
/// 区块未生成/损坏 → None。
pub fn surface_info(
    region_file: &Path,
    bx: i32,
    bz: i32,
    max_y: isize,
) -> Option<(String, Option<String>, i32)> {
    let data = cached_region_bytes(region_file)?;
    let mut reader =
        RegionReader::new_with_decompression(data.as_slice(), JavaWrcDecompression).ok()?;

    let cx = bx.rem_euclid(512) / 16;
    let cz = bz.rem_euclid(512) / 16;
    let parsed = parse_one(&mut reader, cx as u8, cz as u8)?;
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
            let biome = biome_id_at(&parsed.raw_nbt, x, z, y);
            return Some((name.to_string(), biome, y as i32));
        }
    }
    None
}

/// 从 chunk 原始 NBT 读 (x,z) 在高度 y 处的群系 id（biome 4×4×4 网格）。
fn biome_id_at(raw: &[u8], x: usize, z: usize, y: isize) -> Option<String> {
    let root: fastnbt::Value = fastnbt::from_bytes(raw).ok()?;
    let fastnbt::Value::Compound(map) = &root else {
        return None;
    };
    let fastnbt::Value::List(sections) = map.get("sections")? else {
        return None;
    };
    let sec_y = (y >> 4) as i8;
    for s in sections {
        let fastnbt::Value::Compound(sec) = s else {
            continue;
        };
        let fastnbt::Value::Byte(sy) = sec.get("Y")? else {
            continue;
        };
        if *sy != sec_y {
            continue;
        }
        let fastnbt::Value::Compound(bio) = sec.get("biomes")? else {
            return None;
        };
        let fastnbt::Value::List(palette) = bio.get("palette")? else {
            return None;
        };
        let names: Vec<String> = palette
            .iter()
            .filter_map(|p| match p {
                fastnbt::Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        if names.is_empty() {
            return None;
        }
        // biome 单元：4×4×4 网格，索引 = x4 + z4*4 + y4*16
        let idx = ((x >> 2) & 3) + ((z >> 2) & 3) * 4 + (((y as usize) >> 2) & 3) * 16;
        if names.len() == 1 {
            return Some(names[0].clone());
        }
        let fastnbt::Value::LongArray(la) = bio.get("data")? else {
            return None;
        };
        let data = la.clone().into_inner();
        let bits = (names.len() as f64).log2().ceil().max(1.0) as usize;
        if bits == 0 || bits > 64 {
            return None;
        }
        // 调色板按 64 位字打包，值不跨字：word = idx / (64/bits), shift = (idx % per) * bits。
        let per = 64 / bits;
        let word = idx / per;
        let shift = (idx % per) * bits;
        if word >= data.len() {
            return None;
        }
        let mask = (1i64 << bits.min(63)) - 1;
        let v = ((data[word] >> shift) & mask) as usize;
        return names.get(v).cloned();
    }
    None
}

const MAX_CACHED_REGION_BYTES: usize = 64 * 1024 * 1024;

struct RegionEntry {
    path: PathBuf,
    mtime: u64,
    len: u64,
    data: Arc<Vec<u8>>,
}

struct RegionCache {
    max_bytes: usize,
    total: usize,
    entries: Vec<RegionEntry>,
}

impl RegionCache {
    fn new(max_bytes: usize) -> Self {
        RegionCache {
            max_bytes,
            total: 0,
            entries: Vec::new(),
        }
    }

    /// 最近使用返回到队首
    fn get(&mut self, path: &Path, mtime: u64, len: u64) -> Option<Arc<Vec<u8>>> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.path.as_path() == path && e.mtime == mtime && e.len == len)?;
        let entry = self.entries.remove(pos);
        let data = Arc::clone(&entry.data);
        self.entries.insert(0, entry);
        Some(data)
    }

    /// 旧路径先删除; 数据入队首; 从队尾开始淘汰缓存
    fn insert(&mut self, path: PathBuf, mtime: u64, len: u64, data: Arc<Vec<u8>>) {
        let existing = self.entries.iter().position(|e| e.path == path);
        if let Some(pos) = existing {
            let old = self.entries.remove(pos);
            self.total = self.total.saturating_sub(old.data.len());
        }
        self.total += data.len();
        self.entries.insert(
            0,
            RegionEntry {
                path,
                mtime,
                len,
                data,
            },
        );
        // 至少保留一条
        while self.total > self.max_bytes && self.entries.len() > 1 {
            let Some(old) = self.entries.pop() else { break };
            self.total = self.total.saturating_sub(old.data.len());
        }
    }
}

static REGION_CACHE: OnceLock<Mutex<RegionCache>> = OnceLock::new();

#[cfg(test)]
static REGION_DISK_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn region_cache() -> &'static Mutex<RegionCache> {
    REGION_CACHE.get_or_init(|| Mutex::new(RegionCache::new(MAX_CACHED_REGION_BYTES)))
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

    let hit = region_cache().lock().get(path, mtime, len);
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
    if let Some(existing) = cache.get(path, mtime, len) {
        return Some(existing);
    }
    cache.insert(path.to_path_buf(), mtime, len, Arc::clone(&data));
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
    fn region_cache_lru_evict_oldest() {
        let mut cache = RegionCache::new(8);
        let (a, b, c) = (Path::new("a.mca"), Path::new("b.mca"), Path::new("c.mca"));
        let bytes = |n: usize| Arc::new(vec![0u8; n]);

        cache.insert(a.to_path_buf(), 0, 4, bytes(4));
        cache.insert(b.to_path_buf(), 0, 4, bytes(4));
        // 此时队列为[a, b]
        assert!(cache.get(a, 0, 4).is_some(), "a 应在缓存");

        // 淘汰队尾, c插入队头
        cache.insert(c.to_path_buf(), 0, 4, bytes(4));

        // 此时队列为[c, a]
        assert!(cache.get(a, 0, 4).is_some(), "a 最近用过，应保留");
        assert!(cache.get(b, 0, 4).is_none(), "最久未使用的 b 应被淘汰");
        assert!(cache.get(c, 0, 4).is_some(), "c 刚写入");
        assert_eq!(cache.total, 8, "淘汰后总字节数应回到上限");
    }

    #[test]
    fn region_cache_invalidates_on_mtime_change() {
        let mut cache = RegionCache::new(1024);
        let p = Path::new("r.0.0.mca");

        cache.insert(p.to_path_buf(), 100, 4, Arc::new(vec![1; 4]));
        assert!(cache.get(p, 100, 4).is_some());

        assert!(cache.get(p, 101, 4).is_none(), "mtime 变化必须 miss");
        assert!(cache.get(p, 101, 5).is_none(), "长度变化也必须 miss");

        cache.insert(p.to_path_buf(), 101, 4, Arc::new(vec![2; 4]));
        assert_eq!(cache.get(p, 101, 4).unwrap().as_slice(), &[2; 4]);
        assert_eq!(cache.entries.len(), 1, "同路径旧版本必须被替换，不能堆积");
    }

    #[test]
    fn region_cache_keeps_single_oversized_entry() {
        let mut cache = RegionCache::new(4);
        let p = Path::new("huge.mca");

        cache.insert(p.to_path_buf(), 0, 10, Arc::new(vec![0u8; 10]));

        assert!(cache.get(p, 0, 10).is_some(), "单个超大region不能淘汰自身");
        assert_eq!(cache.total, 10);
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

    #[test]
    fn region_full_flag_reflects_location_header() {
        let dir = tempfile::tempdir().unwrap();
        let region = dir.path().join("region");
        fs::create_dir_all(&region).unwrap();

        // 不足 4KiB：header 读不满，full = false
        fs::write(region.join("r.0.0.mca"), b"x").unwrap();
        // 4KiB header，1024 项首字节全非零：full = true
        let mut header = vec![0u8; 4096];
        for i in 0..1024 {
            header[i * 4] = 1;
        }
        fs::write(region.join("r.1.0.mca"), &header).unwrap();

        let got = list_region_files(dir.path());
        assert_eq!(got.len(), 2);
        let r00 = got.iter().find(|r| (r.x, r.z) == (0, 0)).unwrap();
        let r10 = got.iter().find(|r| (r.x, r.z) == (1, 0)).unwrap();
        assert!(!r00.full, "短文件不应判为完整 region");
        assert!(r10.full, "location 表全非零应判为完整 region");
    }
}
