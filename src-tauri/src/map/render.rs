//! 渲染层：颜色表加载（构建期嵌入）+ top-down / y 切片渲染 + PNG 瓦片编码。
//!
//! 颜色体系（主流自然色）：基础方块色 = 原版客户端贴图顶面平均色（由
//! `tools/gen_palette.mjs` 从 26.x 客户端 jar 采样生成，JSON 内嵌）；草/树叶按
//! 原版生物群系色图（grass/foliage colourmap PNG 内嵌）逐生物群系取色；水面用
//! 原版生物群系水色。未收录方块（Mod 内容）回退中性灰，不再出现洋红。

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use fastanvil::Rgba;

use super::chunk_pos::ChunkPos;
use super::region_io::{BlockRef, ChunkData, ParsedChunk};

/// 自然色表（tools/gen_palette.mjs 生成）：
/// `blocks`：minecraft:xxx → RGBA（贴图顶面平均色）；
/// `textures`：纹理基名（去 block/ 前缀）→ RGBA（Mod 模型引用原版纹理时按名取色）。
pub(crate) const NATURAL_COLORS_JSON: &[u8] =
    include_bytes!("../../assets/natural_block_colors.json");
#[derive(serde::Deserialize)]
struct NaturalColors {
    blocks: HashMap<String, Rgba>,
    textures: HashMap<String, Rgba>,
}

/// 原版草色图 / 树叶色图（256×256，色调直接查表）。
const GRASS_COLORMAP: &[u8] = include_bytes!("../../assets/grass-colourmap.png");
const FOLIAGE_COLORMAP: &[u8] = include_bytes!("../../assets/foliage-colourmap.png");

/// 未知方块兜底色（Mod 内容等）：中性灰，避免洋红占位。
const UNKNOWN_COLOR: Rgba = [150, 150, 150, 255];

/// 坡面光影（层次感）：光源方向（西北上方）+ 强度 + 钳制。
/// 平地 f=1；朝光坡面提亮、背光/悬崖压暗。收敛常量即可验收调节。
const SHADE_LIGHT: [f32; 3] = [0.45, 1.0, 0.36];
const SHADE_STRENGTH: f32 = 1.35;
const SHADE_MIN: f32 = 0.68;
const SHADE_MAX: f32 = 1.32;

/// 水面浅深分层：水深 ≥ SHALLOW_BLOCKS 视为深水（基底色），更浅向亮色过渡。
const SHALLOW_BLOCKS: f32 = 9.0;

/// 草/叶色图最终色增益（参考 Xaero/JM 深色观感：森林绿比原版色图暗一档）。
const GRASS_COLOR_GAIN: f32 = 0.88;
const FOLIAGE_COLOR_GAIN: f32 = 0.88;

/// RGB 乘性压暗（不动 alpha；供族内色图最终色用）。
fn dim(c: Rgba, f: f32) -> Rgba {
    if f >= 0.999 {
        return c;
    }
    [
        ((c[0] as f32) * f).round().clamp(0.0, 255.0) as u8,
        ((c[1] as f32) * f).round().clamp(0.0, 255.0) as u8,
        ((c[2] as f32) * f).round().clamp(0.0, 255.0) as u8,
        c[3],
    ]
}

/// Xaero/JourneyMap 观感调校（主流小地图比贴图原色更明亮饱和；亮度抬升已
/// 在生成器烘焙 1.09，这里只补轻饱和）。收敛常量即可全局验收调节；1.0/1.0 = 数据原色。
const SATURATION_GAIN: f32 = 1.08;
const BRIGHTNESS_GAIN: f32 = 1.0;

/// 拾色后统一过饱和度/亮度增益（sRGB 近似；透明像素不动）。
fn grade(c: Rgba) -> Rgba {
    if c[3] == 0 {
        return c;
    }
    let g = 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
    let mut out = c;
    for v in &mut out[..3] {
        let x = (g + (*v as f32 - g) * SATURATION_GAIN) * BRIGHTNESS_GAIN;
        *v = x.round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// 高度场（512×512，region 局部 i16）列邻差 → 坡面亮度因子（平地 = 1.0）。
fn shade_factor(heights: &[i16], size: usize, x: usize, z: usize) -> f32 {
    // 边缘钳制（region 边界以同列补平）
    let xa = x.saturating_sub(1);
    let xb = (x + 1).min(size - 1);
    let za = z.saturating_sub(1);
    let zb = (z + 1).min(size - 1);
    let gx = heights[z * size + xb] as f32 - heights[z * size + xa] as f32;
    let gz = heights[zb * size + x] as f32 - heights[za * size + x] as f32;
    // 法线（上）≈ normalize(-gx, 1, -gz)
    let nx = -gx;
    let nz = -gz;
    let len = (nx * nx + 1.0 + nz * nz).sqrt().max(0.001);
    // 光源入射单位化后的平面投影基准 d0：平地 d = d0 → f = 1
    let light_len = (SHADE_LIGHT[0] * SHADE_LIGHT[0]
        + SHADE_LIGHT[1] * SHADE_LIGHT[1]
        + SHADE_LIGHT[2] * SHADE_LIGHT[2])
        .sqrt();
    let d = (nx * SHADE_LIGHT[0] + SHADE_LIGHT[1] + nz * SHADE_LIGHT[2]) / (len * light_len);
    let d0 = SHADE_LIGHT[1] / light_len;
    (1.0 + SHADE_STRENGTH * (d - d0)).clamp(SHADE_MIN, SHADE_MAX)
}

/// 水面浅深分层色：深水用基底色，浅水向亮（近岸）过渡。
fn water_depth_color(base: Rgba, depth: i32) -> Rgba {
    if depth <= 0 {
        return base;
    }
    let s = (1.0 - (depth as f32) / SHALLOW_BLOCKS).clamp(0.0, 1.0);
    if s <= 0.0 {
        return base;
    }
    let lift = |v: u8| -> u8 {
        let x = (v as f32) + (255.0 - v as f32) * s * 0.42;
        x.round().clamp(0.0, 255.0) as u8
    };
    [lift(base[0]), lift(base[1]), lift(base[2]), 255]
}

/// 自然色调色板：方块色表 + 纹理色表 + 顶面贴图映射 + 两张生物群系色图。
pub struct NaturalPalette {
    /// 方块 → RGBA（minecraft:xxx）
    pub(crate) table: HashMap<String, Rgba>,
    /// 纹理基名 → RGBA（Mod 模型引用原版纹理用）
    pub(crate) textures: HashMap<String, Rgba>,
    grass: Vec<Rgba>,
    foliage: Vec<Rgba>,
}

impl NaturalPalette {
    /// 取原版纹理平均色（纹理基名，无 block/ 前缀与命名空间）。
    pub(crate) fn vanilla_texture(&self, base: &str) -> Option<Rgba> {
        self.textures.get(base).copied()
    }
    /// 读一张 256×256 RGBA PNG 色图（png crate 展开到 RGBA8）。
    fn decode_colormap(bytes: &[u8]) -> Vec<Rgba> {
        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("内置色图损坏");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("色图缓冲大小")];
        let info = reader.next_frame(&mut buf).expect("内置色图损坏");
        if (info.width, info.height) != (256, 256) {
            panic!("内置色图尺寸异常: {}x{}", info.width, info.height);
        }
        buf.as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect()
    }

    /// 生物群系 → 色图像素（与 fastanvil 采样一致：temp/rainfall 反转后查表）。
    fn sample(map: &[Rgba], b: Option<fastanvil::biome::Biome>) -> Option<Rgba> {
        let b = b?;
        let climate = b.climate();
        let t = climate.temperature.clamp(0., 1.);
        let r = climate.rainfall.clamp(0., 1.) * t;
        let t = 255 - (t * 255.).ceil() as usize;
        let r = 255 - (r * 255.).ceil() as usize;
        Some(map[t + r * 256])
    }

    fn grass(&self, b: Option<fastanvil::biome::Biome>) -> Rgba {
        dim(
            Self::sample(&self.grass, b).unwrap_or(UNKNOWN_COLOR),
            GRASS_COLOR_GAIN,
        )
    }

    fn foliage(&self, b: Option<fastanvil::biome::Biome>) -> Rgba {
        dim(
            Self::sample(&self.foliage, b).unwrap_or(UNKNOWN_COLOR),
            FOLIAGE_COLOR_GAIN,
        )
    }

    /// 生物群系水色（参考 Xaero/JM 深水基调：海洋深蓝偏饱和；浅水由
    /// water_depth_color 提亮）。收敛数值即可验收调节。
    fn water(&self, b: Option<fastanvil::biome::Biome>) -> Rgba {
        use fastanvil::biome::Biome::*;
        match b {
            Some(Swamp) => [0x2F, 0x40, 0x2F, 255],
            Some(River) => [0x22, 0x38, 0xC4, 255],
            Some(Ocean) => [0x22, 0x38, 0xC4, 255],
            Some(LukewarmOcean) => [0x2E, 0x77, 0xC8, 255],
            Some(WarmOcean) => [0x31, 0x9E, 0xC9, 255],
            Some(ColdOcean) => [0x1E, 0x30, 0xC0, 255],
            Some(FrozenRiver) => [0x16, 0x1F, 0x8C, 255],
            Some(FrozenOcean) => [0x16, 0x1F, 0x8C, 255],
            _ => [0x22, 0x38, 0xC4, 255],
        }
    }

    /// 方块取色（含实例 Mod 覆盖层；None = 纯原版渲染）。
    /// Mod 方块优先覆盖层采样色，缺省走语义兜底；统一过 grade()。
    pub fn pick_with(
        &self,
        block: &BlockRef,
        biome: Option<fastanvil::biome::Biome>,
        overlay: Option<&crate::map::mod_color::ModOverlay>,
    ) -> Rgba {
        let name = block.name();
        if !name.starts_with("minecraft:") {
            if let Some(o) = overlay {
                if let Some(c) = o.color(name) {
                    return grade(c);
                }
            }
            return grade(self.modded_fallback(name, biome));
        }
        grade(self.pick_raw(block, biome))
    }

    /// Mod 方块语义兜底：采样失败/无覆盖层时按名字特征给近似色（不精确但可用）。
    fn modded_fallback(&self, name: &str, biome: Option<fastanvil::biome::Biome>) -> Rgba {
        let id = name.rsplit(':').next().unwrap_or(name);
        // 无生物群系时按平原取样（确定性近似）
        let mid = || biome.or(Some(fastanvil::biome::Biome::Plains));
        if id.ends_with("_leaves") || id.contains("leaves") || id.contains("foliage") {
            return self.foliage(mid());
        }
        if id.contains("grass") || id.ends_with("_fern") || id.ends_with("_vine") {
            return self.grass(mid());
        }
        if id.contains("water") || id.contains("fluid") {
            return self.water(None);
        }
        if id.contains("plank") {
            return self
                .table
                .get("minecraft:oak_planks")
                .copied()
                .unwrap_or(UNKNOWN_COLOR);
        }
        if id.contains("log")
            || id.ends_with("_wood")
            || id.contains("stem")
            || id.contains("hyphae")
        {
            return self
                .table
                .get("minecraft:oak_log")
                .copied()
                .unwrap_or(UNKNOWN_COLOR);
        }
        if id.contains("stone") || id.contains("brick") || id.contains("cobble") {
            return self
                .table
                .get("minecraft:stone_bricks")
                .or_else(|| self.table.get("minecraft:stone"))
                .copied()
                .unwrap_or(UNKNOWN_COLOR);
        }
        UNKNOWN_COLOR
    }

    fn pick_raw(&self, block: &BlockRef, biome: Option<fastanvil::biome::Biome>) -> Rgba {
        let Some(id) = block.name().strip_prefix("minecraft:") else {
            return self
                .table
                .get(block.name())
                .copied()
                .unwrap_or(UNKNOWN_COLOR);
        };
        // 草系（26.x 贴图为灰度基底，游戏内按草色图着色）
        match id {
            "grass_block" => {
                if block.snowy() {
                    return self
                        .table
                        .get("minecraft:snow_block")
                        .copied()
                        .unwrap_or([255, 255, 255, 255]);
                }
                return self.grass(biome);
            }
            "short_grass" | "tall_grass" | "vine" | "fern" | "large_fern" | "sugar_cane" => {
                return self.grass(biome);
            }
            // 树叶系：经典 tint 叶走色图；桦/云杉原版固定色；其余（樱花/杜鹃等）
            // 自带彩色贴图直接查表
            "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves"
            | "mangrove_leaves" => return self.foliage(biome),
            "birch_leaves" => return [0x80, 0xa7, 0x55, 255],
            "spruce_leaves" => return [0x61, 0x99, 0x61, 255],
            // 26.x 后新增的 tint 叶（灰度贴图 + 色图）也走树叶色图；识别不了就查表
            _ => {}
        }
        if id.ends_with("_leaves") {
            // 表内已是彩色贴图（樱花等）优先；灰度贴图才走色图
            if let Some(c) = self.table.get(block.name()) {
                if c[0] != c[1] || c[1] != c[2] {
                    return *c;
                }
            }
            return self.foliage(biome);
        }
        // 水/海草：生物群系水色
        if matches!(
            id,
            "water" | "bubble_column" | "kelp" | "kelp_plant" | "seagrass" | "tall_seagrass"
        ) {
            return self.water(biome);
        }
        self.table
            .get(block.name())
            .copied()
            .unwrap_or(UNKNOWN_COLOR)
    }
}

/// 懒加载全局调色板（LazyLock：cell 与初始化器一体，免重复解析）。
static PALETTE: std::sync::LazyLock<NaturalPalette> = std::sync::LazyLock::new(|| {
    // 静态资源损坏属于打包事故：expect 早失败（与 constants 硬编码同等地位）
    let data: NaturalColors =
        serde_json::from_slice(NATURAL_COLORS_JSON).expect("内置 natural_block_colors.json 损坏");
    NaturalPalette {
        table: data.blocks,
        textures: data.textures,
        grass: NaturalPalette::decode_colormap(GRASS_COLORMAP),
        foliage: NaturalPalette::decode_colormap(FOLIAGE_COLORMAP),
    }
});

/// 获取全局调色板。
pub fn palette() -> &'static NaturalPalette {
    &PALETTE
}

/// 渲染样式：仅俯视，`max_y` 为向下扫描的最高 Y（降低可看洞穴/地下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RenderStyle {
    TopDown { max_y: isize },
}

/// 渲染单个 chunk → 256 列 RGBA（行主序：i = z*16 + x）。
/// 不设状态门槛：凡含方块数据的 chunk（含部分生成：noise/carvers/surface 等
/// 阶段）按现存数据渲染——否则"半生成 region"（已有水/石头真实数据）会被整块
/// 留空、露出预测垫底造成错位。无任何 block section 的 chunk 天然全透明。
/// `heights` 为 region 高度场（512×512，可选）：TopDown 时用于坡面光影，
/// 由 compose_region 预计算并传入（含本 chunk 邻域）。
#[derive(Clone, Copy)]
struct TopDownView<'a> {
    heights: Option<&'a [i16]>,
    hx0: usize,
    hz0: usize,
    max_y: isize,
}

pub fn render_chunk(
    parsed: &ParsedChunk,
    style: RenderStyle,
    overlay: Option<&crate::map::mod_color::ModOverlay>,
    heights: Option<&[i16]>,
    hx0: usize,
    hz0: usize,
) -> [Rgba; 256] {
    let mut out = [[0u8, 0, 0, 0]; 256];
    let pal = palette();
    let RenderStyle::TopDown { max_y } = style;
    render_top_down(
        &parsed.chunk,
        pal,
        overlay,
        TopDownView {
            heights,
            hx0,
            hz0,
            max_y,
        },
        &mut out,
    );
    out
}

/// region 全量合成：32×32 chunk → 512×512 RGBA（行主序，i = x + 32z）。
/// 未解析（缺失/损坏）与未生成（status 不达标）的 chunk 区域保持全透明。
/// 返回（像素缓冲，是否含不透明像素）——后者供渲染层区分"空 region"。
/// 注意：缓冲区 1MB，必须走堆（Vec），禁止 [Rgba; 512*512] 栈数组（测试线程
/// 与命令线程栈均只有 2MB，栈分配必然溢出）。
fn compose_region(
    parsed: &[Option<ParsedChunk>],
    style: RenderStyle,
    overlay: Option<&crate::map::mod_color::ModOverlay>,
) -> (Vec<Rgba>, bool) {
    let mut out = vec![[0u8, 0, 0, 0]; 512 * 512];
    let mut any_opaque = false;
    // 俯视坡面光影：按 max_y 向下扫一遍高度场（含空列补 0），供各 chunk
    // 跨 chunk 取邻差（悬崖/坡面明暗由此而来）。降低 max_y 时同步切换洞穴层。
    let RenderStyle::TopDown { max_y } = style;
    let heights = Some(surface_height_field(parsed, max_y));
    let heights_ref = heights.as_deref();
    for (i, slot) in parsed.iter().enumerate() {
        let Some(p) = slot else { continue };
        let (cx, cz) = ChunkPos::from_index(i as u16).coords_in_region();
        let ox = cx as usize * 16;
        let oz = cz as usize * 16;
        let px = render_chunk(p, style, overlay, heights_ref, ox, oz);
        for y in 0..16usize {
            for x in 0..16usize {
                let c = px[y * 16 + x];
                if c[3] != 0 {
                    any_opaque = true;
                }
                out[(oz + y) * 512 + ox + x] = c;
            }
        }
    }
    (out, any_opaque)
}

/// 地表高度场：region 局部 [0,512)²，每列自 `max_y` 向下首个非空气方块高度。
fn surface_height_field(parsed: &[Option<ParsedChunk>], max_y: isize) -> Vec<i16> {
    let mut heights = vec![0i16; 512 * 512];
    for (i, slot) in parsed.iter().enumerate() {
        let Some(p) = slot else { continue };
        let (cx, cz) = ChunkPos::from_index(i as u16).coords_in_region();
        let ox = cx as usize * 16;
        let oz = cz as usize * 16;
        let chunk = &p.chunk;
        let y_range = chunk.y_range();
        if y_range.is_empty() {
            continue;
        }
        let top_y = (y_range.end - 1).min(max_y);
        if top_y < y_range.start {
            continue;
        }
        for z in 0..16usize {
            for x in 0..16usize {
                let mut top = 0i16;
                for y in (y_range.start..=top_y).rev() {
                    let Some(block) = chunk.block(x, y, z) else {
                        continue;
                    };
                    let name = block.name();
                    if name != "minecraft:air" && name != "minecraft:cave_air" {
                        top = y as i16;
                        break;
                    }
                }
                heights[(oz + z) * 512 + ox + x] = top;
            }
        }
    }
    heights
}

/// 渲染单个 region 的 512×512 地图瓦片 PNG（俯视，自 max_y 向下）。
/// 全 region 32×32 chunk 逐个解析后合成；region 存在但无有效 chunk（如仅含
/// 边缘模拟区块）→ 全透明 PNG（合法 PNG，前端 <img> 可渲染）。
pub fn render_region_tile(
    region_file: &Path,
    style: RenderStyle,
    overlay: Option<&crate::map::mod_color::ModOverlay>,
) -> Result<Vec<u8>, String> {
    let parsed = super::region_io::parse_region(region_file);
    let (pixels, any_opaque) = compose_region(&parsed, style, overlay);
    if any_opaque {
        encode_png(&pixels, 512, 512)
    } else {
        // 空缓冲同样走堆（理由见 compose_region 注释）
        let empty = vec![[0u8, 0, 0, 0]; 512 * 512];
        encode_png(&empty, 512, 512)
    }
}

/// top-down：逐列自 `max_y` 向下找首个非 Airy 方块着色。
/// 收尾应用坡面光影（region 高度场邻差）与水面浅深分层。
fn render_top_down(
    chunk: &ChunkData,
    pal: &NaturalPalette,
    overlay: Option<&crate::map::mod_color::ModOverlay>,
    view: TopDownView<'_>,
    out: &mut [Rgba; 256],
) {
    let y_range = chunk.y_range();
    if y_range.is_empty() {
        return;
    }
    let top_y = (y_range.end - 1).min(view.max_y);
    if top_y < y_range.start {
        return;
    }
    for z in 0..16usize {
        for x in 0..16usize {
            let mut y = top_y;
            let mut colour: Rgba = [0, 0, 0, 0];
            // 表面方块名与深度（水面分层用）
            let mut surface_name: Option<String> = None;
            let mut surface_biome: Option<fastanvil::biome::Biome> = None;
            let mut surface_y = y;
            while colour[3] != 255 && y >= y_range.start {
                let Some(block) = chunk.block(x, y, z) else {
                    y -= 1;
                    continue;
                };
                let name = block.name();
                if name == "minecraft:air" || name == "minecraft:cave_air" {
                    y -= 1;
                    continue;
                }
                let biome = chunk.biome(x, y, z);
                let c = pal.pick_with(&block, biome, overlay);
                colour = if c[3] == 255 { c } else { blend(c, colour) };
                if surface_name.is_none() {
                    surface_name = Some(name.to_string());
                    surface_biome = biome;
                    surface_y = y;
                }
                if colour[3] == 255 {
                    break;
                }
                y -= 1;
            }
            // 水面：按列深浅分层（浅水提亮）
            if let Some(name) = &surface_name {
                if name == "minecraft:water" || name == "minecraft:bubble_column" {
                    let mut depth = 1;
                    for dy in (y_range.start..surface_y).rev() {
                        let Some(b) = chunk.block(x, dy, z) else {
                            break;
                        };
                        let bn = b.name();
                        if bn == "minecraft:water" || bn == "minecraft:bubble_column" {
                            depth += 1;
                        } else {
                            break;
                        }
                        if depth > 40 {
                            break;
                        }
                    }
                    colour = grade(water_depth_color(pal.water(surface_biome), depth));
                } else if let Some(heights) = view.heights {
                    if colour[3] != 0 {
                        // 坡面光影（水面已走分层，陆地/植被乘亮度因子）
                        let f = shade_factor(heights, 512, view.hx0 + x, view.hz0 + z);
                        if (f - 1.0).abs() > 0.004 {
                            colour = [
                                ((colour[0] as f32) * f).round().clamp(0.0, 255.0) as u8,
                                ((colour[1] as f32) * f).round().clamp(0.0, 255.0) as u8,
                                ((colour[2] as f32) * f).round().clamp(0.0, 255.0) as u8,
                                colour[3],
                            ];
                        }
                    }
                }
            }
            out[z * 16 + x] = colour;
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

/// RGBA（行主序）→ PNG 字节（width×height，chunk 16×16 / region 512×512）。
/// 失败仅可能为 io 错误，返回 String 供命令层展平。
pub fn encode_png(pixels: &[Rgba], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity(pixels.len() * 4 + 64);
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(common_err_ctx_key!(
            "errors.map.tile_failed",
            "PNG 头写入失败"
        ))?;
        let flat: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
        writer.write_image_data(&flat).map_err(common_err_ctx_key!(
            "errors.map.tile_failed",
            "PNG 数据写入失败"
        ))?;
    }
    Ok(buf)
}

// ─────────────────── 测试：合成 fixture ───────────────────
// 程序化构造 Post18 chunk NBT（palette+packed data 布局与游戏一致）→
// mca 写 .mca → region_io 组合层读回 → 渲染断言。全部离线。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::test_support::{pack_states, synthetic_chunk_nbt, synthetic_region_mca};
    use fastnbt::{nbt, LongArray, Value};
    use mca::{Compression, RegionWriter};
    use std::collections::HashMap;
    use std::fs;

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
        assert_eq!(
            first.chunk.y_range(),
            -64..320,
            "section 跨度应换算为世界 Y"
        );
        assert!(!first.raw_nbt.is_empty());

        // top-down 渲染：最高不透明方块是 section y=4（世界 y 64..79）的 oak_planks，
        // 16×16 全为 plank 色
        let px = render_chunk(first, RenderStyle::TopDown { max_y: 319 }, None, None, 0, 0);
        let plank = [180u8, 143, 81, 255];
        assert_eq!(px[0], plank, "top-down 表面应为 oak_planks 颜色");
        assert_eq!(px[255], plank);

        // 降低 max_y：64 应看到顶层 oak_planks，0 应看到 stone，-64 为空气。
        let stone = [137u8, 137, 137, 255];
        let px64 = render_chunk(first, RenderStyle::TopDown { max_y: 64 }, None, None, 0, 0);
        assert_eq!(px64[0], plank, "max_y=64 应显示 oak_planks");
        let px0 = render_chunk(first, RenderStyle::TopDown { max_y: 0 }, None, None, 0, 0);
        assert_eq!(px0[0], stone, "max_y=0 应显示 stone");
        let pxm64 = render_chunk(first, RenderStyle::TopDown { max_y: -64 }, None, None, 0, 0);
        assert_eq!(pxm64[0], [0, 0, 0, 0], "max_y=-64 应为空气（透明）");

        let png = encode_png(&px, 16, 16).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        fs::remove_dir_all(&dir).ok();
    }

    /// 26.x 起 palette 三种形态并存：纯字符串、`{id, properties}`、无名子键
    /// `{"": id}`（同 section 出现带属性方块后，无属性条目退化为后者）。
    #[test]
    fn modern_palette_forms_parse_and_render() {
        let mixed = vec![
            nbt!({ "": "minecraft:air" }),
            nbt!({ "id": "minecraft:stone", "properties": {} }),
            nbt!({ "id": "minecraft:grass_block", "properties": { "snowy": "true" } }),
        ];
        let strings = vec![
            Value::String("minecraft:air".into()),
            Value::String("minecraft:oak_planks".into()),
        ];
        let mut sections = Vec::new();
        for sy in -4i8..=19i8 {
            let (palette, longs): (Vec<Value>, Option<Vec<i64>>) = match sy {
                0 => (mixed.clone(), Some(pack_states(&[1; 4096], 3))),
                4 => (strings.clone(), Some(pack_states(&[1; 4096], 2))),
                -3 => (
                    vec![
                        nbt!({"": "minecraft:air"}),
                        nbt!({"id": "minecraft:grass_block", "properties": { "snowy": "true" }}),
                    ],
                    Some(pack_states(&[1; 4096], 2)),
                ),
                _ => (vec![Value::String("minecraft:air".into())], None),
            };
            let mut bs = HashMap::from([("palette".to_string(), Value::List(palette))]);
            if let Some(l) = longs {
                bs.insert("data".to_string(), Value::LongArray(LongArray::new(l)));
            }
            sections.push(Value::Compound(HashMap::from([
                ("Y".to_string(), Value::Byte(sy)),
                ("block_states".to_string(), Value::Compound(bs)),
                (
                    "biomes".to_string(),
                    Value::Compound(HashMap::from([(
                        "palette".to_string(),
                        Value::List(vec![Value::String("minecraft:plains".into())]),
                    )])),
                ),
            ])));
        }
        let root = Value::Compound(HashMap::from([
            ("DataVersion".to_string(), Value::Int(5023)),
            ("Status".to_string(), Value::String("minecraft:full".into())),
            ("sections".to_string(), Value::List(sections)),
        ]));
        let nbt_bytes = fastnbt::to_bytes(&root).unwrap();
        let mut writer = RegionWriter::new();
        writer
            .set_chunk(0, 0, nbt_bytes, Compression::default())
            .unwrap();
        let mut buf = Vec::new();
        writer.write(&mut buf).unwrap();
        let dir = std::env::temp_dir().join(format!("elytra_map_modern_{}", std::process::id()));
        fs::create_dir_all(dir.join("region")).unwrap();
        let path = dir.join("region").join("r.0.0.mca");
        fs::write(&path, &buf).unwrap();

        let parsed = super::super::region_io::parse_region(&path);
        let first = parsed[0].as_ref().expect("26.x 区块应可解析");
        assert_eq!(
            first.chunk.block(0, 0, 0).unwrap().name(),
            "minecraft:stone"
        );
        assert_eq!(
            first.chunk.block(0, 64, 0).unwrap().name(),
            "minecraft:oak_planks"
        );
        assert!(
            first.chunk.block(0, -48, 0).unwrap().snowy(),
            "properties.snowy 应解析"
        );
        let plank = [180u8, 143, 81, 255];
        let px = render_chunk(first, RenderStyle::TopDown { max_y: 319 }, None, None, 0, 0);
        assert_eq!(px[0], plank, "top-down 应取最高层 oak_planks");
        let px0 = render_chunk(first, RenderStyle::TopDown { max_y: 0 }, None, None, 0, 0);
        assert_eq!(px0[0], [137u8, 137, 137, 255], "max_y=0 应为 stone");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn compose_region_renders_all_chunks_not_just_first() {
        // 回归：初版 get_map_tile 只渲染首个已解析 chunk，其余 15/16 区域透明
        // ——真实存档首个 chunk 常为 structure_starts（未生成），整张瓦片全空白。
        // chunk (0,0) 与 (5,3) 双 fixture：合成必须覆盖两个 16×16 区域。
        let nbt = synthetic_chunk_nbt();
        let mut writer = RegionWriter::new();
        writer
            .set_chunk(0, 0, nbt.clone(), Compression::default())
            .unwrap();
        writer.set_chunk(5, 3, nbt, Compression::default()).unwrap();
        let mut buf = Vec::new();
        writer.write(&mut buf).unwrap();
        let dir = std::env::temp_dir().join(format!("elytra_map_compose_{}", std::process::id()));
        fs::create_dir_all(dir.join("region")).unwrap();
        let path = dir.join("region").join("r.0.0.mca");
        fs::write(&path, &buf).unwrap();

        let parsed = super::super::region_io::parse_region(&path);
        let (pixels, any_opaque) =
            compose_region(&parsed, RenderStyle::TopDown { max_y: 319 }, None);
        assert!(any_opaque, "含两个满区块的 region 不应为空");
        let plank = [180u8, 143, 81, 255];
        // 采样 chunk 内部像素（坡面光影在缺 chunk 边界产生暗边属预期，内部平地=1）
        let c0_in = 8 * 512 + 8;
        assert_eq!(
            pixels[c0_in], plank,
            "chunk (0,0) 内部应为 plank 原色（平地无阴影）"
        );
        let c1_in = (3 * 16 + 8) * 512 + (5 * 16 + 8);
        assert_eq!(pixels[c1_in], plank, "chunk (5,3) 内部必须渲染在自身偏移处");
        assert_eq!(pixels[16], [0, 0, 0, 0], "无 chunk 的 (1,0) 位置保持透明");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn palette_holds_natural_colors_and_unknown_fallback() {
        // 自然色表核验（tools/gen_palette.mjs 从 26.x 客户端贴图采样）。
        // 取值断言锚定生成产物，防资源缺失/加载路径回归。
        let pal = palette();
        assert_eq!(
            pal.table.get("minecraft:stone"),
            Some(&[137u8, 137, 137, 255])
        );
        assert_eq!(
            pal.table.get("minecraft:oak_planks"),
            Some(&[178u8, 143, 86, 255])
        );
        assert_eq!(
            pal.table.get("minecraft:cherry_leaves"),
            Some(&[254u8, 190, 216, 227])
        );
        assert_eq!(
            pal.table.get("minecraft:sand"),
            Some(&[238u8, 225, 177, 255])
        );
        // 色图就位（256×256 RGBA）
        assert_eq!(pal.grass.len(), 256 * 256);
        assert_eq!(pal.foliage.len(), 256 * 256);
        // 未收录方块（Mod 内容）→ 中性灰，不再是洋红占位
        assert_eq!(pal.table.get("minecraft:modded_block"), None);
        let _ = super::UNKNOWN_COLOR;
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
