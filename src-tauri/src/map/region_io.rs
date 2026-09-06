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
//! 组合方案：mca 读 region（全版本统一 region 层）→ fastnbt → fastanvil JavaChunk
//! （Post18/Pre18/Pre13 三级回退，`JavaChunk::from_bytes` 内置）。

use std::fs;
use std::path::{Path, PathBuf};

use fastanvil::{Chunk, JavaChunk, Rgba};
use mca::{CompressionError, CustomDecompression, RegionReader};

use super::chunk_pos::ChunkPos;

/// region 文件内单 chunk 解析结果。
pub struct ParsedChunk {
    pub chunk: JavaChunk,
    /// 原始 NBT 字节（未解析快照；区块复制通道直接用，避免二次解压）
    pub raw_nbt: Vec<u8>,
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
            decoder
                .read_to_end(out)
                .map_err(CompressionError::LZ4)?;
            Ok(out.len())
        } else {
            Err(CompressionError::Unsupported)
        }
    }
}

/// 维度目录解析：空串=主世界（26.x 存档在 dimensions/minecraft/overworld，
/// 旧布局在世界根）；DIM-1/DIM1 同理带 fallback。region 文件/缓存键/列表
/// 必须共用此解析——否则 26.x 布局下列表有 region 而瓦片查不到文件。
pub fn dimension_dir(world_path: &Path, dimension: &str) -> PathBuf {
    let base = world_path.to_path_buf();
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
            let ns = base
                .join("dimensions/minecraft")
                .join(if d == "DIM-1" { "the_nether" } else { "the_end" });
            if ns.join("region").exists() {
                ns
            } else {
                base.join(d)
            }
        }
        other => base.join(other),
    }
}

/// region 概要（前端视口定位 + 变更检测；序列化给地图编辑器前端）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MapRegionInfo {
    pub x: i32,
    pub z: i32,
    /// region 文件最后修改（unix 秒）；前端据此检测变更并增量重取瓦片。
    pub mtime: u64,
    /// region 是否 1024 chunk 全覆盖（满 region 无空洞 → 预测层无需垫底）。
    /// 读 8KB 头部 location 表：非零扇区条目 == 1024 即满。
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
        .chunks_exact(4)
        .filter(|c| c[0] != 0)
        .count();
    count == 1024
}

/// 解析 "r.X.Z.mca" 文件名 → region 坐标（容忍 .mcr 旧扩展）。
fn parse_region_name(name: &str) -> Option<(i32, i32)> {
    let base = name.strip_suffix(".mca").or_else(|| name.strip_suffix(".mcr"))?;
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
    let Ok(data) = fs::read(path) else {
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
        let data_ref = &data;
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
    // fastanvil::JavaChunk::from_bytes 对合法 NBT 做三代回退解析；
    // 解析失败（未生成/损坏 chunk）返回 None 跳过，不影响 region 内其余 chunk
    let chunk = match JavaChunk::from_bytes(raw) {
        Ok(c) => c,
        Err(_) => return None,
    };
    Some(ParsedChunk {
        chunk,
        raw_nbt: raw.to_vec(),
    })
}

/// 单 chunk 256 列颜色（top-down 或 y 切片渲染器产出后经此处取色）。
pub type ChunkPixels = [Rgba; 256];

/// 世界方块坐标 → 所属 region 文件路径（不含验证；跨 dimension_dir 布局）。
pub fn region_file_for(world_path: &Path, dimension: &str, bx: i32, bz: i32) -> PathBuf {
    let rx = bx.div_euclid(512);
    let rz = bz.div_euclid(512);
    dimension_dir(world_path, dimension)
        .join("region")
        .join(format!("r.{rx}.{rz}.mca"))
}

/// 指定方块坐标的表面方块（列首非空气名）+ 所在群系原始 id。
/// 群系经 chunk NBT biome palette 直读（fastanvil 会把 26.x/Mod 群系折叠为
/// Unknown，这里绕开）：返回完整 id 形如 "minecraft:plains"。region 缺失/
/// 区块未生成/损坏 → None。
pub fn surface_info(
    region_file: &Path,
    bx: i32,
    bz: i32,
) -> Option<(String, Option<String>)> {
    let data = fs::read(region_file).ok()?;
    let mut reader = RegionReader::new_with_decompression(&data, JavaWrcDecompression).ok()?;
    let cx = bx.rem_euclid(512) / 16;
    let cz = bz.rem_euclid(512) / 16;
    let parsed = parse_one(&mut reader, cx as u8, cz as u8)?;
    let y_range = parsed.chunk.y_range();
    if y_range.is_empty() {
        return None;
    }
    let x = bx.rem_euclid(16) as usize;
    let z = bz.rem_euclid(16) as usize;
    for y in y_range.rev() {
        let Some(block) = parsed.chunk.block(x, y, z) else {
            continue;
        };
        let name = block.name();
        if name != "minecraft:air" && name != "minecraft:cave_air" {
            let biome = biome_id_at(&parsed.raw_nbt, x, z, y);
            return Some((name.to_string(), biome));
        }
    }
    None
}

/// 指定 (bx, bz, y) 处的精确探测：切片模式悬停用——返回该层的方块（含空气）
/// 与该层覆盖的群系单元（4×4×4 网格按 y 定位，地下显示的是地下群系而非地表）。
pub fn probe_at(
    region_file: &Path,
    bx: i32,
    bz: i32,
    y: isize,
) -> Option<(String, Option<String>)> {
    let data = fs::read(region_file).ok()?;
    let mut reader = RegionReader::new_with_decompression(&data, JavaWrcDecompression).ok()?;
    let cx = bx.rem_euclid(512) / 16;
    let cz = bz.rem_euclid(512) / 16;
    let parsed = parse_one(&mut reader, cx as u8, cz as u8)?;
    let y_range = parsed.chunk.y_range();
    if y_range.is_empty() {
        return None;
    }
    // y 越界钳到可用范围（生物群系单元仍取最近一层）
    let yb = y.clamp(y_range.start, y_range.end - 1);
    let x = bx.rem_euclid(16) as usize;
    let z = bz.rem_euclid(16) as usize;
    let name = parsed
        .chunk
        .block(x, yb, z)
        .map(|b| b.name().to_string())
        .unwrap_or_else(|| "minecraft:air".to_string());
    let biome = biome_id_at(&parsed.raw_nbt, x, z, yb);
    Some((name, biome))
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
        let fastnbt::Value::Compound(sec) = s else { continue };
        let fastnbt::Value::Byte(sy) = sec.get("Y")? else { continue };
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
        let pos = idx * bits;
        let word = pos / 64;
        let shift = pos % 64;
        if word >= data.len() {
            return None;
        }
        let mask = (1i64 << bits.min(63)) - 1;
        let v = ((data[word] >> shift) & mask) as usize;
        return names.get(v).cloned();
    }
    None
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
}
