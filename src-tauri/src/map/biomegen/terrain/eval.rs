//! 通用密度函数求值器 v2（26.1.2 闭包，arena 树，引用归一）。
//!
//! 引用串（"minecraft:overworld/factor" 等）统一归一为 densities 键
//! （去 minecraft: 前缀）。样条：{"spline":{coordinate, points:[{location,
//! derivative, value: 数值|内嵌样条}]}}——值可为数或任意函数（此处递归求值节点）。
//! 非混合常量：blend_alpha=1、blend_offset=0；blend_density/interpolated/cache_* 透传。

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use super::data;
use super::bank::NoiseBank;

#[derive(Clone, Copy)]
enum BinOp {
    Add,
    Mul,
    Min,
    Max,
}

enum Node {
    Lit(f64),
    Ref(String),
    Bin(BinOp, usize, usize),
    Unary(u8, usize),
    Clamp { input: usize, min: usize, max: usize },
    YGradient { from_y: f64, to_y: f64, from: f64, to: f64 },
    RangeChoice { input: usize, min: f64, max: f64, in_range: usize, out: usize },
    Noise { key: String, xz: f64, y: f64 },
    ShiftedNoise { noise: String, xz: f64, y: f64, shift_x: usize, shift_z: usize },
    ShiftA { key: String },
    ShiftB { key: String },
    Spline(SplineNode),
    WeirdScaled { input: usize, noise: String, mapper3d: bool },
    Blended,
}

struct SplineNode {
    coord: usize,
    points: Vec<(f32, f32, usize)>, // loc, der, value-node
}

fn unary_fn(t: u8, d: f64) -> f64 {
    match t {
        0 => d.abs(),
        1 => d * d,
        2 => d * d * d,
        3 => {
            if d > 0.0 {
                d
            } else {
                d * 0.5
            }
        }
        4 => {
            if d > 0.0 {
                d
            } else {
                d * 0.25
            }
        }
        5 => 1.0 / d,
        _ => {
            let c = d.clamp(-1.0, 1.0);
            c / 2.0 - c * c * c / 24.0
        }
    }
}

pub struct DensityRoot {
    arena: Vec<Node>,
    roots: HashMap<String, usize>,
    root: usize,
    bank: Arc<NoiseBank>,
}

pub struct Ctx {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

struct Parser {
    arena: Vec<Node>,
    roots: HashMap<String, usize>,
}

fn norm_ref(s: &str) -> String {
    s.strip_prefix("minecraft:").unwrap_or(s).to_string()
}

impl DensityRoot {
    /// 构建 + 全引用校验：解析后扫描整棵 arena，所有 Ref 键与 noise 键必须
    /// 存在，否则 Err（内嵌数据不完整属程序缺陷，命令层转中文错误返回，
    /// 不 panic 进 spawn_blocking）。
    pub fn build(root: Value, bank: Arc<NoiseBank>) -> Result<Self, String> {
        let mut p = Parser {
            arena: Vec::new(),
            roots: HashMap::new(),
        };
        for (name, v) in data().densities.iter() {
            let i = p.parse(v);
            p.roots.insert(name.clone(), i);
        }
        let r = p.parse(&root);
        // 校验引用可达性
        for (i, node) in p.arena.iter().enumerate() {
            if let Node::Ref(name) = node {
                if !p.roots.contains_key(name) {
                    return Err(format!(
                        "内嵌密度数据不完整\n引用缺失: {name}（节点 #{i}）"
                    ));
                }
            }
        }
        // 校验噪声键（构建期一次；求值期 bank.get 不再可能缺失）
        for (i, node) in p.arena.iter().enumerate() {
            let key = match node {
                Node::Noise { key, .. }
                | Node::ShiftedNoise { noise: key, .. }
                | Node::ShiftA { key }
                | Node::ShiftB { key }
                | Node::WeirdScaled { noise: key, .. } => Some(key.as_str()),
                _ => None,
            };
            if let Some(k) = key {
                if bank.get(k).is_none() {
                    return Err(format!("内嵌噪声规格不完整\n缺少噪声: {k}（节点 #{i}）"));
                }
            }
        }
        Ok(DensityRoot {
            arena: p.arena,
            roots: p.roots,
            root: r,
            bank,
        })
    }

    pub fn eval_root(&self, ctx: &Ctx) -> f64 {
        self.eval_node(self.root, ctx)
    }

    fn eval_node(&self, i: usize, ctx: &Ctx) -> f64 {
        match &self.arena[i] {
            Node::Lit(v) => *v,
            Node::Ref(name) => {
                let j = self.roots[name];
                self.eval_node(j, ctx)
            }
            Node::Bin(op, a, b) => {
                let av = self.eval_node(*a, ctx);
                let bv = self.eval_node(*b, ctx);
                match op {
                    BinOp::Add => av + bv,
                    BinOp::Mul => av * bv,
                    BinOp::Min => av.min(bv),
                    BinOp::Max => av.max(bv),
                }
            }
            Node::Unary(t, a) => unary_fn(*t, self.eval_node(*a, ctx)),
            Node::Clamp { input, min, max } => {
                let v = self.eval_node(*input, ctx);
                let lo = self.eval_node(*min, ctx);
                let hi = self.eval_node(*max, ctx);
                if v < lo {
                    lo
                } else if v > hi {
                    hi
                } else {
                    v
                }
            }
            Node::YGradient { from_y, to_y, from, to } => {
                let y = if ctx.y < *from_y {
                    *from_y
                } else if ctx.y > *to_y {
                    *to_y
                } else {
                    ctx.y
                };
                let t = (y - from_y) / (to_y - from_y);
                from + t * (to - from)
            }
            Node::RangeChoice { input, min, max, in_range, out } => {
                let v = self.eval_node(*input, ctx);
                if v >= *min && v < *max {
                    self.eval_node(*in_range, ctx)
                } else {
                    self.eval_node(*out, ctx)
                }
            }
            Node::Noise { key, xz, y } => {
                let n = self.bank.get(key).expect("noise 实例缺失");
                n.get_value(ctx.x * xz, ctx.y * y, ctx.z * xz)
            }
            Node::ShiftedNoise { noise, xz, y, shift_x, shift_z } => {
                let n = self.bank.get(noise).expect("shifted noise 缺失");
                let sx = self.eval_node(*shift_x, ctx);
                let sz = self.eval_node(*shift_z, ctx);
                n.get_value(ctx.x * xz + sx, ctx.y * y, ctx.z * xz + sz)
            }
            Node::ShiftA { key } => {
                let n = self.bank.get(key).expect("shift_a noise 缺失");
                4.0 * n.get_value(ctx.x * 0.25, 0.0, ctx.z * 0.25)
            }
            Node::ShiftB { key } => {
                let n = self.bank.get(key).expect("shift_b noise 缺失");
                4.0 * n.get_value(ctx.z * 0.25, 0.0, ctx.x * 0.25)
            }
            Node::Spline(sp) => self.eval_spline(sp, ctx),
            Node::WeirdScaled { input, noise, mapper3d } => {
                let v = self.eval_node(*input, ctx);
                let r = if *mapper3d {
                    if v < -0.5 {
                        0.75
                    } else if v < 0.0 {
                        1.0
                    } else if v < 0.5 {
                        1.5
                    } else {
                        2.0
                    }
                } else if v < -0.75 {
                    0.5
                } else if v < -0.5 {
                    0.75
                } else if v < 0.5 {
                    1.0
                } else if v < 0.75 {
                    2.0
                } else {
                    3.0
                };
                let n = self.bank.get(noise).expect("weird noise 缺失");
                (n.get_value(ctx.x / r, ctx.y / r, ctx.z / r).abs()) * r
            }
            Node::Blended => self.bank.blended.compute(ctx.x, ctx.y, ctx.z),
        }
    }

    fn eval_spline(&self, sp: &SplineNode, ctx: &Ctx) -> f64 {
        let f = self.eval_node(sp.coord, ctx) as f32;
        let n = sp.points.len();
        if n == 0 {
            return 0.0;
        }
        if n == 1 {
            let (_, _, vn) = sp.points[0];
            return self.eval_node(vn, ctx);
        }
        let mut loc = [0.0f32; 12];
        let mut der = [0.0f32; 12];
        let mut vf = [0.0f32; 12];
        debug_assert!(n <= 12);
        for (k, (l, d, vn)) in sp.points.iter().enumerate() {
            loc[k] = *l;
            der[k] = *d;
            vf[k] = self.eval_node(*vn, ctx) as f32;
        }
        spline_values(&loc[..n], &der[..n], &vf[..n], f)
    }
}

fn spline_values(loc: &[f32], der: &[f32], vf: &[f32], f: f32) -> f64 {
    let n = loc.len();
    let mut i = 0usize;
    while i < n && loc[i] < f {
        i += 1;
    }
    if i == 0 {
        return vf[0] as f64 + (der[0] * (f - loc[0])) as f64;
    }
    if i == n {
        return vf[n - 1] as f64 + (der[n - 1] * (f - loc[n - 1])) as f64;
    }
    let g = loc[i - 1];
    let h = loc[i];
    let k = (f - g) / (h - g);
    let l = der[i - 1];
    let m = der[i];
    let n0 = vf[i - 1];
    let o = vf[i];
    let p = l * (h - g) - (o - n0);
    let q = -m * (h - g) + (o - n0);
    let r = (n0 + k * (o - n0)) + k * (1.0 - k) * (p + k * (q - p));
    r as f64
}

impl Parser {
    fn parse(&mut self, v: &Value) -> usize {
        match v {
            Value::Number(num) => self.push(Node::Lit(num.as_f64().unwrap_or(0.0))),
            Value::String(s) => self.push(Node::Ref(norm_ref(s))),
            Value::Object(o) => self.parse_object(o),
            _ => self.push(Node::Lit(0.0)),
        }
    }

    fn push(&mut self, n: Node) -> usize {
        let i = self.arena.len();
        self.arena.push(n);
        i
    }

    fn parse_object(&mut self, o: &serde_json::Map<String, Value>) -> usize {
        // 样条内嵌对象（无 type，含 coordinate）直接按样条处理
        if o.contains_key("coordinate") && o.contains_key("points") {
            return self.parse_spline_obj(o);
        }
        let ty = o
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim_start_matches("minecraft:");
        let arg = |p: &mut Parser, key: &str| -> usize {
            p.parse(&o.get(key).cloned().unwrap_or(Value::Null))
        };
        match ty {
            "add" => {
                let a = arg(self, "argument1");
                let b = arg(self, "argument2");
                self.push(Node::Bin(BinOp::Add, a, b))
            }
            "mul" => {
                let a = arg(self, "argument1");
                let b = arg(self, "argument2");
                self.push(Node::Bin(BinOp::Mul, a, b))
            }
            "min" => {
                let a = arg(self, "argument1");
                let b = arg(self, "argument2");
                self.push(Node::Bin(BinOp::Min, a, b))
            }
            "max" => {
                let a = arg(self, "argument1");
                let b = arg(self, "argument2");
                self.push(Node::Bin(BinOp::Max, a, b))
            }
            "abs" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(0, a))
            }
            "square" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(1, a))
            }
            "cube" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(2, a))
            }
            "half_negative" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(3, a))
            }
            "quarter_negative" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(4, a))
            }
            "invert" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(5, a))
            }
            "squeeze" => {
                let a = arg(self, "argument");
                self.push(Node::Unary(6, a))
            }
            "interpolated" | "cache_once" | "cache_2d" | "flat_cache" | "blend_density" => {
                arg(self, "argument")
            }
            "blend_alpha" => self.push(Node::Lit(1.0)),
            "blend_offset" => self.push(Node::Lit(0.0)),
            "clamp" => {
                let input = arg(self, "input");
                let min = arg(self, "min");
                let max = arg(self, "max");
                self.push(Node::Clamp { input, min, max })
            }
            "y_clamped_gradient" => {
                let f64o = |k: &str| o.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
                self.push(Node::YGradient {
                    from_y: f64o("from_y"),
                    to_y: f64o("to_y"),
                    from: f64o("from_value"),
                    to: f64o("to_value"),
                })
            }
            "range_choice" => {
                let input = arg(self, "input");
                let min = o.get("min_inclusive").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let max = o.get("max_exclusive").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let in_range = arg(self, "when_in_range");
                let out = arg(self, "when_out_of_range");
                self.push(Node::RangeChoice { input, min, max, in_range, out })
            }
            "noise" => {
                let key = o.get("noise").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let xz = o.get("xz_scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let y = o.get("y_scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
                self.push(Node::Noise { key, xz, y })
            }
            "shifted_noise" => {
                let noise = o.get("noise").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let xz = o.get("xz_scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let y = o.get("y_scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let shift_x = arg(self, "shift_x");
                let shift_z = arg(self, "shift_z");
                self.push(Node::ShiftedNoise { noise, xz, y, shift_x, shift_z })
            }
            "shift_a" => {
                let key = o.get("argument").and_then(|v| v.as_str()).unwrap_or("").to_string();
                self.push(Node::ShiftA { key })
            }
            "shift_b" => {
                let key = o.get("argument").and_then(|v| v.as_str()).unwrap_or("").to_string();
                self.push(Node::ShiftB { key })
            }
            "spline" => {
                if let Some(sp_obj) = o.get("spline") {
                    return self.parse(sp_obj);
                }
                self.push(Node::Lit(0.0))
            }
            "weird_scaled_sampler" => {
                let input = arg(self, "input");
                let noise = o.get("noise").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mapper = o
                    .get("rarity_value_mapper")
                    .and_then(|v| v.as_str())
                    .unwrap_or("type_1");
                let mapper3d = mapper != "type_2";
                self.push(Node::WeirdScaled { input, noise, mapper3d })
            }
            "old_blended_noise" => self.push(Node::Blended),
            _ => {
                // 未知：argument 透传防静默归零
                if let Some(a) = o.get("argument") {
                    return self.parse(a);
                }
                self.push(Node::Lit(0.0))
            }
        }
    }

    fn parse_spline_obj(&mut self, o: &serde_json::Map<String, Value>) -> usize {
        let coord_val = o.get("coordinate").cloned().unwrap_or(Value::Null);
        let coord = self.parse(&coord_val);
        let mut points = Vec::new();
        if let Some(Value::Array(pts)) = o.get("points") {
            for p in pts {
                if let Value::Object(po) = p {
                    let loc = po.get("location").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let der = po.get("derivative").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let vv = po.get("value").cloned().unwrap_or(Value::Null);
                    let vn = self.parse(&vv);
                    points.push((loc, der, vn));
                }
            }
        }
        self.push(Node::Spline(SplineNode { coord, points }))
    }
}

#[cfg(test)]
mod tests {
    use super::unary_fn;

    /// Mapped 一元算子语义（26.1.2 字节码直读，索引见 parse_object 映射：
    /// abs/square/cube/half_negative/quarter_negative/invert/squeeze）。
    #[test]
    fn unary_semantics_match_vanilla() {
        assert_eq!(unary_fn(0, -3.5), 3.5);
        assert_eq!(unary_fn(1, -4.0), 16.0);
        assert_eq!(unary_fn(2, -2.0), -8.0);
        // half/quarter_negative：正数原样，负数减半/四分之一
        assert_eq!(unary_fn(3, 2.0), 2.0);
        assert_eq!(unary_fn(3, -2.0), -1.0);
        assert_eq!(unary_fn(4, -4.0), -1.0);
        assert_eq!(unary_fn(5, 4.0), 0.25);
        // squeeze: clamp(-1,1)/2 - t^3/24
        let v = unary_fn(6, 0.5);
        assert!((v - (0.5 / 2.0 - 0.125 / 24.0)).abs() < 1e-12);
        assert_eq!(unary_fn(6, 5.0), unary_fn(6, 1.0));
        assert_eq!(unary_fn(6, -5.0), unary_fn(6, -1.0));
    }
}
