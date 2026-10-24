use std::collections::HashMap;
use std::sync::LazyLock;

use super::ffi;
use super::model::{SeedMapBlock, SeedMapSlimeChunk, SeedMapStructure};

const CELLS: usize = 128;
const CELL_PX: usize = 4;
const TILE_PX: usize = CELLS * CELL_PX;

static PALETTE: LazyLock<HashMap<&'static str, [u8; 4]>> = LazyLock::new(|| {
    [
        ("ocean", [44, 84, 156]),
        ("deep_ocean", [24, 46, 100]),
        ("cold_ocean", [88, 140, 192]),
        ("deep_cold_ocean", [62, 100, 148]),
        ("frozen_ocean", [120, 150, 190]),
        ("deep_frozen_ocean", [96, 126, 168]),
        ("lukewarm_ocean", [60, 120, 178]),
        ("deep_lukewarm_ocean", [44, 96, 152]),
        ("warm_ocean", [52, 150, 196]),
        ("deep_warm_ocean", [40, 110, 160]),
        ("river", [54, 110, 172]),
        ("frozen_river", [98, 140, 188]),
        ("beach", [214, 208, 152]),
        ("snowy_beach", [212, 214, 210]),
        ("stony_shore", [132, 134, 132]),
        ("swamp", [64, 96, 66]),
        ("mangrove_swamp", [56, 84, 66]),
        ("plains", [138, 176, 84]),
        ("sunflower_plains", [138, 176, 84]),
        ("snowy_plains", [224, 232, 232]),
        ("ice_spikes", [210, 226, 236]),
        ("forest", [52, 128, 54]),
        ("flower_forest", [88, 148, 80]),
        ("birch_forest", [98, 148, 80]),
        ("old_growth_birch_forest", [104, 156, 84]),
        ("dark_forest", [38, 88, 42]),
        ("cherry_grove", [214, 160, 168]),
        ("dappled_forest", [176, 152, 102]),
        ("taiga", [44, 108, 84]),
        ("snowy_taiga", [72, 132, 116]),
        ("old_growth_pine_taiga", [50, 104, 82]),
        ("old_growth_spruce_taiga", [44, 96, 78]),
        ("jungle", [74, 148, 52]),
        ("bamboo_jungle", [60, 140, 52]),
        ("sparse_jungle", [96, 156, 74]),
        ("savanna", [176, 168, 88]),
        ("savanna_plateau", [164, 152, 80]),
        ("windswept_savanna", [152, 148, 82]),
        ("badlands", [218, 148, 92]),
        ("eroded_badlands", [206, 128, 78]),
        ("wooded_badlands", [188, 136, 92]),
        ("meadow", [132, 176, 92]),
        ("grove", [120, 156, 110]),
        ("snowy_slopes", [226, 230, 228]),
        ("frozen_peaks", [232, 238, 238]),
        ("jagged_peaks", [178, 186, 188]),
        ("stony_peaks", [146, 150, 152]),
        ("windswept_hills", [114, 126, 112]),
        ("windswept_gravelly_hills", [122, 128, 120]),
        ("windswept_forest", [86, 116, 82]),
        ("mushroom_fields", [184, 132, 166]),
        ("dripstone_caves", [124, 96, 74]),
        ("lush_caves", [88, 140, 96]),
        ("deep_dark", [28, 34, 44]),
        ("pale_garden", [172, 190, 172]),
        ("sulfur_caves", [190, 160, 70]),
        ("desert", [218, 192, 116]),
    ]
    .into_iter()
    .map(|(name, rgb)| (name, [rgb[0], rgb[1], rgb[2], 255]))
    .collect()
});

fn biome_color(raw: &str) -> [u8; 4] {
    let name = raw.strip_prefix("minecraft:").unwrap_or(raw);
    if let Some(c) = PALETTE.get(name) {
        return *c;
    }
    let alias = match name {
        "mountains" | "extreme_hills" => "windswept_hills",
        "snowy_tundra" => "snowy_plains",
        "snowy_mountains" => "snowy_slopes",
        "wooded_mountains" => "windswept_forest",
        "gravelly_mountains" => "windswept_gravelly_hills",
        "stone_shore" => "stony_shore",
        "jungle_edge" => "sparse_jungle",
        "jungle_hills" => "jungle",
        "giant_tree_taiga" => "old_growth_pine_taiga",
        "giant_spruce_taiga" => "old_growth_spruce_taiga",
        "shattered_savanna" => "windswept_savanna",
        "shattered_savanna_plateau" => "windswept_savanna",
        "badlands_plateau" => "badlands",
        "wooded_badlands_plateau" => "wooded_badlands",
        "modified_badlands_plateau" => "eroded_badlands",
        "tall_birch_forest" => "old_growth_birch_forest",
        "birch_forest_hills" => "birch_forest",
        "dark_forest_hills" => "dark_forest",
        "taiga_hills" | "taiga_mountains" => "taiga",
        "snowy_taiga_hills" | "snowy_taiga_mountains" => "snowy_taiga",
        "desert_hills" | "desert_lakes" => "desert",
        "forest_hills" => "forest",
        "mushroom_field_shore" => "mushroom_fields",
        "swamp_hills" => "swamp",
        _ => name,
    };
    if let Some(c) = PALETTE.get(alias) {
        return *c;
    }
    let h = crate::util::fnv1a64(name);
    let r = 80 + (h & 0x7f) as u8;
    let g = 90 + ((h >> 8) & 0x7f) as u8;
    let b = 80 + ((h >> 16) & 0x7f) as u8;
    [r, g, b, 255]
}

pub fn render_tile(version: &str, seed: i64, rx: i32, rz: i32) -> Result<Vec<u8>, String> {
    let mc = ffi::mc_from_name(version).ok_or_else(|| format!("版本不受支持\n{version}"))?;
    let y = ffi::max_y_quart(mc);
    let cell_x = rx
        .checked_mul(CELLS as i32)
        .ok_or_else(|| "瓦片坐标越界".to_string())?;
    let cell_z = rz
        .checked_mul(CELLS as i32)
        .ok_or_else(|| "瓦片坐标越界".to_string())?;
    let gen = ffi::Generator::new(version, seed)?;
    let mut ids = vec![0i32; CELLS * CELLS];
    gen.gen_biomes(
        ffi::BiomeRange {
            scale: 4,
            x: cell_x,
            z: cell_z,
            sx: CELLS as i32,
            sz: CELLS as i32,
            y,
        },
        &mut ids,
    )?;

    let mut colors: HashMap<i32, [u8; 4]> = HashMap::new();
    let mut buf = vec![0u8; TILE_PX * TILE_PX * 4];
    for cz in 0..CELLS {
        for cx in 0..CELLS {
            let id = ids[cz * CELLS + cx];
            let color = *colors
                .entry(id)
                .or_insert_with(|| biome_color(&ffi::biome_name(mc, id)));
            for dy in 0..CELL_PX {
                let row = cz * CELL_PX + dy;
                let start = (row * TILE_PX + cx * CELL_PX) * 4;
                for dx in 0..CELL_PX {
                    buf[start + dx * 4..start + dx * 4 + 4].copy_from_slice(&color);
                }
            }
        }
    }
    encode_png(&buf)
}

pub fn biome_at(version: &str, seed: i64, x: i32, z: i32) -> Result<SeedMapBlock, String> {
    let mc = ffi::mc_from_name(version).ok_or_else(|| format!("版本不受支持\n{version}"))?;
    let gen = ffi::Generator::new(version, seed)?;
    let y = ffi::max_y_quart(mc);
    let id = gen.biome_at(4, x.div_euclid(4), y, z.div_euclid(4));
    if id < 0 {
        return Err("群系查询失败\n坐标超出可生成范围".to_string());
    }
    Ok(SeedMapBlock {
        biome: ffi::biome_name(mc, id),
    })
}

#[cfg(test)]
pub fn biome_at_block(
    version: &str,
    seed: i64,
    x: i32,
    y: i32,
    z: i32,
) -> Result<SeedMapBlock, String> {
    let mc = ffi::mc_from_name(version).ok_or_else(|| format!("版本不受支持\n{version}"))?;
    let gen = ffi::Generator::new(version, seed)?;
    let id = gen.biome_at(1, x, y, z);
    if id < 0 {
        return Err("群系查询失败\n坐标超出可生成范围".to_string());
    }
    Ok(SeedMapBlock {
        biome: ffi::biome_name(mc, id),
    })
}

pub fn collect_structures(
    version: &str,
    seed: i64,
    rx: i32,
    rz: i32,
) -> Result<Vec<SeedMapStructure>, String> {
    let mc = ffi::mc_from_name(version).ok_or_else(|| format!("版本不受支持\n{version}"))?;
    let gen = ffi::Generator::new(version, seed)?;
    let mut out: Vec<SeedMapStructure> = gen
        .structures(mc, seed, rx, rz)
        .into_iter()
        .filter_map(|s| {
            structure_key(s.kind).map(|kind| SeedMapStructure {
                kind: kind.to_string(),
                x: s.x,
                z: s.z,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.x, a.z, a.kind.as_str()).cmp(&(b.x, b.z, b.kind.as_str())));
    Ok(out)
}

const STRUCTURE_TYPES: &[(&str, i32)] = &[
    ("desert_pyramid", 1),
    ("jungle_temple", 2),
    ("swamp_hut", 3),
    ("igloo", 4),
    ("village", 5),
    ("ocean_ruin", 6),
    ("shipwreck", 7),
    ("monument", 8),
    ("mansion", 9),
    ("outpost", 10),
    ("ruined_portal", 11),
    ("ancient_city", 13),
    ("treasure", 14),
    ("trail_ruins", 24),
    ("trial_chambers", 25),
    ("abandoned_camp", 26),
];

pub fn structure_type(kind: &str) -> Option<i32> {
    STRUCTURE_TYPES
        .iter()
        .find(|(key, _)| *key == kind)
        .map(|(_, id)| *id)
}

pub fn structure_key(kind: i32) -> Option<&'static str> {
    STRUCTURE_TYPES
        .iter()
        .find(|(_, id)| *id == kind)
        .map(|(key, _)| *key)
}

pub fn find_structures(
    version: &str,
    seed: i64,
    kind: &str,
    center_x: i32,
    center_z: i32,
    radius: i32,
) -> Result<Vec<SeedMapStructure>, String> {
    let mc = ffi::mc_from_name(version).ok_or_else(|| format!("版本不受支持\n{version}"))?;
    let kind_id = structure_type(kind).ok_or_else(|| format!("未知结构\n{kind}"))?;
    let gen = ffi::Generator::new(version, seed)?;
    let raw = gen.find_structures(ffi::StructureSearch {
        mc,
        seed,
        kind: kind_id,
        center_x,
        center_z,
        radius,
        cap: 32,
    });
    Ok(raw
        .into_iter()
        .filter_map(|s| {
            structure_key(s.kind).map(|kind| SeedMapStructure {
                kind: kind.to_string(),
                x: s.x,
                z: s.z,
            })
        })
        .collect())
}

pub fn find_slime_chunks(
    version: &str,
    seed: i64,
    center_x: i32,
    center_z: i32,
    radius: i32,
) -> Result<Vec<SeedMapSlimeChunk>, String> {
    if ffi::mc_from_name(version).is_none() {
        return Err(format!("版本不受支持\n{version}"));
    }
    Ok(find_slime_chunks_seed(seed, center_x, center_z, radius))
}

/// 史莱姆区块与 MC 版本无关；世界预览按存档种子直接调用。
pub fn find_slime_chunks_seed(
    seed: i64,
    center_x: i32,
    center_z: i32,
    radius: i32,
) -> Vec<SeedMapSlimeChunk> {
    ffi::find_slime_chunks(seed, center_x, center_z, radius, 32)
        .into_iter()
        .map(|c| SeedMapSlimeChunk { x: c.x, z: c.z })
        .collect()
}

pub fn slime_chunks(
    version: &str,
    seed: i64,
    rx: i32,
    rz: i32,
) -> Result<Vec<SeedMapSlimeChunk>, String> {
    if ffi::mc_from_name(version).is_none() {
        return Err(format!("版本不受支持\n{version}"));
    }
    Ok(slime_chunks_seed(seed, rx, rz))
}

/// 单个 region 的史莱姆区块（区块坐标）；与版本无关。
pub fn slime_chunks_seed(seed: i64, rx: i32, rz: i32) -> Vec<SeedMapSlimeChunk> {
    ffi::collect_slime_chunks(seed, rx, rz)
        .into_iter()
        .map(|c| SeedMapSlimeChunk { x: c.x, z: c.z })
        .collect()
}

fn encode_png(buf: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, TILE_PX as u32, TILE_PX as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("瓦片编码失败\n{e}"))?;
        writer
            .write_image_data(buf)
            .map_err(|e| format!("瓦片写入失败\n{e}"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_truth_biomes() {
        let plains = biome_at_block("26.3", 7354360020768317315, 1740, 64, 684).unwrap();
        assert_eq!(plains.biome, "plains");
        let dappled = biome_at_block("26.3", -3829811542736183482, 68148, 77, 80990).unwrap();
        assert_eq!(dappled.biome, "dappled_forest");
        // 俯视采样：最大 Y 应与真实地表群系一致。
        let top = biome_at("26.3", 7354360020768317315, 1740, 684).unwrap();
        assert_eq!(top.biome, "plains");
        let top2 = biome_at("26.3", -3829811542736183482, 68148, 80990).unwrap();
        assert_eq!(top2.biome, "dappled_forest");
    }

    #[test]
    fn tile_is_png_512() {
        let png = render_tile("26.3", 7354360020768317315, 0, 0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(&png));
        let reader = decoder.read_info().unwrap();
        assert_eq!(reader.info().width, 512);
        assert_eq!(reader.info().height, 512);
    }

    #[test]
    fn tile_renders_old_families() {
        for version in ["1.20.6", "1.12.2", "1.7.10", "1.0", "Beta 1.7"] {
            let png = render_tile(version, 12345, 0, 0).unwrap();
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "{version} 瓦片编码失败");
        }
    }

    #[test]
    fn structures_are_known_overworld_types() {
        const KNOWN: &[&str] = &[
            "desert_pyramid",
            "jungle_temple",
            "swamp_hut",
            "igloo",
            "village",
            "ocean_ruin",
            "shipwreck",
            "monument",
            "mansion",
            "outpost",
            "ruined_portal",
            "ancient_city",
            "treasure",
            "trail_ruins",
            "trial_chambers",
            "abandoned_camp",
        ];
        let list = collect_structures("26.3", 7354360020768317315, 0, 0).unwrap();
        assert!(!list.is_empty(), "region 0,0 应至少有一个结构候选");
        assert!(
            list.iter().all(|s| KNOWN.contains(&s.kind.as_str())),
            "结构类型必须来自已知列表"
        );
    }

    #[test]
    fn structures_smoke_old_families() {
        for version in ["1.20.6", "1.12.2", "1.7.10"] {
            let list = collect_structures(version, 12345, 0, 0).unwrap();
            assert!(
                list.iter().all(|s| !s.kind.is_empty()),
                "{version} 结构类型不能为空"
            );
        }
    }

    #[test]
    fn finds_nearest_village() {
        let list = find_structures("26.3", 7354360020768317315, "village", 0, 0, 10_000).unwrap();
        assert!(!list.is_empty(), "10k 内应找到村庄");
        let d = |s: &SeedMapStructure| (s.x as i64).pow(2) + (s.z as i64).pow(2);
        assert!(
            list.windows(2).all(|w| d(&w[0]) <= d(&w[1])),
            "结构结果应按距离升序"
        );
    }

    #[test]
    fn finds_slime_chunks() {
        let tile = slime_chunks("26.3", 7354360020768317315, 0, 0).unwrap();
        assert!(!tile.is_empty(), "region (0,0) 应含史莱姆区块");
        let nearest = find_slime_chunks("26.3", 7354360020768317315, 0, 0, 2000).unwrap();
        assert!(!nearest.is_empty(), "2k 内应找到史莱姆区块");
        assert!(nearest.iter().all(|c| c.x >= -125 && c.x <= 125));
    }
}
