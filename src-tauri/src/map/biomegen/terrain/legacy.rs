//! legacy BlendedNoise（base_3d_noise）移植（26.1.2 字节码直译）。
//!
//! 结构：minLimit/maxLimit = octave -15..0（16 层），main = -7..0（8 层）；
//! 三个 PerlinNoise 共用同一 RandomSource 顺序播种。播种采用 legacy 分支：
//! 先抽 1 个 ImprovedNoise 放最高层（len-1），再自 len-2 向下逐层各抽 1 个。
//!
//! compute 语义（字节码核对）：
//! ```text
//! xz = bx·xzMultiplier; y = by·yMultiplier; z = bz·xzMultiplier
//! x0=xz/xzFactor; y0=y/yFactor; z0=z/xzFactor
//! ys = yMultiplier·smearScaleMultiplier / yFactor
//! main: v=1 逐层 /2；mainSum += main[i].noise(wrap(x0·v),wrap(y0·v),wrap(z0·v),
//!        wrap(ys·v), wrap(y0·v)) / v        （i=0..7，v=2^-i）
//! m = (mainSum/10+1)/2
//! min/max 各自 i=0..15：sum += oct[i].noise(wrap(xz·v),wrap(y·v),wrap(z·v),
//!        wrap(ys·v),wrap(y·v))/v
//! 最终 = clampedLerp(m, minSum/512, maxSum/512)/128
//! （m>=1 只用 max；m<=0 只用 min）
//! ```

use super::super::noise::{wrap, ImprovedNoise};
use super::super::rng::Xoroshiro;

pub struct LegacyOctaves {
    levels: Vec<Option<ImprovedNoise>>,
}

impl LegacyOctaves {
    /// legacy 分支：最高层 1 个 + 其余各 1 个，全部顺序来自同一 rng。
    fn create(rng: &mut Xoroshiro, len: usize) -> Self {
        let mut levels: Vec<Option<ImprovedNoise>> = Vec::with_capacity(len);
        for _ in 0..len {
            levels.push(None);
        }
        if len > 0 {
            levels[len - 1] = Some(ImprovedNoise::new(rng));
            for j in (0..len - 1).rev() {
                levels[j] = Some(ImprovedNoise::new(rng));
            }
        }
        Self { levels }
    }

    #[inline]
    fn oct(&self, i: usize) -> Option<&ImprovedNoise> {
        self.levels[i].as_ref()
    }
}

pub struct BlendedNoise {
    min_limit: LegacyOctaves,
    max_limit: LegacyOctaves,
    main: LegacyOctaves,
    xz_multiplier: f64,
    y_multiplier: f64,
    xz_factor: f64,
    y_factor: f64,
    smear_scale_multiplier: f64,
}

impl BlendedNoise {
    /// 参数顺序同 codec：xzScale, yScale, xzFactor, yFactor, smearScaleMultiplier
    pub fn create(
        rng: &mut Xoroshiro,
        xz_scale: f64,
        y_scale: f64,
        xz_factor: f64,
        y_factor: f64,
        smear_scale_multiplier: f64,
    ) -> Self {
        let min_limit = LegacyOctaves::create(rng, 16); // -15..0
        let max_limit = LegacyOctaves::create(rng, 16); // -15..0
        let main = LegacyOctaves::create(rng, 8); // -7..0
        Self {
            min_limit,
            max_limit,
            main,
            xz_multiplier: xz_scale,
            y_multiplier: y_scale,
            xz_factor,
            y_factor,
            smear_scale_multiplier,
        }
    }

    pub fn compute(&self, bx: f64, by: f64, bz: f64) -> f64 {
        let xz = bx * self.xz_multiplier;
        let y = by * self.y_multiplier;
        let z = bz * self.xz_multiplier;
        let x0 = xz / self.xz_factor;
        let y0 = y / self.y_factor;
        let z0 = z / self.xz_factor;
        let ys = self.y_multiplier * self.smear_scale_multiplier / self.y_factor;

        // main：i=0..7
        let mut main_sum = 0.0;
        let mut v = 1.0;
        for i in 0..8usize {
            if let Some(oct) = self.main.oct(i) {
                main_sum +=
                    oct.noise5(wrap(x0 * v), wrap(y0 * v), wrap(z0 * v), wrap(ys * v), wrap(y0 * v))
                        / v;
            }
            v /= 2.0;
        }
        let m = (main_sum / 10.0 + 1.0) / 2.0;
        let m_int = m as i32;
        let use_min = m_int < 1;
        let use_max = m_int > 0;

        let mut min_sum = 0.0;
        let mut max_sum = 0.0;
        let mut v2 = 1.0;
        for i in 0..16usize {
            if use_min {
                if let Some(oct) = self.min_limit.oct(i) {
                    min_sum += oct.noise5(
                        wrap(xz * v2),
                        wrap(y * v2),
                        wrap(z * v2),
                        wrap(ys * v2),
                        wrap(y * v2),
                    ) / v2;
                }
            }
            if use_max {
                if let Some(oct) = self.max_limit.oct(i) {
                    max_sum += oct.noise5(
                        wrap(xz * v2),
                        wrap(y * v2),
                        wrap(z * v2),
                        wrap(ys * v2),
                        wrap(y * v2),
                    ) / v2;
                }
            }
            v2 /= 2.0;
        }
        clamped_lerp(m, min_sum / 512.0, max_sum / 512.0) / 128.0
    }
}

#[inline]
pub fn clamped_lerp(part: f64, from: f64, to: f64) -> f64 {
    if part < 0.0 {
        from
    } else if part > 1.0 {
        to
    } else {
        from + part * (to - from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_finite() {
        let mut r = Xoroshiro::seed(7354360020768317315);
        let n = BlendedNoise::create(&mut r, 0.25, 0.125, 80.0, 160.0, 8.0);
        let mut r2 = Xoroshiro::seed(7354360020768317315);
        let n2 = BlendedNoise::create(&mut r2, 0.25, 0.125, 80.0, 160.0, 8.0);
        assert_eq!(n.compute(0.0, 0.0, 0.0), n2.compute(0.0, 0.0, 0.0));
        assert!(n.compute(123.0, 64.0, -45.0).is_finite());
        assert!(n.compute(-5000.0, 120.0, 3000.0).abs() < 10.0);
    }

    #[test]
    fn clamp_lerp_edges() {
        assert_eq!(clamped_lerp(-1.0, 5.0, 9.0), 5.0);
        assert_eq!(clamped_lerp(2.0, 5.0, 9.0), 9.0);
        assert_eq!(clamped_lerp(0.5, 4.0, 8.0), 6.0);
    }
}
