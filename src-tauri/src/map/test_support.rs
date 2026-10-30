//! 测试专用：程序化构造合法 region/chunk，不依赖真实存档。
//! 由 `mca::RegionWriter` 写标准 8KiB header + zlib chunk，供 render / search 测试复用。

use fastnbt::{nbt, LongArray, Value};
use mca::{Compression, RegionWriter};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 1.16+ 打包位格式：每 i64 按 64/bits 均分（高位在前，无跨 long）。
pub(crate) fn pack_states(states: &[u16], palette_len: usize) -> Vec<i64> {
    let bits = if palette_len < 16 {
        4
    } else {
        (palette_len as f64).log2().ceil() as usize
    };
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
pub(crate) fn synthetic_chunk_nbt() -> Vec<u8> {
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
        let biomes = Value::Compound(HashMap::from([(
            "palette".to_string(),
            Value::List(vec![Value::String("minecraft:plains".into())]),
        )]));
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
pub(crate) fn synthetic_region_mca() -> Vec<u8> {
    let nbt = synthetic_chunk_nbt();
    let mut writer = RegionWriter::new();
    writer.set_chunk(0, 0, nbt, Compression::default()).unwrap();
    let mut buf = Vec::new();
    writer.write(&mut buf).unwrap();
    buf
}

/// 在 world 下写 region/r.0.0.mca，返回文件路径。
pub(crate) fn write_synthetic_region(world: &Path) -> PathBuf {
    let region = world.join("region");
    std::fs::create_dir_all(&region).unwrap();
    let path = region.join("r.0.0.mca");
    std::fs::write(&path, synthetic_region_mca()).unwrap();
    path
}
