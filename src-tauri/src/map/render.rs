//! 渲染层：颜色表加载（构建期嵌入）+ top-down / y 切片渲染 + PNG 瓦片编码。
//!
//! 颜色表来源：fastnbt 仓库根 `palette.tar.gz`（grass/foliage 色图 + blockstates.json，
//! 4369 键，键格式 `minecraft:stone`）。构建期嵌入（include_bytes!），运行时零网络。

use std::io::Cursor;

use fastanvil::{Chunk, JavaChunk, Palette, RenderedPalette, Rgba};

use super::region_io::ParsedChunk;

/// 官方渲染调色板（fastnbt 仓库根 palette.tar.gz，MIT OR Apache-2.0）。
const PALETTE_TAR_GZ: &[u8] = include_bytes!("../../assets/palette.tar.gz");

/// 懒加载全局调色板（LazyLock：cell 与初始化器一体，免重复解析）。
static PALETTE: std::sync::LazyLock<RenderedPalette> = std::sync::LazyLock::new(|| {
    // 静态资源损坏属于打包事故：expect 早失败（与 constants 硬编码同等地位）
    fastanvil::load_rendered_palette(Cursor::new(PALETTE_TAR_GZ))
        .expect("内置 palette.tar.gz 损坏")
});

/// 获取全局调色板。
pub fn palette() -> &'static RenderedPalette {
    &PALETTE
}

/// 渲染样式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RenderStyle {
    /// top-down：每列取最高不透明方块（自写，比 TopShadeRenderer 少依赖北方 chunk）
    TopDown,
    /// y 切片：固定 Y 层的方块分布（不透明度 < 255 的方块照常着色，空气透明）
    Slice { y: isize },
}

/// 渲染单个 chunk → 256 列 RGBA（行主序：i = z*16 + x）。
/// 未生成（status 非 full 类）的 chunk 返回全透明。
pub fn render_chunk(parsed: &ParsedChunk, style: RenderStyle) -> [Rgba; 256] {
    let status = parsed.chunk.status();
    const OK: [&str; 8] = [
        "full", "spawn", "postprocessed", "fullchunk", "minecraft:full", "minecraft:spawn",
        "minecraft:postprocessed", "minecraft:fullchunk",
    ];
    let mut out = [[0u8, 0, 0, 0]; 256];
    if !OK.contains(&status.as_str()) {
        return out;
    }
    let pal = palette();
    match style {
        RenderStyle::TopDown => render_top_down(&parsed.chunk, pal, &mut out),
        RenderStyle::Slice { y } => render_slice(&parsed.chunk, y, pal, &mut out),
    }
    out
}

/// top-down：用 HeightMode::Calculate 现算表面高（Heightmaps 字段可能缺失），
/// 逐列自最高点向下找首个非 Airy 方块着色。
fn render_top_down(chunk: &JavaChunk, pal: &RenderedPalette, out: &mut [Rgba; 256]) {
    let y_range = chunk.y_range();
    if y_range.is_empty() {
        return;
    }
    for z in 0..16usize {
        for x in 0..16usize {
            let mut y = y_range.end - 1;
            let mut colour: Rgba = [0, 0, 0, 0];
            while colour[3] != 255 && y >= y_range.start {
                let Some(block) = chunk.block(x, y, z) else {
                    y -= 1;
                    continue;
                };
                let biome = chunk.biome(x, y, z);
                let c = pal.pick(block, biome);
                if block.name() == "minecraft:air" || block.name() == "minecraft:cave_air" {
                    y -= 1;
                    continue;
                }
                colour = if c[3] == 255 { c } else { blend(c, colour) };
                if colour[3] == 255 {
                    break;
                }
                y -= 1;
            }
            out[z * 16 + x] = colour;
        }
    }
}

/// y 切片：固定 Y 层。该层方块直接着色（透明方块如玻璃半透叠加下方一层的色）。
fn render_slice(chunk: &JavaChunk, y: isize, pal: &RenderedPalette, out: &mut [Rgba; 256]) {
    for z in 0..16usize {
        for x in 0..16usize {
            let Some(block) = chunk.block(x, y, z) else {
                continue;
            };
            if block.name() == "minecraft:air" || block.name() == "minecraft:cave_air" {
                continue;
            }
            let biome = chunk.biome(x, y, z);
            let c = pal.pick(block, biome);
            out[z * 16 + x] = if c[3] == 255 {
                c
            } else {
                // 半透明方块叠一层下方方块色（简单两层的近似混合）
                let below = chunk.block(x, y - 1, z);
                let under = below
                    .map(|b| pal.pick(b, chunk.biome(x, y - 1, z)))
                    .unwrap_or([0, 0, 0, 0]);
                blend(c, under)
            };
        }
    }
}

/// 简单 alpha 混合：上色叠下色（src 在上）。三通道按 alpha 加权，dst alpha 取大。
fn blend(src: Rgba, dst: Rgba) -> Rgba {
    if src[3] == 255 {
        return src;
    }
    if src[3] == 0 {
        return dst;
    }
    let a = src[3] as u32;
    let mix = |s: u8, d: u8| ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8;
    [
        mix(src[0], dst[0]),
        mix(src[1], dst[1]),
        mix(src[2], dst[2]),
        src[3].max(dst[3]),
    ]
}

/// 256 列 RGBA → PNG 字节（16×16）。失败仅可能为 io 错误，返回 String 供命令层展平。
pub fn encode_png(pixels: &[Rgba; 256]) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity(1024);
    {
        let mut encoder = png::Encoder::new(&mut buf, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG 头写入失败\n{e}"))?;
        let flat: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
        writer
            .write_image_data(&flat)
            .map_err(|e| format!("PNG 数据写入失败\n{e}"))?;
    }
    Ok(buf)
}

// ─────────────────── 测试：合成 fixture ───────────────────
// 程序化构造 Post18 chunk NBT（palette+packed data 布局与游戏一致）→
// mca 写 .mca → region_io 组合层读回 → 渲染断言。全部离线。
#[cfg(test)]
mod tests {
    use super::*;
    use fastnbt::{nbt, LongArray, Value};
    use std::collections::HashMap;
    use std::fs;
    use mca::{Compression, RegionWriter};

    /// 1.16+ 打包位格式：每 i64 按 64/bits 均分（高位在前，无跨 long）。
    fn pack_states(states: &[u16], palette_len: usize) -> Vec<i64> {
        let bits = if palette_len < 16 { 4 } else { (palette_len as f64).log2().ceil() as usize };
        let per = 64 / bits;
        let mut longs = vec![0i64; 4096_usize.div_ceil(per)];
        for (i, &s) in states.iter().enumerate() {
            let long_i = i / per;
            let off = (i % per) * bits;
            longs[long_i] |= ((s as i64) & ((1i64 << bits) - 1)) << off;
        }
        longs
    }

    /// 构造一个 1.18+ chunk NBT（DataVersion 3465 ≈ 1.20.1）：
    /// 底部基岩层 stone，Y=4 层全 oak_planks，其余 air；y 范围 -64..320。
    fn synthetic_chunk_nbt() -> Vec<u8> {
        let palette = vec![
            nbt!({"Name": "minecraft:air"}),
            nbt!({"Name": "minecraft:stone"}),
            nbt!({"Name": "minecraft:oak_planks"}),
        ];
        let mut sections = Vec::new();
        for sy in -4i8..=19i8 {
            // section y=0（世界 y -64..-49）全 stone；y=4（世界 y 64..79）全 oak_planks；
            // 其余 section 只带单值 palette [air] 且【不带 data 字段】——与游戏一致
            // （fastanvil DataInner::at 仅在 data.is_none() && palette.len()==1 时短路，
            //  带空 data 会索引越界）
            let (pal_idx, longs): (Vec<Value>, Option<Vec<i64>>) = match sy {
                0 => (palette.clone(), Some(pack_states(&[1; 4096], 3))),
                4 => (palette.clone(), Some(pack_states(&[2; 4096], 3))),
                _ => (vec![nbt!({"Name": "minecraft:air"})], None),
            };
            let mut bs = HashMap::from([("palette".to_string(), Value::List(pal_idx))]);
            if let Some(l) = longs {
                bs.insert("data".to_string(), Value::LongArray(LongArray::new(l)));
            }
            let block_states = Value::Compound(bs);
            // biomes 必带：fastanvil BiomeData::at 对空 palette（serde default）做
            // (0-1).leading_zeros() 下溢 panic；游戏存档的 section 也总是带单值 biome palette
            let biomes = Value::Compound(HashMap::from([
                ("palette".to_string(), Value::List(vec![Value::String("minecraft:plains".into())])),
            ]));
            sections.push(Value::Compound(HashMap::from([
                ("Y".to_string(), Value::Byte(sy)),
                ("block_states".to_string(), block_states),
                ("biomes".to_string(), biomes),
            ])));
        }
        let root = Value::Compound(HashMap::from([
            ("DataVersion".to_string(), Value::Int(3465)),
            ("Status".to_string(), Value::String("minecraft:full".into())),
            ("sections".to_string(), Value::List(sections)),
        ]));
        fastnbt::to_bytes(&root).unwrap()
    }

    /// 合成 1×1 region 文件（chunk (0,0)）。
    fn synthetic_region_mca() -> Vec<u8> {
        let nbt = synthetic_chunk_nbt();
        let mut writer = RegionWriter::new();
        writer.set_chunk(0, 0, nbt, Compression::default()).unwrap();
        let mut buf = Vec::new();
        writer.write(&mut buf).unwrap();
        buf
    }

    #[test]
    fn synthetic_region_roundtrip_and_render() {
        let mca = synthetic_region_mca();
        let dir = std::env::temp_dir().join(format!("elytra_map_fixture_{}", std::process::id()));
        fs::create_dir_all(dir.join("region")).unwrap();
        let path = dir.join("region").join("r.0.0.mca");
        fs::write(&path, &mca).unwrap();

        // 组合层读回
        let parsed = super::super::region_io::parse_region(&path);
        let first = parsed[0].as_ref().expect("chunk (0,0) 应可解析");
        assert_eq!(first.chunk.status(), "minecraft:full");
        assert!(!first.raw_nbt.is_empty());

        // top-down 渲染：最高不透明方块是 section y=4（世界 y 64..79）的 oak_planks，
        // 16×16 全为 plank 色
        let px = render_chunk(first, RenderStyle::TopDown);
        let plank = [164u8, 133, 80, 255];
        assert_eq!(px[0], plank, "top-down 表面应为 oak_planks 颜色");
        assert_eq!(px[255], plank);

        // y 切片断言（section y*16 = 世界 y 起点）：
        // section y=0 → 世界 y 0..15 全 stone；section y=4 → 世界 y 64..79 全 oak_planks
        let stone = [126u8, 126, 126, 255];
        let px64 = render_chunk(first, RenderStyle::Slice { y: 64 });
        assert_eq!(px64[0], plank, "切片 Y=64 应为 oak_planks");
        let px0 = render_chunk(first, RenderStyle::Slice { y: 0 });
        assert_eq!(px0[0], stone, "切片 Y=0 应为 stone");
        let pxm64 = render_chunk(first, RenderStyle::Slice { y: -64 });
        assert_eq!(pxm64[0], [0, 0, 0, 0], "切片 Y=-64 应为空气（透明）");

        // PNG 编码往返
        let png = encode_png(&px).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn palette_contains_stone_key() {
        // blockstates.json 键格式核验（4369 键；minecraft:stone = [126,126,126,255]）
        // 真正的取色路径由 synthetic_region_roundtrip_and_render 的切片断言覆盖
        let pal = palette();
        let rgb = pal.blockstates.get("minecraft:stone");
        assert_eq!(rgb, Some(&[126u8, 126, 126, 255]));
    }

    #[test]
    fn blend_semantics() {
        assert_eq!(blend([10, 20, 30, 255], [1, 2, 3, 255]), [10, 20, 30, 255]);
        assert_eq!(blend([0, 0, 0, 0], [1, 2, 3, 255]), [1, 2, 3, 255]);
        // 半透明 50% 叠白底 → 半灰
        let mixed = blend([0, 0, 0, 128], [255, 255, 255, 255]);
        assert!(mixed[0] > 120 && mixed[0] < 130);
    }
}
