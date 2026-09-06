//! 生物群系色板（预测瓦片用）：人工校准的"群系地图"观感色
//! （非方块色；悬停时名称以词典为准）。

use std::collections::HashMap;
use std::sync::LazyLock;

static PALETTE: LazyLock<HashMap<&'static str, [u8; 3]>> = LazyLock::new(|| {
    [
        ("minecraft:ocean", [44, 84, 156]),
        ("minecraft:deep_ocean", [24, 46, 100]),
        ("minecraft:cold_ocean", [88, 140, 192]),
        ("minecraft:deep_cold_ocean", [62, 100, 148]),
        ("minecraft:frozen_ocean", [120, 150, 190]),
        ("minecraft:deep_frozen_ocean", [96, 126, 168]),
        ("minecraft:lukewarm_ocean", [60, 120, 178]),
        ("minecraft:deep_lukewarm_ocean", [44, 96, 152]),
        ("minecraft:warm_ocean", [52, 150, 196]),
        ("minecraft:deep_warm_ocean", [40, 110, 160]),
        ("minecraft:river", [54, 110, 172]),
        ("minecraft:frozen_river", [98, 140, 188]),
        ("minecraft:beach", [214, 208, 152]),
        ("minecraft:snowy_beach", [212, 214, 210]),
        ("minecraft:stony_shore", [132, 134, 132]),
        ("minecraft:swamp", [64, 96, 66]),
        ("minecraft:mangrove_swamp", [56, 84, 66]),
        ("minecraft:plains", [138, 176, 84]),
        ("minecraft:sunflower_plains", [138, 176, 84]),
        ("minecraft:snowy_plains", [224, 232, 232]),
        ("minecraft:ice_spikes", [210, 226, 236]),
        ("minecraft:forest", [52, 128, 54]),
        ("minecraft:flower_forest", [88, 148, 80]),
        ("minecraft:birch_forest", [98, 148, 80]),
        ("minecraft:old_growth_birch_forest", [104, 156, 84]),
        ("minecraft:dark_forest", [38, 88, 42]),
        ("minecraft:cherry_grove", [214, 160, 168]),
        ("minecraft:taiga", [44, 108, 84]),
        ("minecraft:snowy_taiga", [72, 132, 116]),
        ("minecraft:old_growth_pine_taiga", [50, 104, 82]),
        ("minecraft:old_growth_spruce_taiga", [44, 96, 78]),
        ("minecraft:jungle", [74, 148, 52]),
        ("minecraft:bamboo_jungle", [60, 140, 52]),
        ("minecraft:sparse_jungle", [96, 156, 74]),
        ("minecraft:savanna", [176, 168, 88]),
        ("minecraft:savanna_plateau", [164, 152, 80]),
        ("minecraft:windswept_savanna", [152, 148, 82]),
        ("minecraft:badlands", [218, 148, 92]),
        ("minecraft:eroded_badlands", [206, 128, 78]),
        ("minecraft:wooded_badlands", [188, 136, 92]),
        ("minecraft:meadow", [132, 176, 92]),
        ("minecraft:grove", [120, 156, 110]),
        ("minecraft:snowy_slopes", [226, 230, 228]),
        ("minecraft:frozen_peaks", [232, 238, 238]),
        ("minecraft:jagged_peaks", [178, 186, 188]),
        ("minecraft:stony_peaks", [146, 150, 152]),
        ("minecraft:windswept_hills", [114, 126, 112]),
        ("minecraft:windswept_gravelly_hills", [122, 128, 120]),
        ("minecraft:windswept_forest", [86, 116, 82]),
        ("minecraft:mushroom_fields", [184, 132, 166]),
        ("minecraft:dripstone_caves", [124, 96, 74]),
        ("minecraft:lush_caves", [88, 140, 96]),
        ("minecraft:deep_dark", [28, 34, 44]),
        ("minecraft:pale_garden", [172, 190, 172]),
        ("minecraft:desert", [218, 192, 116]),
    ]
    .into_iter()
    .collect()
});

/// 名称 → RGB（预测瓦片按 4×4 群系单元整格填充）。
pub fn biome_color(name: &str) -> [u8; 3] {
    PALETTE.get(name).copied().unwrap_or([140, 140, 140])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_covers_table() {
        let t = crate::map::biomegen::overworld_26();
        for n in &t.names {
            assert!(biome_color(n) != [140, 140, 140], "缺少色板: {n}");
        }
    }
}
