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
use std::path::Path;

use fastanvil::{JavaChunk, Rgba};
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

/// 列出世界维度目录下全部 region 文件路径（r.X.Z.mca），不递归。
/// `dimension_dir`：主世界=世界根，下界=DIM-1，末地=DIM1。
pub fn list_region_files(dimension_dir: &Path) -> Vec<(i32, i32, std::path::PathBuf)> {
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
        out.push((x, z, entry.path()));
    }
    out.sort();
    out
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
    let Ok(mut reader) = RegionReader::new_with_decompression(&data, JavaWrcDecompression) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(1024);
    for i in 0..1024u16 {
        let (cx, cz) = ChunkPos::from_index(i).coords_in_region();
        out.push(parse_one(&mut reader, cx, cz));
    }
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
    fn region_list_skips_non_region_files() {
        let dir = std::env::temp_dir().join(format!("elytra_map_regions_{}", std::process::id()));
        let region = dir.join("region");
        fs::create_dir_all(&region).unwrap();
        fs::write(region.join("r.0.0.mca"), b"x").unwrap();
        fs::write(region.join("r.1.2.mca"), b"x").unwrap();
        fs::write(region.join("level.dat"), b"x").unwrap();
        fs::write(region.join("r.bad.mca"), b"x").unwrap();
        let got = list_region_files(&dir);
        assert_eq!(got.len(), 2, "只应识别合法 region 名");
        assert_eq!(got[0].0, 0, "按 (x,z) 排序");
        fs::remove_dir_all(&dir).ok();
    }
}
