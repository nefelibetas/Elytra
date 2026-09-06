//! 地形高度引擎（seed → 表面高度/水陆）——26.1.2 final_density 全移植。
//!
//! 原理：真实世界每列自顶向下首个实心 Y（final_density ρ>0）为表面高度 h；
//! h<63 判水（外加表土盖层 CAP=3 修正海岸/低地）。
//! 移植内容：noise_settings 内联树 + density_function 闭包（20 文件）+
//! 43 个 noise 实例 + legacy BlendedNoise。
//!
//! 进度：
//! - [x] 数据嵌入 terrain_26.json
//! - [x] Mapped 一元算子（字节码钉死）
//! - [x] 噪声银行（NormalNoise 全实例 + BlendedNoise 播种链）
//! - [x] 通用求值器（arena 树：二元/一元/clamp/梯度/range/noise/
//!   shifted_noise(shift_a/b)/spline/weird_scaled/blended）
//! - [x] 实档水陆对拍：森林/内陆带（5 region 前段 chunk）96.9%；全档 16 region
//!   12.6k 列 82~85%——北西大片沼泽/滩涂/浅海带（真实水面 62）由地表规则
//!   （深 dirt 岛 4~10 层、pond 沟交错）+ chunk 块插值主导，逐列精确需
//!   surface-rules + 水位模拟（跨日级）；渲染为 4×4 单元整格，视觉影响小
//! - [x] solver：共享引擎 + 盖层判水 + 快分类（~0.7ms/列 debug）
//! - [x] png_tile 水域分层渲染 + 缓存键 v2
//!
//! 已知残余（对拍列级，非像素级）：湖岸浅水底（顶 62）与洼地口袋
//! 密度边界 ±1~3 的列（~2.4%），需 surface-rules/水位模型才能根治——
//! 渲染为 4×4 单元整格，视觉影响更小。

use std::sync::LazyLock;

use serde_json::Value;

pub mod bank;
pub mod eval;
pub mod legacy;
pub mod solver;

const TERRAIN_JSON: &str = include_str!("../terrain_26.json");

/// 内嵌数据包（noise_settings.overworld / densities / noise_specs）。
pub struct TerrainData {
    pub noise_settings: Value,
    pub densities: std::collections::HashMap<String, Value>,
    pub noise_specs: Value,
}

static DATA: LazyLock<TerrainData> = LazyLock::new(|| {
    let root: Value = serde_json::from_str(TERRAIN_JSON).expect("terrain_26.json 内嵌解析失败");
    let obj = root.as_object().expect("包根为对象");
    let mut densities = std::collections::HashMap::new();
    for (k, v) in obj["densities"].as_object().expect("densities 为对象") {
        densities.insert(k.clone(), v.clone());
    }
    TerrainData {
        noise_settings: obj["noise_settings"]["overworld"].clone(),
        densities,
        noise_specs: obj["noise_specs"].clone(),
    }
});

/// 数据访问（惰性解析一次）。
pub fn data() -> &'static TerrainData {
    &DATA
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_data_loads() {
        let d = data();
        assert!(d.noise_settings.get("noise_router").is_some());
        assert_eq!(d.densities.len(), 20);
        let specs = d.noise_specs.as_object().expect("noise_specs");
        assert!(specs.contains_key("cave_cheese"));
        assert!(specs.contains_key("surface"));
        assert_eq!(specs.len(), 42);
    }
}
