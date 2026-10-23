//! chunk 坐标换算纯函数：region 索引 ↔ chunk 坐标 ↔ 世界方块坐标。
//! 无状态、无 IO，全部可单测。

/// chunk 在 32×32 region 网格内的定位与换算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    /// region 内线性索引（0..1024）→ region 内 (x,z)（0..32）。
    /// 约定与 mca/官方一致：i = x + 32z。
    pub fn from_index(i: u16) -> Self {
        Self {
            x: (i % 32) as i32,
            z: (i / 32) as i32,
        }
    }

    /// 该 chunk 在所属 region 内的 (x,z)（0..32）。
    pub fn coords_in_region(self) -> (u8, u8) {
        (self.x.rem_euclid(32) as u8, self.z.rem_euclid(32) as u8)
    }

    /// 所属 region 坐标（floored division）。
    #[cfg(test)]
    pub fn region(self) -> (i32, i32) {
        (self.x.div_euclid(32), self.z.div_euclid(32))
    }

    /// 世界方块坐标（chunk 原点）。
    #[cfg(test)]
    pub fn block_origin(self) -> (i32, i32) {
        (self.x * 16, self.z * 16)
    }
}

/// 方块坐标 → chunk 坐标（floored division，负坐标正确）。
#[cfg(test)]
pub fn block_to_chunk(b: i32) -> i32 {
    b.div_euclid(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_roundtrip_matches_official_layout() {
        // 官方布局：i = x + 32z（minecraft.wiki Region file format）
        assert_eq!(ChunkPos::from_index(0).coords_in_region(), (0, 0));
        assert_eq!(ChunkPos::from_index(1).coords_in_region(), (1, 0));
        assert_eq!(ChunkPos::from_index(32).coords_in_region(), (0, 1));
        assert_eq!(ChunkPos::from_index(1023).coords_in_region(), (31, 31));
    }

    #[test]
    fn negative_chunks_floored() {
        // chunk (-1,-1) 在 region (-1,-1) 的 (31,31)
        let p = ChunkPos { x: -1, z: -1 };
        assert_eq!(p.region(), (-1, -1));
        assert_eq!(p.coords_in_region(), (31, 31));
        // chunk (31, 31) 在 region (0,0)；chunk (32, 0) 在 region (1, 0)
        assert_eq!(ChunkPos { x: 32, z: 0 }.region(), (1, 0));
        assert_eq!(ChunkPos { x: -32, z: 0 }.region(), (-1, 0));
    }

    #[test]
    fn block_origin_and_inverse() {
        let p = ChunkPos { x: -3, z: 12 };
        assert_eq!(p.block_origin(), (-48, 192));
        assert_eq!(block_to_chunk(-1), -1, "方块 -1 属于 chunk -1（非 0）");
        assert_eq!(block_to_chunk(-16), -1);
        assert_eq!(block_to_chunk(-17), -2);
        assert_eq!(block_to_chunk(15), 0);
    }
}
