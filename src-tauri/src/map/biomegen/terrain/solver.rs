//! 地形引擎：种子 → 每列表面高度 / 水陆判定（B 路线）。
//!
//! 语义（实档对拍验证）：
//! - 列表面 = 自 y=320 向下首个 final_density ρ>0 的整数 Y（与 1.18+ 一致：
//!   空气以上全 ρ≤0，首个 ρ>0 即地表实心顶——洞穴只在表层之下，向下搜索
//!   不会误读洞顶为表面）。
//! - 水陆 = 表面 + 盖层 3 < 63（真实水面 62~63、盖层 1~5 依地表规则）。
//!   实测（种子 7354360020768317315，真实 full chunk 逐列去植被）：
//!   森林/内陆带 96.9%；全档 12.6k 列 82~85%——北西大沼泽/滩涂带
//!   （dirt 岛 4~10 层 + pond 沟 + 块插值抖动）列级需 surface-rules 模拟。
//!   双线性/三线性晶格插值尝试均不优于盖层模型（63.5%/58.4%），已弃。

use std::sync::{Arc, Mutex, OnceLock};

use super::bank::NoiseBank;
use super::eval::{Ctx, DensityRoot};

pub struct TerrainEngine {
    root: DensityRoot,
}

impl TerrainEngine {
    /// 构建引擎：噪声银行 + final_density 树（引用/噪声键全量校验，
    /// 数据缺陷经 Err 返回，不 panic）。
    pub fn new(seed: i64) -> Result<Self, String> {
        let specs = super::data().noise_specs.as_object().unwrap().clone();
        let bank = Arc::new(NoiseBank::new(seed, &specs));
        let final_json = super::data().noise_settings["noise_router"]["final_density"].clone();
        let root = DensityRoot::build(final_json, bank)?;
        Ok(TerrainEngine { root })
    }

    /// 列表面高度（自顶向下首个 ρ>0；阶梯粗扫 + 段内精扫）。
    pub fn surface_top(&self, x: f64, z: f64) -> i32 {
        let e = |y: f64| self.root.eval_root(&Ctx { x, y, z });
        // 粗扫 16：320 → -64
        let mut y = 320f64;
        while y >= -64.0 && e(y) <= 0.0 {
            y -= 16.0;
        }
        if y < -64.0 {
            return -64;
        }
        let hi = y + 16.0;
        // 精扫 2（段内自顶向下）
        let mut y2 = hi - 2.0;
        while y2 >= y && e(y2) <= 0.0 {
            y2 -= 2.0;
        }
        if y2 < y {
            return y as i32;
        }
        let hi2 = y2 + 2.0;
        // 精扫 1
        let mut y1 = hi2 - 1.0;
        while y1 >= y2 && e(y1) <= 0.0 {
            y1 -= 1.0;
        }
        if y1 < y2 {
            y2 as i32
        } else {
            y1 as i32
        }
    }

    /// 快速水陆分类：None=干地；Some(depth_rank) 水（1 中浅、2 深水）。
    /// 逐级早期退出：陆地绝大多数 1~3 次求值，深海 ~5 次
    /// （ladder 全扫 ~20+ 次，瓦片渲染总成本差 4~5 倍）。
    ///
    /// 判定（与 surface_top+盖层 3 < 63 同阈，此处用密度点门实现）：
    /// 表面顶 ≥60 → 干地；否则水。精度侧重水陆边界 ±1 与
    /// ~0.77ms/eval 的成本折中，见模块注释。
    pub fn water_class(&self, x: f64, z: f64) -> Option<u8> {
        let e = |y: f64| self.root.eval_root(&Ctx { x, y, z });
        // 干地探测：顶 ≥60（含盖层后出水）；补 62/64/68 防海平面带洞穴假阴性
        if e(60.0) > 0.0 || e(62.0) > 0.0 || e(64.0) > 0.0 || e(68.0) > 0.0 {
            return None;
        }
        // 水：定浅深度档
        if e(58.0) > 0.0 {
            // 顶 58/59 → 深 4~5
            return Some(1);
        }
        if e(55.0) > 0.0 {
            // 顶 55~57 → 深 6~8
            return Some(2);
        }
        Some(2)
    }
}

type Slot = Option<(i64, Arc<TerrainEngine>)>;
static TERRAIN_SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();

/// 共享地形引擎（同种子复用；与 shared_predictor 一致的单槽策略）。
/// 构建失败（内嵌数据缺陷）返回中文错误，不缓存失败态。
pub fn shared_terrain(seed: i64) -> Result<Arc<TerrainEngine>, String> {
    let m = TERRAIN_SLOT.get_or_init(|| Mutex::new(None));
    let mut guard = m.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((s, e)) = guard.as_ref() {
        if *s == seed {
            return Ok(e.clone());
        }
    }
    let e = Arc::new(TerrainEngine::new(seed)?);
    *guard = Some((seed, e.clone()));
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 数值锚点（26.1.2 final_density 移植回归）：
    /// 种子 7354360020768317315 实档列 (26,-502) 表面顶 = 60（湿地/湖岸带）。
    /// 语义链任何漂移（播种/噪声/算子/样条/盖层）都会破坏该锚点。
    #[test]
    fn numeric_anchor_surface_top() {
        let eng = TerrainEngine::new(7354360020768317315).expect("引擎构建");
        let top = eng.surface_top(26.0, -502.0);
        assert_eq!(top, 60, "表面高度锚点漂移: {top}");
        // 快分类与盖层一致：顶 60 → 干地（+3 ≥ 63）
        assert!(eng.water_class(26.0, -502.0).is_none());
    }

    /// 快分类与精确阶梯判水一致性冒烟（分类近似质量不回归）。
    #[test]
    fn deep_water_anchor() {
        let eng = TerrainEngine::new(7354360020768317315).expect("引擎构建");
        // 广域采样（±1500 内 4 列间隔）与精确顶+3<63 判水对比
        let mut waters = 0;
        let mut lands = 0;
        let mut mm = 0;
        for k in 0..600 {
            let x = -1500.0 + (k as f64) * 6.0;
            let z = -1200.0 + ((k % 90) as f64) * 7.0;
            let w = eng.water_class(x, z).is_some();
            let ex = eng.surface_top(x, z) + 3 < 63;
            if w {
                waters += 1;
            } else {
                lands += 1;
            }
            if w != ex {
                mm += 1;
            }
        }
        eprintln!("waters={waters} lands={lands} fast-vs-exact mismatch={mm}");
        assert!(waters > 30 && lands > 30);
        assert!(mm < 40, "快分类与精确判水分歧过多: {mm}");
    }
}
