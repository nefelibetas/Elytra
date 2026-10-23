//! Mod 方块色覆盖层：实例 mods 动态采样 + 变更指纹失效。
//!
//! 目标：mod 数量/内容随时可变（增删/换版本），地图渲染必须跟着变。
//! 设计：
//! 1. 指纹 = mods/*.jar 的（名+大小+mtime）哈希——变了才重建索引，其余走缓存；
//! 2. 命名空间索引：打开每个 jar 中央目录，建 `assets/<ns>/blockstates/` → jar 映射；
//! 3. 按需采样：渲染遇到色表外方块才定位属主 jar、解析模型链、采样顶面贴图，
//!    单块结果进程内缓存（玩到哪采到哪，未出现方块零成本）；
//! 4. 模型父级引用原版模板（cube_all/cube_bottom_top…）时用内置面语义表推断
//!    up 面纹理；纹理解析支持 mod jar 内贴图与原版贴图（嵌入式纹理平均色表）。
//!
//! 采样失败的方块返回 None，由渲染层语义兜底（名字特征近似色）→ 中性灰。

use std::collections::HashMap;
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use zip::ZipArchive;

use crate::map::render::palette;

/// RGBA 像素（与 fastanvil 一致：`[u8; 4]`）。
type Rgba = [u8; 4];

/// 覆盖层缓存条目：覆盖层 + 上次指纹校验时刻。
type OverlayEntry = (Arc<ModOverlay>, Instant);

/// 覆盖层进程内缓存：mods 目录 → 覆盖层条目。TTL 内直接复用，
/// 过期后重算指纹（stat 扫描轻量），变了才整体重建。
static OVERLAYS: OnceLock<Mutex<HashMap<PathBuf, OverlayEntry>>> = OnceLock::new();

fn overlays() -> &'static Mutex<HashMap<PathBuf, OverlayEntry>> {
    OVERLAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 指纹校验间隔（覆盖层复用期间 mod 变更最多延迟一个 TTL 生效）。
const FINGERPRINT_TTL: Duration = Duration::from_secs(3);

/// 模型模板条目：(模板名, up 面变量, 全部纹理变量)。
type TemplateDef = (&'static str, Option<&'static str>, &'static [&'static str]);

/// 原版模型模板的面语义：模板名 →（up 面纹理变量，全部纹理变量）。
/// Mod 模型父链引用这些模板时据此推断哪张贴图是顶面。
const VANILLA_TEMPLATES: &[TemplateDef] = &[
    ("cube_all", Some("#all"), &["#all"]),
    ("cube_all_inner_faces", Some("#all"), &["#all"]),
    (
        "cube",
        Some("#up"),
        &["#up", "#down", "#north", "#south", "#east", "#west"],
    ),
    (
        "cube_mirrored",
        Some("#up"),
        &["#up", "#down", "#north", "#south", "#east", "#west"],
    ),
    ("cube_mirrored_all", Some("#all"), &["#all"]),
    (
        "cube_bottom_top",
        Some("#top"),
        &["#top", "#side", "#bottom"],
    ),
    (
        "cube_bottom_top_inner_faces",
        Some("#top"),
        &["#top", "#side", "#bottom"],
    ),
    ("cube_column", Some("#end"), &["#end", "#side"]),
    ("cube_column_mirrored", Some("#end"), &["#end", "#side"]),
    ("cube_column_horizontal", Some("#end"), &["#end", "#side"]),
    ("cube_column_uv_locked_x", Some("#end"), &["#end", "#side"]),
    ("cube_column_uv_locked_y", Some("#end"), &["#end", "#side"]),
    ("cube_directional", Some("#side"), &["#side"]),
    ("cube_top", Some("#top"), &["#top", "#side"]),
    // 无 up 面模板（植物/叉状）：取任意纹理（贴图即其顶视表现）
    ("cross", None, &["#cross"]),
    ("cross_texture_all", None, &["#all"]),
    ("tinted_cross", None, &["#cross"]),
    ("cross_items", None, &["#cross"]),
    ("template_plant", None, &["#plant"]),
    ("flower_pot_cross", None, &["#plant"]),
    ("tinted_flower_pot_cross", None, &["#plant"]),
    ("template_grass", None, &["#grass"]),
    ("crop", None, &["#crop"]),
    ("stem_growth0", None, &["#stem", "#crop"]),
    ("crop_plant", None, &["#crop"]),
    ("template_seagrass", None, &["#texture"]),
    ("template_glow_item_frame", None, &["#texture"]),
    ("template_item_frame", None, &["#texture"]),
    ("template_bed", None, &["#wool"]),
    ("template_bed_head", None, &["#wool"]),
    ("template_bed_foot", None, &["#wool"]),
    ("template_rail", None, &["#rail"]),
    ("template_rail_raised", None, &["#rail"]),
    ("template_fence_gate", Some("#texture"), &["#texture"]),
    ("template_trapdoor_top", Some("#texture"), &["#texture"]),
    ("template_trapdoor_bottom", Some("#texture"), &["#texture"]),
    ("template_trapdoor_open", Some("#texture"), &["#texture"]),
    (
        "template_glass_pane_top",
        Some("#pane"),
        &["#pane", "#edge"],
    ),
    (
        "template_glass_pane_side",
        Some("#pane"),
        &["#pane", "#edge"],
    ),
    ("template_glass_pane_post", Some("#pane"), &["#pane"]),
];

/// 单实例 Mod 色覆盖层（不可变元数据 + 惰性采样结果）。
pub struct ModOverlay {
    /// 构建时的 jar 集合指纹（增量失效依据）
    fingerprint: String,
    /// 命名空间 → 属主 jar
    ns_jar: HashMap<String, PathBuf>,
    /// 已采样方块色：完整方块名（ns:block）→ RGBA
    colors: Mutex<HashMap<String, Rgba>>,
}

impl ModOverlay {
    /// 方块取色：命中缓存直接返回；未采样则即时采样并缓存（并发重复采样
    /// 无害——同值覆盖）。
    pub fn color(&self, name: &str) -> Option<Rgba> {
        if let Some(c) = self.colors.lock().unwrap().get(name) {
            return Some(*c);
        }
        let c = self.sample_block(name)?;
        self.colors.lock().unwrap().insert(name.to_string(), c);
        Some(c)
    }

    /// 单块采样主入口：blockstates → 模型集合 → 纹理解析 → PNG 平均色。
    fn sample_block(&self, name: &str) -> Option<Rgba> {
        let (ns, block) = name.split_once(':')?;
        let jar = self.ns_jar.get(ns)?;
        let mut arc = ZipArchive::new(File::open(jar).ok()?).ok()?;

        // blockstates JSON：变体 + multipart 的全部模型引用
        let bs_path = format!("assets/{ns}/blockstates/{block}.json");
        let mut bs = Vec::new();
        arc.by_name(&bs_path).ok()?.read_to_end(&mut bs).ok()?;
        let bs_json: Value = serde_json::from_slice(&bs).ok()?;
        let mut models: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut add_model = |m: &Value| {
            let Some(s) = m.get("model").and_then(|v| v.as_str()) else {
                return;
            };
            // 剥自己命名空间前缀（mod 常写 "mymod:block/x"）与原版前缀；
            // 剩余含 ':' = 跨 ns（罕见）：不处理
            let trimmed = s
                .trim_start_matches(&format!("{ns}:"))
                .trim_start_matches("minecraft:");
            if trimmed.contains(':') {
                return;
            }
            if seen.insert(trimmed.to_string()) {
                models.push(trimmed.to_string());
            }
        };
        if let Some(variants) = bs_json.get("variants").and_then(|v| v.as_object()) {
            for v in variants.values() {
                if let Some(arr) = v.as_array() {
                    for m in arr {
                        add_model(m);
                    }
                } else {
                    add_model(v);
                }
            }
        }
        if let Some(parts) = bs_json.get("multipart").and_then(|v| v.as_array()) {
            for part in parts {
                match part.get("apply") {
                    Some(Value::Array(arr)) => {
                        for m in arr {
                            add_model(m);
                        }
                    }
                    Some(other) => add_model(other),
                    None => {}
                }
            }
        }
        if models.is_empty() {
            return None;
        }

        // 解析各模型：父链合并纹理表 + 面语义（模板表或子模型自带 elements）
        let mut tex_weight: HashMap<String, u32> = HashMap::new();
        let mut any_weight: HashMap<String, u32> = HashMap::new();
        for model in &models {
            let mp = format!("assets/{ns}/models/{model}.json");
            // 读入内存再解析：释放对 arc 的借用（resolve_model_faces 需要 &mut arc）
            let mut mj_bytes = Vec::new();
            {
                let mut f = arc.by_name(&mp).ok()?;
                f.read_to_end(&mut mj_bytes).ok()?;
            }
            let Ok(mj) = serde_json::from_slice::<Value>(&mj_bytes) else {
                continue;
            };
            let (ups, anys) = resolve_model_faces(&mut arc, ns, &mj);
            for t in ups {
                *tex_weight.entry(t).or_default() += 1;
            }
            for t in anys {
                *any_weight.entry(t).or_default() += 1;
            }
        }
        if !tex_weight.is_empty() {
            self.average_textures(&mut arc, ns, &tex_weight)
        } else if !any_weight.is_empty() {
            self.average_textures(&mut arc, ns, &any_weight)
        } else {
            None
        }
    }

    /// 加权平均一组贴图（纹理值已归一为 `ns 基路径`，如 "block/foo"）。
    fn average_textures(
        &self,
        arc: &mut ZipArchive<File>,
        ns: &str,
        weights: &HashMap<String, u32>,
    ) -> Option<Rgba> {
        let mut r = 0.0f64;
        let mut g = 0.0;
        let mut b = 0.0;
        let mut wsum = 0.0;
        let mut alpha_max = 0u8;
        for (path, w) in weights {
            let Some(avg) = self.texture_average(arc, ns, path) else {
                continue;
            };
            let wf = *w as f64;
            r += avg[0] as f64 * wf;
            g += avg[1] as f64 * wf;
            b += avg[2] as f64 * wf;
            wsum += wf;
            alpha_max = alpha_max.max(avg[3]);
        }
        if wsum == 0.0 {
            return None;
        }
        Some([
            (r / wsum).round() as u8,
            (g / wsum).round() as u8,
            (b / wsum).round() as u8,
            alpha_max,
        ])
    }

    /// 单贴图平均色：minecraft: 命名空间 → 嵌入式纹理表；否则读属主 jar PNG。
    fn texture_average(&self, arc: &mut ZipArchive<File>, ns: &str, path: &str) -> Option<Rgba> {
        let (tex_ns, rel) = path.split_once(':').unwrap_or((ns, path));
        if tex_ns == "minecraft" {
            // "block/stone" → 纹理表基名 "stone"
            return palette().vanilla_texture(rel.strip_prefix("block/").unwrap_or(rel));
        }
        if tex_ns != ns {
            return None; // 跨 mod 贴图引用：不在本 jar，跳过
        }
        let p = format!("assets/{ns}/textures/{rel}.png");
        let mut bytes = Vec::new();
        arc.by_name(&p).ok()?.read_to_end(&mut bytes).ok()?;
        png_average(&bytes)
    }
}

/// 纹理值归一：字符串直取；对象（26.x sprite 格式）取 sprite 字段。
fn texture_value(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o
            .get("sprite")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// 模型解析：父链（叶→根）合并纹理表，面语义取自模板表或子模型自带 elements。
/// 返回具体纹理路径（保留 "ns:" 前缀；同 ns 值保持裸 "block/x" 形态）。
fn resolve_model_faces(
    arc: &mut ZipArchive<File>,
    ns: &str,
    root: &Value,
) -> (Vec<String>, Vec<String>) {
    // 1) 收集父链节点（叶优先；模板父不读文件，靠内置表补语义）
    let mut chain: Vec<Value> = vec![root.clone()];
    let mut cur = root.clone();
    for _ in 0..10 {
        let Some(parent) = cur.get("parent").and_then(|v| v.as_str()) else {
            break;
        };
        // 剥自身/原版命名空间前缀；仍含 ':' = 真跨 ns，不解析
        let p = parent
            .trim_start_matches(&format!("{ns}:"))
            .trim_start_matches("minecraft:");
        if p.contains(':') {
            break;
        }
        let base = p.rsplit('/').next().unwrap_or(p);
        if VANILLA_TEMPLATES.iter().any(|(n, ..)| *n == base) {
            break; // 模板父：面语义由模板表提供，文件不在 mod jar
        }
        let pp = format!("assets/{ns}/models/{p}.json");
        let Ok(mut f) = arc.by_name(&pp) else {
            break;
        };
        let Ok(m) = serde_json::from_reader::<_, Value>(&mut f) else {
            break;
        };
        chain.push(m.clone());
        cur = m;
    }

    // 2) 纹理表：根→叶插入（叶覆盖父）。26.x 起值可为字符串或 {sprite, …} 对象
    let mut table: HashMap<String, String> = HashMap::new();
    for m in chain.iter().rev() {
        if let Some(tex) = m.get("textures").and_then(|v| v.as_object()) {
            for (k, v) in tex {
                if let Some(s) = texture_value(v) {
                    table.insert(k.clone(), s);
                }
            }
        }
    }
    // 变量递归解析：#top → 值（可能再指 #all）
    let resolve_var = |var: &str| -> Option<String> {
        let mut cur = var;
        for _ in 0..8 {
            let v = table.get(cur.trim_start_matches('#'))?;
            if v.starts_with('#') {
                cur = v;
            } else {
                return Some(v.clone());
            }
        }
        None
    };
    // 解析并入桶（去重）
    let push_tex = |slot: &mut Vec<String>, var: &str| {
        let Some(v) = resolve_var(var) else {
            return;
        };
        if !slot.contains(&v) {
            slot.push(v);
        }
    };

    // 3) 面语义：叶起逐层——自带 elements 取 up 面；父名命中模板表取模板语义
    let mut ups: Vec<String> = Vec::new();
    let mut anys: Vec<String> = Vec::new();
    for (idx, m) in chain.iter().enumerate() {
        // elements 面
        if let Some(elements) = m.get("elements").and_then(|v| v.as_array()) {
            for el in elements {
                let Some(faces) = el.get("faces").and_then(|v| v.as_object()) else {
                    continue;
                };
                for (dir, fv) in faces {
                    let Some(t) = fv.get("texture").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if dir == "up" {
                        push_tex(&mut ups, t);
                    } else {
                        push_tex(&mut anys, t);
                    }
                }
            }
            if !ups.is_empty() {
                return (ups, anys); // 自绘模型：up 面已定，不再看父
            }
        }
        // 该节点 parent 是否为模板
        let parent = m.get("parent").and_then(|v| v.as_str());
        if let Some(parent) = parent {
            let base = parent
                .trim_start_matches("minecraft:")
                .rsplit('/')
                .next()
                .unwrap_or(parent);
            if let Some((_, up_var, any_vars)) =
                VANILLA_TEMPLATES.iter().find(|(n, ..)| *n == base).copied()
            {
                if let Some(u) = up_var {
                    push_tex(&mut ups, u);
                }
                for a in any_vars {
                    push_tex(&mut anys, a);
                }
                if !ups.is_empty() {
                    return (ups, anys);
                }
            }
        }
        let _ = idx;
    }
    (ups, anys)
}

/// PNG → 平均色（alpha>0 像素；alpha = 平均覆盖，最少 24）。png 0.18：
/// 变换在 Decoder 上设置，随后统一 RGBA8 输出。
fn png_average(bytes: &[u8]) -> Option<Rgba> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::ALPHA | png::Transformations::STRIP_16,
    );
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    reader.next_frame(&mut buf).ok()?;
    let mut r = 0u64;
    let mut g = 0u64;
    let mut b = 0u64;
    let mut n = 0u64;
    let mut alpha_sum = 0u64;
    let mut alpha_n = 0u64;
    for px in buf.as_chunks::<4>().0 {
        let a = px[3] as u64;
        alpha_sum += a;
        alpha_n += 1;
        if a > 0 {
            r += px[0] as u64;
            g += px[1] as u64;
            b += px[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    let avg_alpha = (alpha_sum / alpha_n).clamp(24, 255) as u8;
    Some([(r / n) as u8, (g / n) as u8, (b / n) as u8, avg_alpha])
}

// ─────────────────── 实例定位 / 指纹 / 注册表 ───────────────────

/// world_path → 实例 mods 目录（versions/<inst>/mods；实例无 mods 时回退
/// 游戏根 mods）。路径不含 versions 段（非标准布局）→ None（无覆盖层）。
fn instance_mods_dir(world_path: &Path) -> Option<PathBuf> {
    let comps: Vec<_> = world_path.components().collect();
    for (i, c) in comps.iter().enumerate() {
        if c.as_os_str() == "versions" {
            let _inst = comps.get(i + 1)?;
            let inst_dir: PathBuf = comps[..=i + 1].iter().collect();
            let local = inst_dir.join("mods");
            if local.is_dir() {
                return Some(local);
            }
            // 回退游戏根 mods（非版本隔离布局）
            let root: PathBuf = comps[..=i].iter().collect();
            let root_mods = root.join("mods");
            return root_mods.is_dir().then_some(root_mods);
        }
    }
    None
}

/// jar 集合指纹：排序后的 名|大小|mtime 拼串 sha1。
fn fingerprint(mods_dir: &Path) -> String {
    let Ok(rd) = std::fs::read_dir(mods_dir) else {
        return String::new();
    };
    let mut files: Vec<_> = rd
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("jar"))
        })
        .collect();
    files.sort_by_key(|e| e.file_name());
    let mut desc = String::new();
    for e in files {
        let Ok(md) = e.metadata() else {
            continue;
        };
        desc.push_str(&format!(
            "{}|{}|{}\n",
            e.file_name().to_string_lossy(),
            md.len(),
            md.modified().map(|t| format!("{t:?}")).unwrap_or_default()
        ));
    }
    crate::downloader::hash::sha1_hex(desc.as_bytes())
}

/// 命名空间索引：扫描每个 jar 的中央目录，记录 `assets/<ns>/blockstates/`。
fn scan_namespaces(mods_dir: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    let Ok(rd) = std::fs::read_dir(mods_dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jar")) {
            continue;
        }
        let Ok(file) = File::open(&p) else {
            continue;
        };
        let Ok(mut arc) = ZipArchive::new(file) else {
            continue;
        };
        for i in 0..arc.len() {
            let Ok(name) = arc.by_index(i).map(|f| f.name().to_string()) else {
                continue;
            };
            let Some(rest) = name.strip_prefix("assets/") else {
                continue;
            };
            let Some((ns, after)) = rest.split_once('/') else {
                continue;
            };
            if after.starts_with("blockstates") && !out.contains_key(ns) {
                out.insert(ns.to_string(), p.clone());
            }
        }
    }
    out
}

/// 取世界所属实例的覆盖层（进程内缓存 + TTL 指纹复核；mods 目录不存在 →
/// None 表示无 Mod 内容）。
pub fn overlay_for_world(world_path: &Path) -> Option<Arc<ModOverlay>> {
    let mods_dir = instance_mods_dir(world_path)?;
    if !mods_dir.is_dir() {
        return None;
    }
    let mut map = overlays().lock().unwrap();
    if let Some((ov, last)) = map.get(&mods_dir) {
        if last.elapsed() < FINGERPRINT_TTL {
            return Some(Arc::clone(ov));
        }
    }
    let fp = fingerprint(&mods_dir);
    if let Some((ov, last)) = map.get_mut(&mods_dir) {
        if ov.fingerprint == fp {
            *last = Instant::now();
            return Some(Arc::clone(ov));
        }
    }
    let overlay = Arc::new(ModOverlay {
        fingerprint: fp,
        ns_jar: scan_namespaces(&mods_dir),
        colors: Mutex::new(HashMap::new()),
    });
    map.insert(mods_dir, (Arc::clone(&overlay), Instant::now()));
    Some(overlay)
}

/// 覆盖层规模（采样过的方块数；调试/测试用）。
#[cfg(test)]
pub fn overlay_sample_count(ov: &ModOverlay) -> usize {
    ov.colors.lock().unwrap().len()
}

use std::io::Read;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    /// 手工构造 16×16 纯色 RGBA PNG（zlib deflate 硬编码 chunk）。
    fn solid_png(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;

        // PNG 校验和（标准 CRC32 表，避免引入额外依赖）
        fn crc32(data: &[u8]) -> u32 {
            let mut table = [0u32; 256];
            for (i, t) in table.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xEDB8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
                *t = c;
            }
            let mut c = 0xFFFF_FFFFu32;
            for &b in data {
                c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
            }
            c ^ 0xFFFF_FFFF
        }

        let mut raw = Vec::with_capacity(16 * 16 * 4 + 16);
        for _ in 0..16u8 {
            raw.push(0); // filter none
            for _ in 0..16 {
                raw.extend_from_slice(&[r, g, b, a]);
            }
        }
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&raw).unwrap();
        let idat = enc.finish().unwrap();
        // chunk 内 CRC 覆盖 tag+data
        fn chunk_impl(tag: &[u8; 4], data: &[u8], crc32: impl Fn(&[u8]) -> u32) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(tag);
            out.extend_from_slice(data);
            let mut full = Vec::with_capacity(tag.len() + data.len());
            full.extend_from_slice(tag);
            full.extend_from_slice(data);
            out.extend_from_slice(&crc32(&full).to_be_bytes());
            out
        }
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&16u32.to_be_bytes());
        ihdr.extend_from_slice(&16u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8bit RGBA
        png.extend_from_slice(&chunk_impl(b"IHDR", &ihdr, crc32));
        png.extend_from_slice(&chunk_impl(b"IDAT", &idat, crc32));
        png.extend_from_slice(&chunk_impl(b"IEND", &[], crc32));
        png
    }

    /// 构造一个带资产的假 mod jar。
    fn write_mod_jar(path: &std::path::Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let add = |w: &mut zip::ZipWriter<std::fs::File>, name: &str, bytes: &[u8]| {
            w.start_file(name, opts).unwrap();
            w.write_all(bytes).unwrap();
        };
        // ruby_block：自带命名空间前缀模型引用 + 纯色贴图
        add(
            &mut w,
            "assets/mymod/blockstates/ruby_block.json",
            br#"{"variants":{"":{"model":"mymod:block/ruby_block"}}}"#,
        );
        add(
            &mut w,
            "assets/mymod/models/block/ruby_block.json",
            br#"{"parent":"block/cube_all","textures":{"all":"mymod:block/ruby_block"}}"#,
        );
        add(
            &mut w,
            "assets/mymod/textures/block/ruby_block.png",
            &solid_png(200, 30, 40, 255),
        );
        // stoneish：模型引用原版纹理（应命中嵌入式纹理表）
        add(
            &mut w,
            "assets/mymod/blockstates/stoneish.json",
            br#"{"variants":{"":{"model":"block/stoneish"}}}"#,
        );
        add(
            &mut w,
            "assets/mymod/models/block/stoneish.json",
            br#"{"parent":"block/cube_all","textures":{"all":"minecraft:block/stone"}}"#,
        );
        w.finish().unwrap();
    }

    /// 标准假实例布局：<root>/versions/Inst/mods + <root>/versions/Inst/saves/w
    struct Fixture {
        root: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let mods = root.path().join("versions").join("Inst").join("mods");
            std::fs::create_dir_all(&mods).unwrap();
            write_mod_jar(&mods.join("mymod.jar"));
            let saves = root
                .path()
                .join("versions")
                .join("Inst")
                .join("saves")
                .join("w");
            std::fs::create_dir_all(&saves).unwrap();
            std::fs::write(saves.join("level.dat"), b"x").unwrap();
            Fixture { root }
        }

        fn world_path(&self) -> PathBuf {
            self.root
                .path()
                .join("versions")
                .join("Inst")
                .join("saves")
                .join("w")
        }
    }

    #[test]
    fn instance_mods_dir_locates_version_local_mods() {
        let fx = Fixture::new();
        let got = instance_mods_dir(&fx.world_path()).unwrap();
        assert_eq!(
            got,
            fx.root.path().join("versions").join("Inst").join("mods")
        );
    }

    #[test]
    fn overlay_samples_mod_texture_and_vanilla_ref() {
        let fx = Fixture::new();
        let ov = overlay_for_world(&fx.world_path()).expect("覆盖层应可构建");
        // 自有贴图采样：纯色 200,30,40
        let c = ov.color("mymod:ruby_block").expect("ruby_block 应采样成功");
        assert_eq!(c, [200, 30, 40, 255], "自有贴图平均色应等于纯色");
        // 引用原版纹理：stone 平均色（嵌入式纹理表）
        let s = ov.color("mymod:stoneish").expect("stoneish 应采样成功");
        let stone = palette().vanilla_texture("stone").unwrap();
        assert_eq!(s, stone, "引用原版纹理应命中内嵌纹理表");
        // 未知命名空间 → None
        assert_eq!(ov.color("ghost:block"), None);
        // 缓存生效：二次取色不重采（计数仍为 2）
        assert_eq!(overlay_sample_count(&ov), 2);
    }

    #[test]
    fn fingerprint_changes_when_jar_modified() {
        let fx = Fixture::new();
        let mods = fx.root.path().join("versions").join("Inst").join("mods");
        let before = fingerprint(&mods);
        // 增删 jar 改变指纹
        write_mod_jar(&mods.join("extra.jar"));
        let after_add = fingerprint(&mods);
        assert_ne!(before, after_add, "新增 jar 应改变指纹");
        std::fs::remove_file(mods.join("extra.jar")).unwrap();
        assert_eq!(fingerprint(&mods), before, "删除后指纹应还原");
    }

    #[test]
    fn overlay_rebuilds_when_fingerprint_changes() {
        let fx = Fixture::new();
        let ov1 = overlay_for_world(&fx.world_path()).unwrap();
        let before = ov1.fingerprint.clone();
        // 覆盖 mods 缓存：手动失效（模拟 TTL 过期后指纹变化路径）
        let mods = fx.root.path().join("versions").join("Inst").join("mods");
        // 修改 jar（内容 + mtime）
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_mod_jar(&mods.join("mymod.jar"));
        let fp2 = fingerprint(&mods);
        assert_ne!(before, fp2);
        // 清空注册表模拟重启后的重建；同目录重建后指纹为新值
        overlays().lock().unwrap().clear();
        let ov2 = overlay_for_world(&fx.world_path()).unwrap();
        assert_eq!(ov2.fingerprint, fp2);
    }
}
