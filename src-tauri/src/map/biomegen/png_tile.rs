//! 预测瓦片渲染：region（512×512 方块）→ 512×512 PNG。
//! 生物群系按 4×4 块单元整格填色（与 MC 群系分辨率一致）。
//! 地形版（v2）：单元中央列求解表面高度——水体按深度分层着色，
//! 陆地保留群系色（弱化 12%）。
//! 性能：128×128 单元按行带 scoped 并行（8 线程封顶）；单元计算零分配。

use std::io::Cursor;

use super::colors::biome_color;
use super::table::overworld_26;
use super::terrain::solver::TerrainEngine;
use super::BiomePredictor;

const CELLS: usize = 128; // 每轴单元数（512 / 4）
const ROW_BYTES: usize = 512 * 4; // 一行像素 RGBA
const CELL_BLOCKS: i32 = 4;

/// 水体色带（浅→深），按分类 rank 分档（贴岸浅水/外海深蓝）。
const SHALLOW_B: [u8; 3] = [58, 130, 202];
const DEEP: [u8; 3] = [28, 74, 160];

/// region 内 4×4 群系单元填色渲染（固定 quart y=16 ≈ 地表深度档；
/// 正常群系行成对覆盖 0/1 深度，结果与 y 无关）。
pub fn render_region_prediction(pred: &BiomePredictor, rx: i32, rz: i32) -> Result<Vec<u8>, String> {
    render_region_prediction_terrain(pred, None, rx, rz)
}

/// 地形版：`eng` 提供时单元表面/水深参与着色（水替群系色，陆仍群系色）。
pub fn render_region_prediction_terrain(
    pred: &BiomePredictor,
    eng: Option<&TerrainEngine>,
    rx: i32,
    rz: i32,
) -> Result<Vec<u8>, String> {
    let table = overworld_26();
    let mut buf = vec![0u8; 512 * ROW_BYTES];
    // 行带划分（每带整行字节区间，线程间零重叠）。
    // 线程数 = 逻辑核一半（SMT 下留余量给 UI/主进程；高负载时资源可控）
    let n = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(2, 8))
        .unwrap_or(4);
    let mut ranges = Vec::with_capacity(n);
    for t in 0..n {
        ranges.push((CELLS * t / n, CELLS * (t + 1) / n));
    }
    // 每带一段连续字节（整 4 行单元对齐），move 进各线程，无共享可变借用
    let mut bands: Vec<&mut [u8]> = Vec::with_capacity(n);
    let mut rest: &mut [u8] = &mut buf[..];
    for (cz0, cz1) in ranges.iter().take(n - 1) {
        let len = (cz1 - cz0) * 4 * ROW_BYTES;
        let (band, tail) = rest.split_at_mut(len);
        bands.push(band);
        rest = tail;
    }
    bands.push(rest);

    // 引擎为只读共享（DensityRoot 无内部可变状态）；Option<&T> 是 Copy，
    // 线程闭包按值捕获共享引用即可（scoped 线程保证生命周期）。
    std::thread::scope(|s| {
        for (band, (cz0, _cz1)) in bands.into_iter().zip(ranges) {
            s.spawn(move || {
                for (row_i, row_bytes) in band.chunks_mut(ROW_BYTES).enumerate() {
                    let cz = cz0 + row_i / 4;
                    for cx in 0..CELLS {
                        let qx = rx * CELLS as i32 + cx as i32;
                        let qz = rz * CELLS as i32 + cz as i32;
                        // 水域判定（引擎存在时）：单元中心列
                        let mut water_rgb: Option<[u8; 3]> = None;
                        if let Some(eng) = eng {
                            let wx = (qx * CELL_BLOCKS + 2) as f64;
                            let wz = (qz * CELL_BLOCKS + 2) as f64;
                            match eng.water_class(wx, wz) {
                                Some(1) => water_rgb = Some(SHALLOW_B),
                                Some(_) => water_rgb = Some(DEEP),
                                None => {}
                            }
                        }
                        let rgb = if let Some(w) = water_rgb {
                            w
                        } else {
                            let id = pred.predict_cell(qx, qz, 16);
                            if id as usize >= table.names.len() {
                                continue;
                            }
                            let [r, g, b] = biome_color(table.names[id as usize]);
                            // 预测语义弱化 12%，与真实渲染瓦片拉开观感
                            [
                                (r as u16 * 88 / 100) as u8,
                                (g as u16 * 88 / 100) as u8,
                                (b as u16 * 88 / 100) as u8,
                            ]
                        };
                        let px = [rgb[0], rgb[1], rgb[2], 255u8];
                        let p = (cx * 4) * 4;
                        for dx in 0..4usize {
                            row_bytes[p + dx * 4..p + dx * 4 + 4].copy_from_slice(&px);
                        }
                    }
                }
            });
        }
    });

    let img = image::RgbaImage::from_raw(512, 512, buf).expect("缓冲尺寸恒定 512²");
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    let mut out = Cursor::new(Vec::new());
    dyn_img
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("预测瓦片编码失败\n{e}"))?;
    Ok(out.into_inner())
}
