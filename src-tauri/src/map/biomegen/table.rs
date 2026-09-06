//! 26.1.2 主世界生物群系参数表（官方 datagen `biome_parameters/minecraft/overworld.json`
//! 一次性转二进制内嵌；生成脚本 `tools/gen_biome_table.mjs`，可再生）。
//!
//! 行布局：u16 biome_id + 6×[lo,hi] i32（温度/湿度/大陆度/侵蚀/深度/weirdness，×10000 域）。
//! 搜索距离与 vanilla RTree 同口径：逐维出界量平方和（int 域）。

use std::sync::OnceLock;

const TABLE_BIN: &[u8] = include_bytes!("overworld_26.bin");

pub struct BiomeTable {
    pub names: Vec<&'static str>,
    rows: Vec<Row>,
}

struct Row {
    biome: u16,
    lo: [i64; 6],
    hi: [i64; 6],
}

static TABLE: OnceLock<BiomeTable> = OnceLock::new();

fn decode() -> BiomeTable {
    let mut off = 4usize;
    assert_eq!(&TABLE_BIN[0..4], b"EBM1", "表头损坏");
    let name_count = u32::from_le_bytes(TABLE_BIN[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let mut names = Vec::with_capacity(name_count);
    for _ in 0..name_count {
        let len = u16::from_le_bytes(TABLE_BIN[off..off + 2].try_into().unwrap()) as usize;
        off += 2;
        let s = std::str::from_utf8(&TABLE_BIN[off..off + len]).expect("名称非 UTF-8");
        // 表为 include_bytes 常量，静态生命周期安全
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        names.push(leaked);
        off += len;
    }
    let mut rows = Vec::with_capacity((TABLE_BIN.len() - off) / 50);
    while off + 50 <= TABLE_BIN.len() {
        let biome = u16::from_le_bytes(TABLE_BIN[off..off + 2].try_into().unwrap());
        off += 2;
        let mut lo = [0i64; 6];
        let mut hi = [0i64; 6];
        for dim in 0..6 {
            lo[dim] = i32::from_le_bytes(TABLE_BIN[off..off + 4].try_into().unwrap()) as i64;
            hi[dim] = i32::from_le_bytes(TABLE_BIN[off + 4..off + 8].try_into().unwrap()) as i64;
            off += 8;
        }
        rows.push(Row { biome, lo, hi });
    }
    BiomeTable { names, rows }
}

/// 全局 26.1.2 主世界表。
pub fn overworld_26() -> &'static BiomeTable {
    TABLE.get_or_init(decode)
}

impl BiomeTable {
    /// 行数（对拍/文档用）。
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// 最近点搜索。target 六维浮点（t,h,c,e,d,w）。
    /// 量化与 vanilla 一致：d2f(float32) → ×10000f → trunc（Climate.quantizeCoord）。
    /// 单趟无分配（热路径：每瓦片调用 16384 次）。
    pub fn search(&self, target: &[f64; 6]) -> Option<u16> {
        let np = [
            ((target[0] as f32) * 10000.0f32) as i64,
            ((target[1] as f32) * 10000.0f32) as i64,
            ((target[2] as f32) * 10000.0f32) as i64,
            ((target[3] as f32) * 10000.0f32) as i64,
            ((target[4] as f32) * 10000.0f32) as i64,
            ((target[5] as f32) * 10000.0f32) as i64,
        ];
        let mut best: Option<(u128, u16)> = None;
        for row in &self.rows {
            let mut ds: u128 = 0;
            for (dim, v) in np.iter().copied().enumerate() {
                let a = v - row.hi[dim];
                let b = row.lo[dim] - v;
                let d = if a > 0 {
                    a
                } else if b > 0 {
                    b
                } else {
                    0
                };
                ds += (d * d) as u128;
            }
            match best {
                Some((bd, _)) if bd <= ds => {}
                _ => best = Some((ds, row.biome)),
            }
        }
        best.map(|(_, id)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_decodes() {
        let t = overworld_26();
        assert_eq!(t.names.len(), 54);
        assert_eq!(t.row_count(), 7593);
        assert!(t.names.contains(&"minecraft:pale_garden"));
    }

    #[test]
    fn search_known_point() {
        let t = overworld_26();
        // 任意靶点必须落在表内合法群系（真实语义校准由对拍测试覆盖）
        let id = t
            .search(&[1.2, -1.0, 0.8, 0.0, 0.0, 0.2])
            .expect("表非空必有最近");
        assert!((id as usize) < t.names.len());
        assert_eq!(
            t.search(&[1.2, -1.0, 0.8, 0.0, 0.0, 0.2]),
            Some(id),
            "确定性"
        );
    }
}
