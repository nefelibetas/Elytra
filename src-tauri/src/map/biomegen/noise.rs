//! 噪声原语移植：语义逐条对照 26.1.2 官方字节码。
//!
//! - `ImprovedNoise`：`xo/yo/zo = nextDouble*256`，置换表 256 次 `nextInt(256-i)` 交换；
//!   采样 = 8 角 `gradDot(perm…)`，全部索引 `& 255`；`wrap()` 按 2^25 折返。
//! - `PerlinNoise`（世界生成分支）：`forkPositional()` 取两个 long 作 octave 工厂，
//!   每个非零倍频 ImprovedNoise 的种子 = `md5("octave_<first+i>") XOR 工厂对`；
//!   频率基 `2^firstOctave`（每倍频 ×2），幅值基 `2^(len-1)/(2^len-1)`（每倍频 ÷2）。
//! - `NormalNoise` = 两份 `PerlinNoise`（后者坐标 ×337/331）× `valueFactor`，
//!   `valueFactor = (1/6)/expectedDeviation(d)`，`expectedDeviation(d)=0.1(1+1/(d+1))`。

use super::rng::{md5_halves, Xoroshiro};

/// `PerlinNoise.getValue` 的坐标折返：x - floor(x/2^25 + 0.5)·2^25（round 语义）。
#[inline]
pub(crate) fn wrap(x: f64) -> f64 {
    let k = (x / 33_554_432.0 + 0.5).floor();
    x - k * 33_554_432.0
}

/// 与 MC 一致的三线性缓动。
#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(part: f64, from: f64, to: f64) -> f64 {
    from + part * (to - from)
}

/// `SimplexNoise.gradDot` 的 16 向梯度表（索引 & 15）。
#[inline]
fn grad_dot(idx: usize, x: f64, y: f64, z: f64) -> f64 {
    const GX: [f64; 16] = [
        1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0,
    ];
    const GY: [f64; 16] = [
        1.0, 1.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0,
    ];
    const GZ: [f64; 16] = [
        0.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 0.0, 1.0, 0.0, -1.0,
    ];
    let i = idx & 15;
    GX[i] * x + GY[i] * y + GZ[i] * z
}

/// `ImprovedNoise`：256 置换 + 3 个 [0,256) 偏移。
pub struct ImprovedNoise {
    pub xo: f64,
    pub yo: f64,
    pub zo: f64,
    perm: [u8; 257],
}

impl ImprovedNoise {
    /// `ImprovedNoise(RandomSource)`：3×nextDouble*256 + 256×nextInt(256-i) 交换。
    pub fn new(rng: &mut Xoroshiro) -> Self {
        let xo = rng.next_double() * 256.0;
        let yo = rng.next_double() * 256.0;
        let zo = rng.next_double() * 256.0;
        let mut perm = [0u8; 257];
        for (i, p) in perm.iter_mut().enumerate().take(256) {
            *p = i as u8;
        }
        for i in 0..256usize {
            let j = rng.next_int((256 - i) as u32) as usize + i;
            perm.swap(i, j);
        }
        perm[256] = perm[0];
        Self { xo, yo, zo, perm }
    }

    #[inline]
    fn p(&self, i: i64) -> usize {
        self.perm[(i & 255) as usize] as usize
    }

    /// 等价 `noise(x,y,z)`（无竖直缩放）。
    #[inline]
    pub fn noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let dx = x + self.xo;
        let dy = y + self.yo;
        let dz = z + self.zo;
        let xi = dx.floor();
        let yi = dy.floor();
        let zi = dz.floor();
        let fx = dx - xi;
        let fy = dy - yi;
        let fz = dz - zi;
        self.sample_corners(xi as i64, yi as i64, zi as i64, fx, fy, fz)
    }

    /// MC `ImprovedNoise.noise(x,y,z,yScale,yMin)`：竖直方向按 `yScale` 折叠，
    /// 折叠量 `d26 = floor(d28/yScale + 1e-7)·yScale`（d28 = 在 y 分内的 yMin 或 fy），
    /// 格点行取原 fy 的行，仅分内修正。字节码直译。
    #[inline]
    pub fn noise5(&self, x: f64, y: f64, z: f64, y_scale: f64, y_min: f64) -> f64 {
        let dx = x + self.xo;
        let dy = y + self.yo;
        let dz = z + self.zo;
        let xi = dx.floor();
        let yi = dy.floor();
        let zi = dz.floor();
        let fx = dx - xi;
        let fy = dy - yi;
        let fz = dz - zi;
        let shift = if y_scale == 0.0 {
            0.0
        } else {
            let d28 = if y_min >= 0.0 && y_min < fy { y_min } else { fy };
            (d28 / y_scale + 1.000_000_011_686_097_4e-7).floor() * y_scale
        };
        self.sample_corners(xi as i64, yi as i64, zi as i64, fx, fy - shift, fz)
    }

    /// 8 角梯度插值（共享核）。
    #[inline]
    fn sample_corners(&self, xi: i64, yi: i64, zi: i64, fx: f64, fy: f64, fz: f64) -> f64 {
        let w = fade(fx);
        let u = fade(fy);
        let v = fade(fz);

        let a = self.p(xi) as i64 + yi;
        let aa = self.p(a) as i64 + zi;
        let ab = self.p(a + 1) as i64 + zi;
        let b = self.p(xi + 1) as i64 + yi;
        let ba = self.p(b) as i64 + zi;
        let bb = self.p(b + 1) as i64 + zi;

        let l1 = grad_dot(self.p(aa), fx, fy, fz);
        let l2 = grad_dot(self.p(ba), fx - 1.0, fy, fz);
        let l3 = grad_dot(self.p(ab), fx, fy - 1.0, fz);
        let l4 = grad_dot(self.p(bb), fx - 1.0, fy - 1.0, fz);
        let l5 = grad_dot(self.p(aa + 1), fx, fy, fz - 1.0);
        let l6 = grad_dot(self.p(ba + 1), fx - 1.0, fy, fz - 1.0);
        let l7 = grad_dot(self.p(ab + 1), fx, fy - 1.0, fz - 1.0);
        let l8 = grad_dot(self.p(bb + 1), fx - 1.0, fy - 1.0, fz - 1.0);

        let x1 = lerp(w, l1, l2);
        let x2 = lerp(w, l3, l4);
        let x3 = lerp(w, l5, l6);
        let x4 = lerp(w, l7, l8);
        let y1 = lerp(u, x1, x2);
        let y2 = lerp(u, x3, x4);
        lerp(v, y1, y2)
    }
}

/// 倍频组（对应 MC `PerlinNoise` 世界生成分支）。
pub struct PerlinNoise {
    levels: Vec<Option<ImprovedNoise>>, // 按原始倍频下标存放
    amplitudes: Vec<f64>,
    lowest_freq_input_factor: f64,
    lowest_freq_value_factor: f64,
}

impl PerlinNoise {
    /// `PerlinNoise.create(random, firstOctave, amplitudes)`（positional/md5 分支）。
    pub fn create(rng: &mut Xoroshiro, first_octave: i32, amplitudes: Vec<f64>) -> Self {
        let len = amplitudes.len();
        let factory_lo = rng.next_long();
        let factory_hi = rng.next_long();
        let mut levels: Vec<Option<ImprovedNoise>> = Vec::with_capacity(len);
        for (i, amp) in amplitudes.iter().enumerate() {
            if *amp == 0.0 {
                levels.push(None);
                continue;
            }
            let key = format!("octave_{}", first_octave + i as i32);
            let (lo, hi) = md5_halves(&key);
            let mut oct_rng = Xoroshiro::new(lo ^ factory_lo, hi ^ factory_hi);
            levels.push(Some(ImprovedNoise::new(&mut oct_rng)));
        }
        let nz_min = amplitudes
            .iter()
            .position(|a| *a != 0.0)
            .unwrap_or(0) as i32;
        let nz_max = amplitudes
            .iter()
            .rposition(|a| *a != 0.0)
            .unwrap_or(0) as i32;
        debug_assert!(nz_max >= 0);
        let lowest_freq_input_factor = 2f64.powi(first_octave);
        let lowest_freq_value_factor =
            2f64.powi(len as i32 - 1) / (2f64.powi(len as i32) - 1.0);
        let _ = nz_min;
        let _ = nz_max;
        Self {
            levels,
            amplitudes,
            lowest_freq_input_factor,
            lowest_freq_value_factor,
        }
    }

    /// `getValue(x,y,z)`（气候/`yScale=0` 场景，不做竖直缩放）。
    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut acc = 0.0;
        let mut input = self.lowest_freq_input_factor;
        let mut value = self.lowest_freq_value_factor;
        for (i, level) in self.levels.iter().enumerate() {
            if let Some(noise) = level {
                let v = noise.noise(wrap(x * input), wrap(y * input), wrap(z * input));
                acc += self.amplitudes[i] * v * value;
            }
            input *= 2.0;
            value *= 0.5;
        }
        acc
    }
}

/// `NormalNoise`：双 PerlinNoise（第二份坐标 ×337/331）。
pub struct NormalNoise {
    first: PerlinNoise,
    second: PerlinNoise,
    value_factor: f64,
}

const INPUT_FACTOR: f64 = 337.0 / 331.0;

impl NormalNoise {
    /// `NormalNoise.create(random, NoiseParameters)`（世界生成分支）。
    pub fn create(rng: &mut Xoroshiro, first_octave: i32, amplitudes: Vec<f64>) -> Self {
        let first = PerlinNoise::create(rng, first_octave, amplitudes.clone());
        let second = PerlinNoise::create(rng, first_octave, amplitudes.clone());
        // valueFactor = (1/6) / expectedDeviation(d)；expectedDeviation(d)=0.1*(1+1/(d+1))
        // 等价 (5/3)*(d+1)/(d+2)，其中 d = maxNonZeroIdx - minNonZeroIdx。
        let nz: Vec<usize> = (0..amplitudes.len()).filter(|i| amplitudes[*i] != 0.0).collect();
        let d = nz.last().unwrap_or(&0) - nz.first().unwrap_or(&0);
        let value_factor = 0.1 * (1.0 + 1.0 / (d as f64 + 1.0));
        let value_factor = (1.0 / 6.0) / value_factor;
        Self {
            first,
            second,
            value_factor,
        }
    }

    /// `getValue(x,y,z)`：两份 Perlin 相加（第二份坐标 ×337/331）再乘 valueFactor。
    #[inline]
    pub fn get_value(&self, x: f64, y: f64, z: f64) -> f64 {
        let v = self.first.get_value(x, y, z)
            + self.second.get_value(x * INPUT_FACTOR, y * INPUT_FACTOR, z * INPUT_FACTOR);
        v * self.value_factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn improved_noise_deterministic() {
        let mut r = Xoroshiro::seed(1);
        let n = ImprovedNoise::new(&mut r);
        let v1 = n.noise(0.0, 0.0, 0.0);
        let mut r2 = Xoroshiro::seed(1);
        let n2 = ImprovedNoise::new(&mut r2);
        assert_eq!(v1, n2.noise(0.0, 0.0, 0.0));
        assert!(v1.is_finite());
        assert!((-2.0..2.0).contains(&v1));
    }

    #[test]
    fn normal_noise_stable_seed() {
        // 只验证确定性 + 数值域（幅值归一后输出典型 ±2 内）
        let mut r = Xoroshiro::seed(7354360020768317315);
        let n = NormalNoise::create(&mut r, -10, vec![1.5, 0.0, 1.0, 0.0, 0.0, 0.0]);
        let mut r2 = Xoroshiro::seed(7354360020768317315);
        let n2 = NormalNoise::create(&mut r2, -10, vec![1.5, 0.0, 1.0, 0.0, 0.0, 0.0]);
        assert_eq!(n.get_value(0.0, 0.0, 0.0), n2.get_value(0.0, 0.0, 0.0));
        assert!(n.get_value(0.0, 0.0, 0.0).abs() < 3.0);
    }

    #[test]
    fn octave_seed_salts_known() {
        // cubiomes 表内嵌 md5 常量抽查（顺序: lo,hi of md5('octave_-10')）
        let (lo, hi) = md5_halves("octave_-10");
        assert_eq!(lo, 0x36d3_26ee_d40e_feb2);
        assert_eq!(hi, 0x5be9_ce18_223c_636a);
        let (lo2, hi2) = md5_halves("octave_-9");
        assert_eq!(lo2, 0x082f_e255_f8be_6631);
        assert_eq!(hi2, 0x4e96_119e_22de_dc81);
    }
}
