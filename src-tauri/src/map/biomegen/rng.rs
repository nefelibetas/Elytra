//! 生物群系预测引擎：随机源与哈希原语。
//!
//! 全部语义按 26.1.2 官方 server 字节码逐一核对：
//! - `Xoroshiro128PlusPlus`：MC `net.minecraft.world.level.levelgen.Xoroshiro128PlusPlus`
//! - `nextInt(bound)`：MC `XoroshiroRandomSource.nextInt(int)`（32 位乘法高位 + 低 32 位拒绝采样）
//! - `nextDouble`：`nextLong() >>> 11 * 2^-53`
//! - 种子升级 `upgradeSeedTo128bit`：splitmix 两次（`RandomSupport`）
//! - `md5_utf8`：guava `Hashing.md5()`，lo = 字节 0..7 大端，hi = 8..15 大端（`RandomSupport.seedFromHashOf`）

/// Xoroshiro128PlusPlus（lo/hi 直接即状态，无额外混合——仅 `seed(long)` 走 splitmix 升级）。
#[derive(Clone)]
pub struct Xoroshiro {
    lo: u64,
    hi: u64,
}

const XL: u64 = 0x9e37_79b9_7f4a_7c15;
const XH: u64 = 0x6a09_e667_f3bc_c909;
const A_MIX: u64 = 0xbf58_476d_1ce4_e5b9;
const B_MIX: u64 = 0x94d0_49bb_1331_11eb;

impl Xoroshiro {
    #[inline]
    pub fn new(lo: u64, hi: u64) -> Self {
        Self { lo, hi }
    }

    /// `XoroshiroRandomSource(long)`：`upgradeSeedTo128bit`（splitmix×2）后直接作状态。
    pub fn seed(seed: u64) -> Self {
        let mut l = seed ^ XH;
        let mut h = l.wrapping_add(XL);
        l = mix_stafford(l);
        h = mix_stafford(h);
        Self { lo: l, hi: h }
    }

    #[inline]
    pub fn next_long(&mut self) -> u64 {
        let l = self.lo;
        let h = self.hi;
        let n = (l.wrapping_add(h)).rotate_left(17).wrapping_add(l);
        let h2 = h ^ l;
        self.lo = l.rotate_left(49) ^ h2 ^ (h2 << 21);
        self.hi = h2.rotate_left(28);
        n
    }

    #[inline]
    fn next_int32(&mut self) -> u32 {
        (self.next_long() & 0xffff_ffff) as u32
    }

    /// `XoroshiroRandomSource.nextInt(bound)`（bound>0）。
    pub fn next_int(&mut self, bound: u32) -> u32 {
        let mut r = (self.next_int32() as u64).wrapping_mul(bound as u64);
        let mut low = (r & 0xffff_ffff) as u32;
        if low < bound {
            let t = (bound.wrapping_neg()).wrapping_rem(bound); // Integer.remainderUnsigned(-bound, bound)
            while low < t {
                r = (self.next_int32() as u64).wrapping_mul(bound as u64);
                low = (r & 0xffff_ffff) as u32;
            }
        }
        (r >> 32) as u32
    }

    #[inline]
    pub fn next_double(&mut self) -> f64 {
        (self.next_long() >> 11) as f64 * 1.110_223_024_625_156_5e-16
    }
}

#[inline]
fn mix_stafford(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(A_MIX);
    z = (z ^ (z >> 27)).wrapping_mul(B_MIX);
    z ^ (z >> 31)
}

/// MD5（RFC 1321），输出 16 字节。
pub fn md5_utf8(s: &str) -> [u8; 16] {
    md5(s.as_bytes())
}

pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6,
        10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76a_a478, 0xe8c7_b756, 0x2420_70db, 0xc1bd_ceee, 0xf57c_0faf, 0x4787_c62a, 0xa830_4613,
        0xfd46_9501, 0x6980_98d8, 0x8b44_f7af, 0xffff_5bb1, 0x895c_d7be, 0x6b90_1122, 0xfd98_7193,
        0xa679_438e, 0x49b4_0821, 0xf61e_2562, 0xc040_b340, 0x265e_5a51, 0xe9b6_c7aa, 0xd62f_105d,
        0x0244_1453, 0xd8a1_e681, 0xe7d3_fbc8, 0x21e1_cde6, 0xc337_07d6, 0xf4d5_0d87, 0x455a_14ed,
        0xa9e3_e905, 0xfcef_a3f8, 0x676f_02d9, 0x8d2a_4c8a, 0xfffa_3942, 0x8771_f681, 0x6d9d_6122,
        0xfde5_380c, 0xa4be_ea44, 0x4bde_cfa9, 0xf6bb_4b60, 0xbebf_bc70, 0x289b_7ec6, 0xeaa1_27fa,
        0xd4ef_3085, 0x0488_1d05, 0xd9d4_d039, 0xe6db_99e5, 0x1fa2_7cf8, 0xc4ac_5665, 0xf429_2244,
        0x432a_ff97, 0xab94_23a7, 0xfc93_a039, 0x655b_59c3, 0x8f0c_cc92, 0xffef_f47d, 0x8584_5dd1,
        0x6fa8_7e4f, 0xfe2c_e6e0, 0xa301_4314, 0x4e08_11a1, 0xf753_7e82, 0xbd3a_f235, 0x2ad7_d2bb,
        0xeb86_d391,
    ];
    let mut a0: u32 = 0x6745_2301;
    let mut b0: u32 = 0xefcd_ab89;
    let mut c0: u32 = 0x98ba_dcfe;
    let mut d0: u32 = 0x1032_5476;

    let orig_len = data.len();
    let mut ml = (orig_len as u64).wrapping_mul(8);
    // padding
    let mut buf: Vec<u8> = Vec::with_capacity(((orig_len + 8) / 64 + 1) * 64);
    buf.extend_from_slice(data);
    buf.push(0x80);
    while buf.len() % 64 != 56 {
        buf.push(0);
    }
    for _ in 0..8 {
        buf.push((ml & 0xff) as u8);
        ml >>= 8;
    }
    let mut chunks = buf.chunks_exact(64);
    for chunk in &mut chunks {
        let mut m = [0u32; 16];
        for (i, w) in chunk.chunks_exact(4).enumerate() {
            m[i] = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let tmp = d;
            d = c;
            c = b;
            let sum = a
                .wrapping_add(f)
                .wrapping_add(K[i])
                .wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = tmp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = [0u8; 16];
    for (chunk, v) in out.chunks_exact_mut(4).zip([a0, b0, c0, d0]) {
        chunk.copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// md5("...") 128 位拆两半（guava `seedFromHashOf` 布局：lo = 字节 0..7 大端）。
pub fn md5_halves(s: &str) -> (u64, u64) {
    let b = md5_utf8(s);
    let lo = u64::from_be_bytes(b[0..8].try_into().unwrap());
    let hi = u64::from_be_bytes(b[8..16].try_into().unwrap());
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_known_vectors() {
        assert_eq!(md5_utf8(""), hex("d41d8cd98f00b204e9800998ecf8427e"));
        assert_eq!(md5_utf8("abc"), hex("900150983cd24fb0d6963f7d28e17f72"));
        assert_eq!(
            md5_utf8("minecraft:temperature"),
            hex("5c7e6b29735f0d7ff7d86f1bbc734988")
        );
        // 顺位：md5("minecraft:temperature") 前后 8 字节大端拼 = 上面两半
        let (lo, hi) = md5_halves("minecraft:temperature");
        assert_eq!(lo, 0x5c7e_6b29_735f_0d7f);
        assert_eq!(hi, 0xf7d8_6f1b_bc73_4988);
    }

    fn hex(s: &str) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (i, c) in s.as_bytes().chunks(2).enumerate() {
            out[i] = u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap();
        }
        out
    }

    #[test]
    fn xoro_sequence_stable() {
        // 26.1.2 官方 XoroshiroRandomSource 实测锚点（javac/java 对拍）
        let mut r = Xoroshiro::seed(7354360020768317315);
        assert_eq!(r.next_long(), 0x644c_ba39_39ee_076d);
        assert_eq!(r.next_long(), 0x4a77_a3ea_79f2_fdc9);
        assert_eq!(r.next_long(), 0x4ae5_e6c5_6622_ba0b);
        let mut r2 = Xoroshiro::seed(42);
        assert_eq!(r2.next_long(), 0xbed4_a3d4_69c5_d91f);
    }

    #[test]
    fn next_int_bounds() {
        let mut r = Xoroshiro::new(0x1234_5678_9abc_def0, 0x0fed_cba9_8765_4321);
        for _ in 0..1000 {
            let b = 1 + (r.next_long() % 256) as u32;
            assert!(r.next_int(b) < b);
        }
    }
}
