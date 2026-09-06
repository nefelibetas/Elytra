//! 噪声银行：41 个世界生成噪声实例 + legacy BlendedNoise。
//! 播种链与气候引擎一致：world seed → upgrade → fork 对；每噪声
//! `md5("minecraft:<key>")` xor 根对，随后 NormalNoise 顺序构建。
//! BlendedNoise 随机源 = 根对 xor md5("minecraft:terrain")。

use std::collections::HashMap;
use std::sync::Arc;

use super::legacy::BlendedNoise;
use super::super::noise::NormalNoise;
use super::super::rng::{md5_halves, Xoroshiro};

pub struct NoiseBank {
    normal: HashMap<String, Arc<NormalNoise>>,
    pub blended: BlendedNoise,
}

impl NoiseBank {
    pub fn new(seed: i64, specs: &serde_json::Map<String, serde_json::Value>) -> Self {
        let mut root = Xoroshiro::seed(seed as u64);
        let root_lo = root.next_long();
        let root_hi = root.next_long();

        let mut normal = HashMap::with_capacity(specs.len());
        for (key, spec) in specs {
            let first = spec
                .get("firstOctave")
                .and_then(|v| v.as_i64())
                .unwrap_or(-10) as i32;
            let amps: Vec<f64> = spec
                .get("amplitudes")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                .unwrap_or_default();
            let full_key = format!("minecraft:{key}");
            let (mlo, mhi) = md5_halves(&full_key);
            let mut rng = Xoroshiro::new(mlo ^ root_lo, mhi ^ root_hi);
            let n = NormalNoise::create(&mut rng, first, amps);
            normal.insert(full_key, Arc::new(n));
        }

        let (tlo, thi) = md5_halves("minecraft:terrain");
        let mut terrain_rng = Xoroshiro::new(tlo ^ root_lo, thi ^ root_hi);
        let blended = BlendedNoise::create(&mut terrain_rng, 0.25, 0.125, 80.0, 160.0, 8.0);

        Self { normal, blended }
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&Arc<NormalNoise>> {
        self.normal.get(key)
    }
}
