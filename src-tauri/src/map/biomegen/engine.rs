//! 主世界群系气候采样（26.x = 1.20.1+ 同源语义）。
//!
//! 播种链（与 26.1.2 字节码一致）：
//! ```text
//! world_seed --upgradeSeedTo128bit--> XoroshiroRandomSource
//!   --forkPositional(2×nextLong)--> root factory (lo,hi)
//! 每个噪声 N（registry key 形如 "minecraft:temperature"）：
//!   noiseRng = Xoroshiro(md5(N)lo ^ root.lo, md5(N)hi ^ root.hi)
//!   NormalNoise.create(noiseRng, firstOctave, amplitudes)：
//!     PerlinNoise#1: fork = noiseRng.nextLong×2 → 每倍频 ImprovedNoise 种子 = md5("octave_<k>") ^ fork
//!     PerlinNoise#2: 再 fork（nextLong×2）
//! ```
//! 采样（气候在 1:1 域，`y=0`）：
//! ```text
//! shift_x = offsetNoise(x*0.25, 0, z*0.25) * 4      (ShiftA: getValue(p*0.25)*4)
//! shift_z = offsetNoise(z*0.25, 0, x*0.25) * 4      (ShiftB)
//! v = noise((x + shift? ) ...)
//! ```
//! 26.x 路由里 temperature/vegetation/continents/erosion/ridges 都是
//! `shifted_noise{ xz_scale: 0.25, y_scale: 0, shift_x/z }`：
//! `x' = x*0.25 + shift_x(ctx); y' = 0; z' = z*0.25 + shift_z(ctx)`，随后 NormalNoise(x',0,z')。
//!
//! 深度输入 = `y_clamped_gradient(1.5@-64 → -1.5@320)` + `overworld/offset`；
//! offset（非混合区） = `spline(continents/erosion/ridges_folded) - 0.5037500262260437`。

use super::rng::{md5_halves, Xoroshiro};
use super::noise::NormalNoise;
use serde_json::Value;

pub const SPLINE_BASE: f64 = -0.503_750_026_226_043_7;
const INPUT_SCALE: f64 = 0.25; // shifted_noise xz_scale（26.x 主世界路由）

/// 噪声注册表参数（noise/*.json，26.1.2 官方值；large 变体 separate）。
#[derive(Clone, Copy)]
pub struct NoiseSpec {
    pub key: &'static str,
    pub first_octave: i32,
    pub amplitudes: &'static [f64],
}

pub fn overworld_specs(large: bool) -> [NoiseSpec; 6] {
    // 顺序：温度/湿度/大陆度/侵蚀/ridge(weirdness)/offset
    if large {
        [
            NoiseSpec { key: "minecraft:temperature_large", first_octave: -12, amplitudes: &[1.5, 0.0, 1.0, 0.0, 0.0, 0.0] },
            NoiseSpec { key: "minecraft:vegetation_large", first_octave: -10, amplitudes: &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0] },
            NoiseSpec { key: "minecraft:continentalness_large", first_octave: -11, amplitudes: &[1.0, 1.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0] },
            NoiseSpec { key: "minecraft:erosion_large", first_octave: -11, amplitudes: &[1.0, 1.0, 0.0, 1.0, 1.0] },
            NoiseSpec { key: "minecraft:ridge", first_octave: -7, amplitudes: &[1.0, 2.0, 1.0, 0.0, 0.0, 0.0] },
            NoiseSpec { key: "minecraft:offset", first_octave: -3, amplitudes: &[1.0, 1.0, 1.0, 0.0] },
        ]
    } else {
        [
            NoiseSpec { key: "minecraft:temperature", first_octave: -10, amplitudes: &[1.5, 0.0, 1.0, 0.0, 0.0, 0.0] },
            NoiseSpec { key: "minecraft:vegetation", first_octave: -8, amplitudes: &[1.0, 1.0, 0.0, 0.0, 0.0, 0.0] },
            NoiseSpec { key: "minecraft:continentalness", first_octave: -9, amplitudes: &[1.0, 1.0, 2.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0] },
            NoiseSpec { key: "minecraft:erosion", first_octave: -9, amplitudes: &[1.0, 1.0, 0.0, 1.0, 1.0] },
            NoiseSpec { key: "minecraft:ridge", first_octave: -7, amplitudes: &[1.0, 2.0, 1.0, 0.0, 0.0, 0.0] },
            NoiseSpec { key: "minecraft:offset", first_octave: -3, amplitudes: &[1.0, 1.0, 1.0, 0.0] },
        ]
    }
}

/// 六个气候输入在 26.x 采样里的实现（校准实证：与真实世界对拍 95.8%，
/// shifted_noise xz_scale=0.25 的字节码直译语义）。
pub struct ClimateEngine {
    temp: NormalNoise,
    humidity: NormalNoise,
    continentalness: NormalNoise,
    erosion: NormalNoise,
    weirdness: NormalNoise,
    offset: NormalNoise,
    /// 解析后的 overworld/offset 样条树（非混合区 blend_alpha=1/blend_offset=0）
    spline: Option<Box<SplineNode>>,
}

/// 预解析样条节点：每单元求值零分配（原逐调用 JSON 遍历是性能黑洞）。
#[derive(Clone)]
pub(crate) struct SplineNode {
    pub axis: usize,
    pub loc: Vec<f32>,
    pub der: Vec<f32>,
    pub fixed: Vec<f32>,
    pub child: Vec<Option<Box<SplineNode>>>,
}

impl SplineNode {
    /// 从 JSON spline 子树构建（points 内 value 可为数值或嵌套 spline）。
    pub fn from_json(v: &Value) -> Option<Box<SplineNode>> {
        let o = v.as_object()?;
        let points = o.get("points")?.as_array()?;
        let coord = o
            .get("coordinate")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");
        let axis = match coord {
            "continents" => 0usize,
            "erosion" => 1,
            "ridges_folded" => 2,
            _ => 0,
        };
        let mut loc = Vec::with_capacity(points.len());
        let mut der = Vec::with_capacity(points.len());
        let mut fixed = Vec::with_capacity(points.len());
        let mut child: Vec<Option<Box<SplineNode>>> = Vec::with_capacity(points.len());
        for p in points {
            loc.push(p.get("location").and_then(Value::as_f64).unwrap_or(0.0) as f32);
            der.push(p.get("derivative").and_then(Value::as_f64).unwrap_or(0.0) as f32);
            match p.get("value") {
                Some(Value::Number(num)) => {
                    fixed.push(num.as_f64().unwrap_or(0.0) as f32);
                    child.push(None);
                }
                Some(nested @ Value::Object(_)) => {
                    fixed.push(0.0);
                    child.push(SplineNode::from_json(nested));
                }
                _ => {
                    fixed.push(0.0);
                    child.push(None);
                }
            }
        }
        Some(Box::new(SplineNode {
            axis,
            loc,
            der,
            fixed,
            child,
        }))
    }

    /// 数值求值（f32 域，无分配）：axes = [continentalness, erosion, weirdness_folded]。
    #[inline]
    pub fn eval(&self, axes: &[f64; 3]) -> f64 {
        let f = axes[self.axis.min(2)] as f32;
        let n = self.loc.len();
        if n == 0 {
            return 0.0;
        }
        // 逐点值：固定或递归子样条
        let mut vf = [0.0f32; 12];
        debug_assert!(n <= 12);
        for (i, slot) in vf.iter_mut().take(n).enumerate() {
            *slot = match &self.child[i] {
                Some(ch) => ch.eval(axes) as f32,
                None => self.fixed[i],
            };
        }
        eval_spline_values(&self.loc, &self.der, &vf[..n], f)
    }
}

impl ClimateEngine {
    pub fn new(seed: i64, large: bool) -> Self {
        let mut root = Xoroshiro::seed(seed as u64);
        let root_lo = root.next_long();
        let root_hi = root.next_long();
        let specs = overworld_specs(large);
        let mut noises = specs.iter().map(|s| {
            let (mlo, mhi) = md5_halves(s.key);
            let mut rng = Xoroshiro::new(mlo ^ root_lo, mhi ^ root_hi);
            NormalNoise::create(&mut rng, s.first_octave, s.amplitudes.to_vec())
        });
        let spline_json = include_str!("offset_26.json");
        let spline: Value = serde_json::from_str(spline_json).expect("offset_26.json 内嵌解析失败");
        let spline = extract_spline(&spline).and_then(|s| SplineNode::from_json(&s));
        Self {
            temp: noises.next().unwrap(),
            humidity: noises.next().unwrap(),
            continentalness: noises.next().unwrap(),
            erosion: noises.next().unwrap(),
            weirdness: noises.next().unwrap(),
            offset: noises.next().unwrap(),
            spline,
        }
    }

    /// 单点六输入（t,h,c,e,d,w）：一次移位 + 一次五噪声采样 + 一次深度样条。
    /// y 为块坐标（深度梯度用；气温湿度等 y_scale=0 与 y 无关）。
    #[inline]
    pub fn sample6(&self, bx: f64, bz: f64, y: f64) -> [f64; 6] {
        let sx = self.offset.get_value(bx * INPUT_SCALE, 0.0, bz * INPUT_SCALE) * 4.0;
        let sz = self.offset.get_value(bz * INPUT_SCALE, 0.0, bx * INPUT_SCALE) * 4.0;
        let x = bx * INPUT_SCALE + sx;
        let z = bz * INPUT_SCALE + sz;
        let t = self.temp.get_value(x, 0.0, z);
        let h = self.humidity.get_value(x, 0.0, z);
        let c = self.continentalness.get_value(x, 0.0, z);
        let e = self.erosion.get_value(x, 0.0, z);
        let w = self.weirdness.get_value(x, 0.0, z);
        let wf = -3.0 * ((w.abs() - 2.0 / 3.0).abs() - 1.0 / 3.0);
        let offset = self
            .spline
            .as_ref()
            .map(|s| s.eval(&[c, e, wf]))
            .unwrap_or(0.0)
            + SPLINE_BASE;
        let d = depth_value(y, offset);
        [t, h, c, e, d, w]
    }
}

/// 深度 = y 梯度（1.5@-64 → -1.5@320，越界钳制）+ offset。
#[inline]
pub fn depth_value(y_block: f64, offset: f64) -> f64 {
    let yc = y_block.clamp(-64.0, 320.0);
    let grad = 1.5 - 3.0 * (yc + 64.0) / 384.0;
    grad + offset
}

/// 从 `flat_cache(cache_2d(...))` 中抽出 spline 子树（value 链根部）。
fn extract_spline(v: &Value) -> Option<Value> {
    fn find(v: &Value) -> Option<Value> {
        if let Value::Object(o) = v {
            if o.get("coordinate").is_some() {
                return Some(v.clone());
            }
            for val in o.values() {
                if let Some(s) = find(val) {
                    return Some(s);
                }
            }
        } else if let Value::Array(a) = v {
            for val in a {
                if let Some(s) = find(val) {
                    return Some(s);
                }
            }
        }
        None
    }
    find(v)
}

/// 官方样条数值插值（f32 域，与 cubiomes `getSpline` 一致的舍入语义）。
pub fn eval_spline_values(locs: &[f32], ders: &[f32], vfs: &[f32], f: f32) -> f64 {
    let n = locs.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return vfs[0] as f64;
    }
    let mut i = 0usize;
    while i < n && locs[i] < f {
        i += 1;
    }
    if i == 0 {
        return vfs[0] as f64 + (ders[0] * (f - locs[0])) as f64;
    }
    if i == n {
        return vfs[n - 1] as f64 + (ders[n - 1] * (f - locs[n - 1])) as f64;
    }
    let g = locs[i - 1];
    let h = locs[i];
    let k = (f - g) / (h - g);
    let l = ders[i - 1];
    let m = ders[i];
    let n0 = vfs[i - 1];
    let o = vfs[i];
    let p = l * (h - g) - (o - n0);
    let q = -m * (h - g) + (o - n0);
    let r = lerp_f(k, n0, o) + k * (1.0 - k) * lerp_f(k, p, q);
    r as f64
}

#[inline]
fn lerp_f(part: f32, from: f32, to: f32) -> f32 {
    from + part * (to - from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_sane() {
        let s = overworld_specs(false);
        assert_eq!(s[0].key, "minecraft:temperature");
        assert_eq!(s[0].first_octave, -10);
        assert_eq!(s[4].key, "minecraft:ridge");
        assert_eq!(s[5].key, "minecraft:offset");
        let s2 = overworld_specs(true);
        assert_eq!(s2[0].key, "minecraft:temperature_large");
    }

    #[test]
    fn spline_flat_constant() {
        // 恒 0.044 样条（loc -1.1 起）：f 任何处 ≈0.044
        let json = serde_json::json!({
            "coordinate": "minecraft:overworld/continents",
            "points": [{"derivative": 0.0, "location": -1.1, "value": 0.044}]
        });
        let node = SplineNode::from_json(&json).expect("解析");
        let v = node.eval(&[0.0, 0.0, 0.0]);
        assert!((v - 0.044).abs() < 1e-6);
    }

    #[test]
    fn engine_deterministic() {
        let a = ClimateEngine::new(7354360020768317315, false);
        let b = ClimateEngine::new(7354360020768317315, false);
        let t1 = a.sample6(0.0, 0.0, 60.0);
        let t2 = b.sample6(0.0, 0.0, 60.0);
        assert_eq!(t1, t2);
        assert!(t1[0].abs() < 2.0 && t1[2].abs() < 2.0);
    }
}
