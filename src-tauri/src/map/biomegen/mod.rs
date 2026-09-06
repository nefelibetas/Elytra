//! 生物群系预测引擎（未加载区预览的"底图"层）。
//!
//! 输入：世界种子（来自 `data/minecraft/world_gen_settings.dat`，`nbt_seed` 解析）。
//! 输出：任意坐标的 vanilla 主世界生物群系预测（26.x/1.20.1+ 族；1.18 前不支持）。
//!
//! 语义逐项对照 26.1.2 官方 server 字节码 + 官方 datagen 参数表：
//! 噪声播种链 / ImprovedNoise / PerlinNoise(md5 octave) / NormalNoise(337:331 双叠) /
//! ShiftA/B / shifted_noise(xz_scale=0.25) / depth = y 梯度 + offset 样条
//! （样条树内嵌 `offset_26.json`，非混合区 blend_alpha=1/blend_offset=0）。
//!
//! mod 影响边界：本模块只负责"vanilla 采样器在给定参数表下的输出"；
//! 数据型 mod（换表/换噪声参数）后续经"以世界档案声明为准"注入；引擎替换型不支持。

pub mod colors;
pub mod terrain;
pub mod png_tile;
pub mod world;
mod engine;
mod noise;
mod rng;
mod table;

pub use engine::ClimateEngine;
pub use table::{overworld_26, BiomeTable};

/// 完整预测器：一个种子一套引擎实例。
pub struct BiomePredictor {
    engine: ClimateEngine,
}

impl BiomePredictor {
    /// vanilla 主世界（非 large_biomes）预测器。
    pub fn new(seed: i64) -> Self {
        Self { engine: ClimateEngine::new(seed, false) }
    }

    /// large_biomes 档案用构造。
    pub fn with_large_biomes(seed: i64) -> Self {
        Self { engine: ClimateEngine::new(seed, true) }
    }

    /// 单元（quart）预测：qy 为 quart y，块坐标 = q*4；深度按该单元基块 y。
    pub fn predict_cell(&self, qx: i32, qz: i32, qy: i32) -> u16 {
        let target = self.target_cell(qx, qz, qy);
        overworld_26()
            .search(&target)
            .unwrap_or(u16::MAX)
    }

    /// 给定表层块 y（例如悬停/真实地表高度）预测该列表层群系。
    pub fn predict_at_top(&self, block_x: i32, block_z: i32, top_y: i32) -> u16 {
        let qx = block_x.div_euclid(4);
        let qz = block_z.div_euclid(4);
        let qy = top_y.div_euclid(4);
        self.predict_cell(qx, qz, qy)
    }

    /// 六维 target（块坐标必须落在 4 网格基点上；否则内部自动取单元原点）。
    pub fn target_cell(&self, qx: i32, qz: i32, qy: i32) -> [f64; 6] {
        let x = (qx * 4) as f64;
        let z = (qz * 4) as f64;
        let y = (qy * 4) as f64;
        self.engine.sample6(x, z, y)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predictor_runs_known_seed() {
        let p = BiomePredictor::new(7354360020768317315);
        let t = p.target_cell(0, 0, 15);
        assert!(t.iter().all(|v| v.is_finite()));
        let id = p.predict_cell(0, 0, 15);
        assert!(id != u16::MAX);
        let name = overworld_26().names[id as usize];
        // 真实世界 (0,0) 附近为 forest 族（对拍实证）
        assert!(name.contains("forest") || name.contains("plains") || name.contains("river"));
    }

    #[test]
    fn numeric_anchor_26_seed() {
        // 对拍期钉死的数值锚点：任何重构改变采样/量化语义都会在此失败。
        let p = BiomePredictor::new(7354360020768317315);
        let t = p.target_cell(0, 0, 15); // 块 (0,60,0)
        for (got, want) in t.iter().zip([-0.09364, 0.09766, 0.04906, 0.69052, 0.10051, 0.57359]) {
            assert!((got - want).abs() < 2e-4, "数值锚点漂移: got {got}, want {want}");
        }
        let id = p.predict_cell(0, 0, 15);
        assert_eq!(overworld_26().names[id as usize], "minecraft:forest");
    }
}
