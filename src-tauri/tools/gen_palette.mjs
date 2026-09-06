// 自然色生成器（throwaway，跑完保留在仓库供后续 MC 版本重生成）：
// 从原版客户端 jar 采样每方块"顶面贴图平均色"，产出 natural_block_colors.json。
// 用法: bun gen_palette.mjs <jar路径> <输出json>
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import os from "node:os";

const jar = process.argv[2];
const outJson = process.argv[3] || "src-tauri/assets/natural_block_colors.json";
const work = fs.mkdtempSync(path.join(os.tmpdir(), "palgen-"));

// 解压需要的资源目录
execSync(`unzip -q -o "${jar}" "assets/minecraft/blockstates/*" "assets/minecraft/models/block/*" "assets/minecraft/textures/block/*" -d ${work}`);

const ROOT = path.join(work, "assets", "minecraft");
const bsDir = path.join(ROOT, "blockstates");
const modelDir = path.join(ROOT, "models"); // 引用/父级形如 block/xxx（相对 models/ 根）
const texDir = path.join(ROOT, "textures"); // 引用形如 block/xxx，相对于 textures/ 根

// ── PNG 解码（8/4bit：0灰度/2RGB/3调色板/4灰α/6RGBA，含 tRNS）。
// 26.x 贴图常见 4-bit 调色板格式，必须支持位深 4。──
function decodePNG(buf) {
  let off = 8; const idat = []; let w = 0, h = 0, depth = 0, color = 0;
  let plte = null, trns = null;
  while (off < buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString("ascii", off + 4, off + 8);
    const data = buf.slice(off + 8, off + 8 + len);
    if (type === "IHDR") { w = data.readUInt32BE(0); h = data.readUInt32BE(4); depth = data[8]; color = data[9]; }
    else if (type === "IDAT") idat.push(data);
    else if (type === "PLTE") plte = data;
    else if (type === "tRNS") trns = data;
    off += 12 + len;
  }
  if (![1, 2, 4, 8].includes(depth)) throw new Error("unsupported depth " + depth);
  const channels = [1, 0, 3, 1, 2, 0, 4][color];
  if (!channels) throw new Error("unsupported color type " + color);
  const raw = zlib.inflateSync(Buffer.concat(idat));
  // 过滤在压缩字节流上做（4bit 时每字节两像素）
  const bpp = Math.max(1, Math.ceil(depth * channels / 8));
  const rawStride = Math.ceil(w * depth * channels / 8);
  const rows = [];
  let prev = Buffer.alloc(rawStride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (rawStride + 1)];
    const line = raw.slice(y * (rawStride + 1) + 1, (y + 1) * (rawStride + 1));
    const cur = Buffer.alloc(rawStride);
    for (let i = 0; i < rawStride; i++) {
      const a = i >= bpp ? cur[i - bpp] : 0;
      const bv = prev[i];
      const c = i >= bpp ? prev[i - bpp] : 0;
      let x = line[i];
      if (f === 1) x = (x + a) & 255;
      else if (f === 2) x = (x + bv) & 255;
      else if (f === 3) x = (x + ((a + bv) >> 1)) & 255;
      else if (f === 4) {
        const pa = Math.abs(bv - c), pb = Math.abs(a - c), pc = Math.abs(a + bv - 2 * c);
        const pr = pa <= pb && pa <= pc ? a : pb <= pc ? bv : c;
        x = (x + pr) & 255;
      }
      cur[i] = x;
    }
    rows.push(cur);
    prev = cur;
  }
  const tmp = Buffer.alloc(w * h * 4);
  for (let y = 0; y < h; y++) {
    const row = rows[y];
    for (let i = 0; i < w; i++) {
      const bitsPerPx = depth * channels;
      let shift;
      if (depth === 8) { shift = 0; }
      else { shift = 8 - bitsPerPx; }
      const byteOff = Math.floor(i * bitsPerPx / 8);
      let samples;
      if (depth === 8) {
        samples = [];
        for (let c = 0; c < channels; c++) samples.push(row[byteOff + c]);
      } else {
        let bits = i * bitsPerPx % 8 === 0;
        samples = [];
        for (let c = 0; c < channels; c++) {
          const bitPos = i * bitsPerPx + c * depth;
          const bo = Math.floor(bitPos / 8);
          const sh = 8 - depth - (bitPos % 8);
          samples.push((row[bo] >> sh) & ((1 << depth) - 1));
        }
      }
      const g = samples[0], d = (y * w + i) * 4;
      const grayScale = depth === 8 ? 1 : 255 / ((1 << depth) - 1);
      let r, g2, b2, a2;
      switch (color) {
        case 0: r = g2 = b2 = Math.round(g * grayScale); a2 = 255; break;
        case 2: r = samples[0]; g2 = samples[1]; b2 = samples[2]; a2 = 255; break;
        case 3: { const idx = g; const po = idx * 3;
          r = plte[po]; g2 = plte[po + 1]; b2 = plte[po + 2]; a2 = trns && idx < trns.length ? trns[idx] : 255; break; }
        case 4: r = g2 = b2 = Math.round(g * grayScale); a2 = Math.round(samples[1] * grayScale); break;
        case 6: r = samples[0]; g2 = samples[1]; b2 = samples[2]; a2 = samples[3]; break;
      }
      tmp[d] = r; tmp[d + 1] = g2; tmp[d + 2] = b2; tmp[d + 3] = a2;
    }
  }
  return { w, h, data: tmp };
}

// 纹理"感知色"（alpha>0 像素）：
// - 只采中心区域（跳过 1-2px 边缘——贴图边框常带相邻色渗入/暗轮廓，整图均值
//   偏暗偏脏，主流地图观感因此差很多）；16×16 采中心 12×12，更小贴图按比例收窄
// - 轻度亮度抬升（抵消平均化损失；Xaero/JM 观感更亮）
// - a = 中心区不透明占比决定透明度（全覆盖=255，洞越多越透，最少 24）
const cache = new Map();
function texMean(texPath) {
  if (cache.has(texPath)) return cache.get(texPath);
  const out = { r: 0, g: 0, b: 0, n: 0, alphaSum: 0, alphaN: 0 };
  try {
    const img = decodePNG(fs.readFileSync(path.join(texDir, texPath + ".png")));
    // 中心裁剪：每边留 25% 边缘（16px 贴图 → 中心 8px…留 4px 太少，用 12px：边 2px）
    const edge = Math.max(0, Math.floor(Math.min(img.w, img.h) * 0.125));
    const x0 = edge, x1 = img.w - edge, y0 = edge, y1 = img.h - edge;
    for (let y = y0; y < y1; y++) {
      for (let x = x0; x < x1; x++) {
        const o = (y * img.w + x) * 4;
        const a = img.data[o + 3];
        if (a > 0) { out.r += img.data[o]; out.g += img.data[o + 1]; out.b += img.data[o + 2]; out.n++; }
        out.alphaSum += a; out.alphaN++;
      }
    }
    if (out.n > 0) {
      const fullAlpha = out.alphaSum / out.alphaN; // 平均覆盖
      const LIFT = 1.09; // 感知亮度抬升（主流小地图比贴图均值亮一档）
      const clamp = v => Math.min(255, Math.round(v * LIFT));
      const v = [clamp(out.r / out.n), clamp(out.g / out.n), clamp(out.b / out.n), Math.max(24, Math.round(fullAlpha))];
      cache.set(texPath, v);
      return v;
    }
  } catch { /* 缺图/坏图 */ }
  cache.set(texPath, null);
  return null;
}

// ── 模型解析：收集某模型全部"朝上面纹理 + 任意面兜底纹理" ──
const modelCache = new Map(); // name -> {textures: Map, upTexs: string[], anyTexs: string[]}
function loadModel(name) {
  if (modelCache.has(name)) return modelCache.get(name);
  const entry = { upTexs: [], anyTexs: [] };
  modelCache.set(name, entry);
  let file = path.join(modelDir, name + ".json");
  if (!fs.existsSync(file)) return entry;
  let json;
  try { json = JSON.parse(fs.readFileSync(file, "utf8")); } catch { return entry; }
  const chain = [];
  let cur = json, guard = 0;
  while (cur && guard++ < 12) {
    chain.push(cur);
    if (!cur.parent) break;
    const p = path.join(modelDir, cur.parent.replace(/^minecraft:/, "") + ".json");
    if (!fs.existsSync(p)) break;
    cur = JSON.parse(fs.readFileSync(p, "utf8"));
  }
  // 纹理合并（子覆盖父）；值可能仍是 #引用，取用时递归到最终路径
  const tex = new Map();
  for (const m of chain) {
    if (m.textures) {
      for (const [k, v] of Object.entries(m.textures)) {
        // 26.x：值可为字符串或 { sprite: "block/x", … } 对象
        const s = typeof v === "string" ? v : v?.sprite;
        if (typeof s === "string") tex.set(k, s);
      }
    }
  }
  const resolve = ref => {
    let cur = String(ref).replace(/^#/, "");
    for (let g = 0; g < 8 && tex.has(cur); g++) {
      const v = tex.get(cur);
      if (!v.startsWith("#")) return v.replace(/^minecraft:/, "");
      cur = v.slice(1);
    }
    return null;
  };
  for (const m of chain) {
    if (!m.elements) continue;
    for (const el of m.elements) {
      const faces = el.faces || {};
      const pick = [];
      for (const dir of ["up"]) if (faces[dir] && faces[dir].texture) pick.push(faces[dir].texture);
      for (const dir of ["down", "north", "south", "east", "west"]) if (faces[dir] && faces[dir].texture) pick.push(faces[dir].texture);
      for (const ref of pick) {
        const resolved = resolve(ref.replace(/^#/, ""));
        if (!resolved) continue;
        const isUp = pick.indexOf(ref) === pick.indexOf(faces.up ? faces.up.texture : "") && faces.up && faces.up.texture === ref;
        (ref === (faces.up && faces.up.texture) ? entry.upTexs : entry.anyTexs).push(resolved);
        if (!faces.up) entry.anyTexs.push(resolved);
      }
    }
  }
  if (entry.anyTexs.length === 0 && json.particle) {
    const p2 = resolve(String(json.particle).replace(/^#/, "").replace(/^minecraft:/, ""));
    if (p2) entry.anyTexs.push(p2);
  }
  return entry;
}

// blockstates → 该方块引用的全部模型（variants + multipart）
function modelsFor(blockName) {
  const file = path.join(bsDir, blockName + ".json");
  if (!fs.existsSync(file)) return [];
  const seen = new Set(), out = [];
  let json;
  try { json = JSON.parse(fs.readFileSync(file, "utf8")); } catch { return []; }
  const add = m => {
    if (!m || seen.has(m)) return;
    seen.add(m);
    out.push(m);
  };
  if (json.variants) for (const v of Object.values(json.variants)) {
    if (Array.isArray(v)) v.forEach(x => add(x.model.replace(/^minecraft:/, "")));
    else add(v.model.replace(/^minecraft:/, ""));
  }
  if (json.multipart) for (const part of json.multipart) {
    const apply = part.apply;
    if (Array.isArray(apply)) apply.forEach(x => add(x.model.replace(/^minecraft:/, "")));
    else add(apply.model.replace(/^minecraft:/, ""));
  }
  return out;
}

// ── 主循环 ──
const palette = {};
let missingTex = 0, missingModel = 0, total = 0;
for (const f of fs.readdirSync(bsDir)) {
  if (!f.endsWith(".json")) continue;
  const block = f.slice(0, -5);
  total++;
  const models = modelsFor(block);
  if (models.length === 0) { missingModel++; continue; }
  const texSet = new Map(); // texPath -> weight(出现的面次数)
  for (const m of models) {
    const e = loadModel(m);
    // 有 up 面：只取 up 纹理；无 up 面：取任意面纹理
    const list = e.upTexs.length ? e.upTexs : e.anyTexs;
    for (const t of list) texSet.set(t, (texSet.get(t) || 0) + 1);
  }
  if (texSet.size === 0) { missingTex++; palette[`minecraft:${block}`] = [150, 150, 150, 255]; continue; }
  // 按权重聚合（多模型同纹理出现次数加权）
  let r = 0, g = 0, b = 0, wsum = 0, alphaMax = 0;
  let ok = false;
  for (const [t, w] of texSet) {
    const mean = texMean(t);
    if (!mean) continue;
    ok = true;
    r += mean[0] * w; g += mean[1] * w; b += mean[2] * w;
    wsum += w;
    alphaMax = Math.max(alphaMax, mean[3]);
  }
  if (!ok) { palette[`minecraft:${block}`] = [150, 150, 150, 255]; missingTex++; continue; }
  palette[`minecraft:${block}`] = [Math.round(r / wsum), Math.round(g / wsum), Math.round(b / wsum), alphaMax];
}

fs.mkdirSync(path.dirname(outJson), { recursive: true });
// 结构：{ "blocks": {ns:block → 平均色}, "textures": {纹理基名(去 block/ 前缀) → 平均色} }
// textures 供 Mod 模型引用原版纹理时按纹理名取色（模型纹理 ≠ 方块同名的情况）。
const textureMap = {};
for (const [texPath, mean] of cache) {
  if (!mean) continue;
  const base = texPath.replace(/^block\//, "");
  if (!textureMap[base]) textureMap[base] = mean.map(v => Math.round(v));
}
fs.writeFileSync(outJson, JSON.stringify({ blocks: palette, textures: textureMap }));
console.log(`blocks: ${total}, resolved: ${Object.keys(palette).length}, missingTex: ${missingTex}, missingModel: ${missingModel}, textures: ${Object.keys(textureMap).length}`);
if (process.env.DBG) {
  const grays = Object.entries(palette).filter(([, v]) => v[0] === 150 && v[1] === 150).map(([k]) => k);
  console.log("gray count:", grays.length, "sample:", grays.slice(0, 25).join(", "));
}
for (const k of ["minecraft:grass_block", "minecraft:oak_planks", "minecraft:stone", "minecraft:sand", "minecraft:oak_leaves", "minecraft:spruce_leaves", "minecraft:cherry_leaves", "minecraft:dirt", "minecraft:water"]) {
  if (palette[k]) console.log(k, JSON.stringify(palette[k]));
}
fs.rmSync(work, { recursive: true, force: true });
