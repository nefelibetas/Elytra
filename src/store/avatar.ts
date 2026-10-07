// 头像渲染：正版/第三方/离线统一「本地等距 3D 头部」（Cubatar 式 canvas 渲染：8×8×8 头盒 +
// 0.5 膨胀帽子层，yaw/pitch 正交投影 + painter 排序 + 六面明暗）；离线用户名无真实账号时回退像素脸。
// 模块级单例：状态在 avatarState，渲染函数直接导出，store/组件均可调用。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

/** 取 canvas 2D 上下文：极端 WebView/内存压力下可能为 null，调用方须兼底（避免非空断言崩溃）。 */
function ctx2d(c: HTMLCanvasElement): CanvasRenderingContext2D | null {
  return c.getContext("2d");
}

/** FNV-1a 哈希（与 Rust 侧 config/launcher/skin.rs hash_name 一致）。 */
function hashName(name: string): number {
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** 离线皮肤模型决策：哈希奇数 → Alex（slim）。 */
function randomIsAlex(name: string): boolean {
  return hashName(name) % 2 === 1;
}

export const avatarState = reactive({
  premiumAvatar: "",
  offlineAvatar: "",
  /** 第三方登录（皮肤站）3D 头像 */
  externalAvatar: "",
  avatarError: false,
});

// 模块级去重缓存：账号/用户名未变不重复请求（路由切换重挂载不重复请求、不闪占位）
let lastPremiumUuid = "";
let lastOfflineName = "";
let lastExternalKey = "";
let avatarTimer: number | undefined;

/** 正版：拉皮肤 PNG + 模型类型（一次 IPC），本地 3D 渲染头部。
 * force 用于主动刷新（上传皮肤后等）。 */
export async function renderPremiumAvatar(uuid: string, force = false) {
  if (!uuid) {
    avatarState.premiumAvatar = "";
    lastPremiumUuid = "";
    return;
  }
  // 账号未变 → 不重复请求（含进行中，避免并发竞态；失败时重置可重试）
  if (!force && lastPremiumUuid === uuid) {
    return;
  }
  lastPremiumUuid = uuid;
  try {
    const profile = await invoke<{ skin: string; model: string }>("fetch_account_skin_profile", { uuid });
    avatarState.premiumAvatar = await renderSkinHead3D(profile.skin).catch(() => "");
    avatarState.avatarError = false;
  } catch (e) {
    // 装饰性资源：静默降级（UI 回退占位图标），不弹 dialog——网络恢复后用户可点刷新重试
    console.error("正版头像加载失败:", e);
    avatarState.premiumAvatar = "";
    lastPremiumUuid = ""; // 失败允许下次重试
    avatarState.avatarError = true;
  }
}

/** 确定性 PRNG（mulberry32） */
function mulberry32(seed: number) {
  return function () {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("image load failed"));
    img.src = src;
  });
}

/** 离线头像：按用户名哈希确定性渲染 Steve/Alex 默认皮肤（与游戏内一致），失败回退像素脸。 */
export async function renderOfflineAvatar(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    avatarState.offlineAvatar = "";
    lastOfflineName = "";
    return;
  }
  if (lastOfflineName === trimmed) {
    return; // 用户名未变 → 不重复请求（含进行中）
  }
  lastOfflineName = trimmed;
  const head = await renderOfflineHead(trimmed);
  if (head) {
    avatarState.offlineAvatar = head;
  } else {
    lastOfflineName = ""; // 失败允许下次重试（如版本 jar 就绪后）
    avatarState.offlineAvatar = generatePixelFace(trimmed);
  }
}

// ── 离线头像缓存（用户名 → Steve/Alex 3D 头部 dataURL；切换回已有用户名秒出，不重复 IO）──
// 模块级 reactive：组件重建（路由切换等）不丢缓存，列表与主头像共用同一份。
export const offlineHeads = reactive<Record<string, string>>({});

/** 渲染离线用户名的 Steve/Alex 默认皮肤 3D 头部（哈希决定模型，与主头像/游戏内一致）。
 * 缓存按用户名复用（仅缓存成功结果，失败不缓存以便之后重试）；失败返回 ""（调用方回退）。 */
export async function renderOfflineHead(name: string): Promise<string> {
  const trimmed = name.trim();
  if (!trimmed) return "";
  const cached = offlineHeads[trimmed];
  if (cached !== undefined) return cached;
  try {
    const skinKind = randomIsAlex(trimmed) ? "alex" : "steve";
    const b64 = await invoke<string>("fetch_default_skin", { skin: skinKind });
    const head = await renderSkinHead3D(b64);
    offlineHeads[trimmed] = head;
    return head;
  } catch {
    return "";
  }
}

// ── 等距 3D 头部渲染（算法移植自 Cubatar IsometricRenderer 的头部部分）──
// 模型：8×8×8 voxel 头盒（中心 y=4）+ 帽子盒（每侧 +0.5 膨胀，双面渲染——透过透明像素
// 可见远侧内壁，与游戏内一致）；面按标准皮肤 UV 布局取子图，仿射变换贴到正交投影的
// 平行四边形上，painter 算法按面中心深度排序，六面按法线方向明暗（顶 1.0 / 前后 0.85 /
// 左右 0.7 / 底 0.55）。

interface V3 {
  x: number;
  y: number;
  z: number;
}

const v3 = (x: number, y: number, z: number): V3 => ({ x, y, z });

function rotY(v: V3, rad: number): V3 {
  const c = Math.cos(rad);
  const s = Math.sin(rad);
  return v3(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
}

function rotX(v: V3, rad: number): V3 {
  const c = Math.cos(rad);
  const s = Math.sin(rad);
  return v3(v.x, v.y * c - v.z * s, v.y * s + v.z * c);
}

type Side = "FRONT" | "BACK" | "LEFT" | "RIGHT" | "TOP" | "BOTTOM";

interface FaceSpec {
  normal: V3;
  uAxis: V3;
  vAxis: V3;
  shade: number;
}

const FACE_SPECS: Record<Side, FaceSpec> = {
  FRONT: { normal: v3(0, 0, 1), uAxis: v3(1, 0, 0), vAxis: v3(0, -1, 0), shade: 0.85 },
  BACK: { normal: v3(0, 0, -1), uAxis: v3(-1, 0, 0), vAxis: v3(0, -1, 0), shade: 0.85 },
  RIGHT: { normal: v3(1, 0, 0), uAxis: v3(0, 0, -1), vAxis: v3(0, -1, 0), shade: 0.7 },
  LEFT: { normal: v3(-1, 0, 0), uAxis: v3(0, 0, 1), vAxis: v3(0, -1, 0), shade: 0.7 },
  TOP: { normal: v3(0, 1, 0), uAxis: v3(1, 0, 0), vAxis: v3(0, 0, 1), shade: 1 },
  BOTTOM: { normal: v3(0, -1, 0), uAxis: v3(1, 0, 0), vAxis: v3(0, 0, 1), shade: 0.55 },
};

/** 头/帽子盒子：中心 (cx,cy,cz) voxel，尺寸 w/h/d，UV 原点 (uvX,uvY)，inflate 每侧膨胀。 */
interface HeadBox {
  cx: number;
  cy: number;
  cz: number;
  w: number;
  h: number;
  d: number;
  uvX: number;
  uvY: number;
  inflate: number;
  doubleSided: boolean;
  /** 限定渲染的面（默认全部）。披风等非立方体部件用它避免渲染无意义的薄边。 */
  onlySides?: Side[];
  /** 自定义按面 UV，覆盖标准皮肤布局。披风贴图是 10×16 展开图而非实体 UV，需要它。 */
  customUv?: (s: Side) => { x: number; y: number; w: number; h: number };
  /** 使用哪张贴图（默认 `skin`）。披风与皮肤是两张独立 PNG。 */
  tex?: TextureKey;
  /** 背面水平镜像。披风是单片双面 quad：MC 里从背面看到的是同一张贴图的背面，
   *  带 logo/文字的披风若不做镜像会「反字」。皮肤各部件的背面是标准 UV，不用此项。 */
  mirrorBack?: boolean;
}

/** 可用贴图源：皮肤（64×64）与披风（64×32，有效区 10×16）。 */
type TextureKey = "skin" | "cape";
type TextureSet = Partial<Record<TextureKey, HTMLImageElement>>;

/** 切向半尺寸（沿 uAxis），含膨胀。 */
function halfU(b: HeadBox, s: Side): number {
  return ((s === "FRONT" || s === "BACK" || s === "TOP" || s === "BOTTOM") ? b.w : b.d) / 2 + b.inflate;
}

/** 切向半尺寸（沿 vAxis），含膨胀。 */
function halfV(b: HeadBox, s: Side): number {
  return ((s === "TOP" || s === "BOTTOM") ? b.d : b.h) / 2 + b.inflate;
}

/** 法线方向半尺寸，含膨胀。 */
function halfN(b: HeadBox, s: Side): number {
  return ((s === "FRONT" || s === "BACK") ? b.d : s === "LEFT" || s === "RIGHT" ? b.w : b.h) / 2 + b.inflate;
}

/** 标准皮肤 UV 布局（voxel 坐标）：LEFT(d) FRONT(w) RIGHT(d) BACK(w)，TOP 在 FRONT 上，BOTTOM 在 RIGHT 上。 */
function faceUv(b: HeadBox, s: Side): { x: number; y: number; w: number; h: number } {
  const u = b.uvX;
  const v = b.uvY;
  const w = b.w;
  const h = b.h;
  const d = b.d;
  switch (s) {
    case "FRONT":
      return { x: u + d, y: v + d, w, h };
    case "BACK":
      return { x: u + d + w + d, y: v + d, w, h };
    case "RIGHT":
      return { x: u + d + w, y: v + d, w: d, h };
    case "LEFT":
      return { x: u, y: v + d, w: d, h };
    case "TOP":
      return { x: u + d, y: v, w, h: d };
    case "BOTTOM":
      return { x: u + d + w, y: v, w, h: d };
  }
}

/** 面角点在 yaw/pitch 旋转 + 正交投影下的坐标（au/bv ∈ {-1,+1} 为面内参数坐标）。
 * 返回未偏移的投影点；画布偏移由调用方统一计算。 */
function projectCorner(
  b: HeadBox,
  s: Side,
  au: number,
  bv: number,
  yaw: number,
  pitch: number,
  ppv: number,
): V3 {
  const f = FACE_SPECS[s];
  const local = v3(
    b.cx + f.normal.x * halfN(b, s) + f.uAxis.x * au * halfU(b, s) + f.vAxis.x * bv * halfV(b, s),
    b.cy + f.normal.y * halfN(b, s) + f.uAxis.y * au * halfU(b, s) + f.vAxis.y * bv * halfV(b, s),
    b.cz + f.normal.z * halfN(b, s) + f.uAxis.z * au * halfU(b, s) + f.vAxis.z * bv * halfV(b, s),
  );
  const r = rotX(rotY(local, yaw), pitch);
  return v3(r.x * ppv, -r.y * ppv, r.z);
}

/** 面切片缓存：WeakMap<贴图, Map<「UV 区 + 明暗」, 切片画布>>。
 *  切片只取决于「哪张图的哪个 UV 区域、按哪个面的明暗」——与 yaw/pitch 无关，
 *  故可跨帧、跨视角复用。拖动旋转时若不缓存，每帧要重切 14 部件 × 6 面 ≈ 84 张，
 *  实测慢到不可用（5 帧超 30s）。 */
const faceCache = new WeakMap<HTMLImageElement, Map<string, HTMLCanvasElement | null>>();

/** 面纹理：裁 UV 子图 + 按面明暗（跳过透明像素）。UV 越界返回 null。结果按图缓存。 */
function faceTexture(img: HTMLImageElement, scale: number, b: HeadBox, s: Side): HTMLCanvasElement | null {
  const uv = b.customUv ? b.customUv(s) : faceUv(b, s);
  let per = faceCache.get(img);
  if (!per) {
    per = new Map();
    faceCache.set(img, per);
  }
  // scale 由图片宽度决定（每张图固定），无需进 key
  const key = `${uv.x},${uv.y},${uv.w},${uv.h},${FACE_SPECS[s].shade},${b.mirrorBack && s === "BACK" ? "m" : ""}`;
  const hit = per.get(key);
  if (hit !== undefined) return hit;
  const sx = Math.round(uv.x * scale);
  const sy = Math.round(uv.y * scale);
  const sw = Math.round(uv.w * scale);
  const sh = Math.round(uv.h * scale);
  if (sx + sw > img.width || sy + sh > img.height) {
    per.set(key, null); // 越界也缓存，避免每帧重复判定
    return null;
  }
  const crop = document.createElement("canvas");
  crop.width = sw;
  crop.height = sh;
  const c = ctx2d(crop);
  if (!c) return null;
  c.imageSmoothingEnabled = false;
  if (b.mirrorBack && s === "BACK") {
    c.translate(sw, 0);
    c.scale(-1, 1); // 水平镜像：背面看到的是同一张贴图的背面
  }
  c.drawImage(img, sx, sy, sw, sh, 0, 0, sw, sh);
  const shade = FACE_SPECS[s].shade;
  if (shade >= 1) {
    per.set(key, crop);
    return crop;
  }
  const data = c.getImageData(0, 0, sw, sh);
  const px = data.data;
  for (let i = 0; i < px.length; i += 4) {
    if (px[i + 3] === 0) continue;
    px[i] = (px[i] * shade) | 0;
    px[i + 1] = (px[i + 1] * shade) | 0;
    px[i + 2] = (px[i + 2] * shade) | 0;
  }
  c.putImageData(data, 0, 0);
  per.set(key, crop);
  return crop;
}

/** 帽子层是否可见：帽子 UV 区（x 32..64 / y 0..16 @64px 布局）存在非透明像素。 */
function hatHasPixels(img: HTMLImageElement, scale: number): boolean {
  const x0 = Math.round(32 * scale);
  const x1 = Math.min(Math.round(64 * scale), img.width);
  const y1 = Math.min(Math.round(16 * scale), img.height);
  const tmp = document.createElement("canvas");
  tmp.width = x1 - x0;
  tmp.height = y1;
  const c = ctx2d(tmp);
  if (!c) return false; // 读不到像素：按无帽子层处理（保守，仅影响外观降级）
  c.drawImage(img, x0, 0, x1 - x0, y1, 0, 0, x1 - x0, y1);
  const data = c.getImageData(0, 0, x1 - x0, y1).data;
  for (let i = 3; i < data.length; i += 4) {
    if (data[i] > 0) return true;
  }
    return false;
}

/** 通用盒子渲染核心：box 列表 → yaw/pitch 正交投影 → painter 排序 → 贴合画布输出。
 *  头部头像与全身预览共用（六面明暗、仿射贴图、深度排序均不含部位假设）。
 *
 *  `viewport` 为空时按可见面角点 min/max 贴合裁切（头像用，画布无空白）；
 *  传入时改用固定画布并把内容居中——拖动旋转时包围盒每帧都变，
 *  贴合裁切会让画布尺寸跟着跳（图像忽大忽小），故全身预览必须固定视口。 */
function renderBoxes(
  tex: TextureSet,
  boxes: HeadBox[],
  yawDeg: number,
  pitchDeg: number,
  ppv: number,
  viewport?: { w: number; h: number },
): string {
  const yaw = (yawDeg * Math.PI) / 180;
  const pitch = (pitchDeg * Math.PI) / 180;

  // 收集可见面 quad（投影未偏移）
  const quads: {
    tex: HTMLCanvasElement;
    p00: V3;
    p10: V3;
    p01: V3;
    p11: V3;
    depth: number;
  }[] = [];
  for (const b of boxes) {
    const img = tex[b.tex ?? "skin"];
    if (!img) continue; // 该贴图源缺失（如未提供披风）→ 跳过该部件
    const scale = img.width / 64;
    const sides = b.onlySides ?? (Object.keys(FACE_SPECS) as Side[]);
    for (const s of sides) {
      const nz = rotX(rotY(FACE_SPECS[s].normal, yaw), pitch).z;
      if (Math.abs(nz) < 1e-6) continue; // 棱边朝相机：退化面
      if (!b.doubleSided && nz < 0) continue; // 单面：背面不画
      const q = faceTexture(img, scale, b, s);
      if (!q) continue;
      const p00 = projectCorner(b, s, -1, -1, yaw, pitch, ppv);
      const p10 = projectCorner(b, s, 1, -1, yaw, pitch, ppv);
      const p01 = projectCorner(b, s, -1, 1, yaw, pitch, ppv);
      const p11 = projectCorner(b, s, 1, 1, yaw, pitch, ppv);
      quads.push({ tex: q, p00, p10, p01, p11, depth: (p00.z + p10.z + p01.z + p11.z) / 4 });
    }
  }
  if (quads.length === 0) return "";

  let ox: number;
  let oy: number;
  let cw: number;
  let ch: number;
  if (viewport) {
    cw = viewport.w;
    ch = viewport.h;
    // 内容包围盒居中：模型水平中轴 cx=0、脚底 y=0、身高 32 voxel
    const spanY = 32 * ppv;
    ox = cw / 2;
    oy = ch / 2 + spanY / 2;
  } else {
    // 画布包围盒：可见面角点的 min/max（贴合内容，无空白）
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    for (const q of quads) {
      for (const p of [q.p00, q.p10, q.p01, q.p11]) {
        minX = Math.min(minX, p.x);
        maxX = Math.max(maxX, p.x);
        minY = Math.min(minY, p.y);
        maxY = Math.max(maxY, p.y);
      }
    }
    cw = Math.ceil(maxX - minX) + 2;
    ch = Math.ceil(maxY - minY) + 2;
    ox = 1 - minX;
    oy = 1 - minY;
  }
  const out = document.createElement("canvas");
  out.width = cw;
  out.height = ch;
  const ctx = ctx2d(out);
  if (!ctx) return ""; // 拿不到上下文返回空串，调用方按无头像降级
  ctx.imageSmoothingEnabled = false;

  quads.sort((a, bq) => a.depth - bq.depth); // painter：远的先画
  for (const q of quads) {
    // 仿射矩阵（与 Java AffineTransform 同序）：纹理 (0,0)→p00，(w,0)→p10，(0,h)→p01
    const m00 = (q.p10.x - q.p00.x) / q.tex.width;
    const m10 = (q.p10.y - q.p00.y) / q.tex.width;
    const m01 = (q.p01.x - q.p00.x) / q.tex.height;
    const m11 = (q.p01.y - q.p00.y) / q.tex.height;
    ctx.setTransform(m00, m10, m01, m11, q.p00.x + ox, q.p00.y + oy);
    ctx.drawImage(q.tex, 0, 0);
  }
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  return out.toDataURL("image/png");
}

/** 头部 3D 渲染（Cubatar IsometricRenderer 移植）：8×8×8 头盒 + 0.5 膨胀帽子层（双面），
 * yaw/pitch 正交投影 + painter 排序 + 六面明暗。默认完全正脸视角 yaw=0° pitch=0°
 *（仅正面可见，无俯视）；legacy 64×32 皮肤无独立帽子层，自动跳过。
 * 画布按可见面角点贴合（透明背景，头 8 voxel = 64px），容器用 object-fit 等比适配。 */
export async function renderSkinHead3D(b64: string, yawDeg = 0, pitchDeg = 0): Promise<string> {
  const img = await loadImage(`data:image/png;base64,${b64}`);
  const scale = img.width / 64;
  const head: HeadBox = { cx: 0, cy: 4, cz: 0, w: 8, h: 8, d: 8, uvX: 0, uvY: 0, inflate: 0, doubleSided: false };
  const boxes: HeadBox[] = [head];
  if (img.height !== 32 && hatHasPixels(img, scale)) {
    boxes.push({ cx: 0, cy: 4, cz: 0, w: 8, h: 8, d: 8, uvX: 32, uvY: 0, inflate: 0.5, doubleSided: true });
  }
  return renderBoxes({ skin: img }, boxes, yawDeg, pitchDeg, 8);
}

/** 标准皮肤（1.8+ 64×64）各部位盒：静止站姿，脚底 y=0，向上为 +y（单位 = voxel）。
 *  UV 原点按 MC 标准布局（v=16 起）。第二层 inflate 0.25（帽子 0.5）+ 双面渲染——
 *  透过透明像素可见远侧内壁，与游戏内一致。legacy 64×32 旧皮肤没有第二层，
 *  且左臂/左腿复用右臂/右腿的 UV。 */
function bodyBoxes(slim: boolean, legacy: boolean): HeadBox[] {
  const aw = slim ? 3 : 4; // 纤细手臂 3px（位图仍按 4px 预留，UV 布局不变）
  const boxes: HeadBox[] = [
    // ── 第一层（本体）──
    { cx: 0, cy: 28, cz: 0, w: 8, h: 8, d: 8, uvX: 0, uvY: 0, inflate: 0, doubleSided: false }, // 头
    { cx: 0, cy: 18, cz: 0, w: 8, h: 12, d: 4, uvX: 16, uvY: 16, inflate: 0, doubleSided: false }, // 躯干
    { cx: -6, cy: 18, cz: 0, w: aw, h: 12, d: 4, uvX: 40, uvY: 16, inflate: 0, doubleSided: false }, // 右臂
    { cx: 6, cy: 18, cz: 0, w: aw, h: 12, d: 4, uvX: legacy ? 40 : 32, uvY: 16, inflate: 0, doubleSided: false }, // 左臂
    { cx: -2, cy: 6, cz: 0, w: 4, h: 12, d: 4, uvX: 0, uvY: 16, inflate: 0, doubleSided: false }, // 右腿
    { cx: 2, cy: 6, cz: 0, w: 4, h: 12, d: 4, uvX: legacy ? 0 : 16, uvY: legacy ? 16 : 48, inflate: 0, doubleSided: false }, // 左腿
  ];
  if (!legacy) {
    // ── 第二层（帽子 / 外套 / 袖 / 裤腿）──
    boxes.push(
      { cx: 0, cy: 28, cz: 0, w: 8, h: 8, d: 8, uvX: 32, uvY: 0, inflate: 0.5, doubleSided: true }, // 帽子层
      { cx: 0, cy: 18, cz: 0, w: 8, h: 12, d: 4, uvX: 16, uvY: 32, inflate: 0.25, doubleSided: true }, // 外套层
      { cx: -6, cy: 18, cz: 0, w: aw, h: 12, d: 4, uvX: 40, uvY: 32, inflate: 0.25, doubleSided: true }, // 右臂层
      { cx: 6, cy: 18, cz: 0, w: aw, h: 12, d: 4, uvX: 48, uvY: 48, inflate: 0.25, doubleSided: true }, // 左臂层
      { cx: -2, cy: 6, cz: 0, w: 4, h: 12, d: 4, uvX: 0, uvY: 32, inflate: 0.25, doubleSided: true }, // 右腿层
      { cx: 2, cy: 6, cz: 0, w: 4, h: 12, d: 4, uvX: 0, uvY: 48, inflate: 0.25, doubleSided: true }, // 左腿层
    );
  }
  return boxes;
}

/** 披风盒：10×16×1 薄板贴背（`cy=16` 自肩部向下，`cz` 在躯干后半深之外）。
 *  披风贴图是 10×16 展开图而非 MC 实体 UV 布局，故用 `customUv`：
 *  正/背面取整张，四个薄边取相邻 1px 拉伸，避免边缘出现空白像素。 */
function capeBox(): HeadBox {
  return {
    cx: 0,
    cy: 16,
    cz: -2.5,
    w: 10,
    h: 16,
    d: 1,
    uvX: 0,
    uvY: 0,
    inflate: 0,
    doubleSided: true,
    tex: "cape",
    mirrorBack: true,
    customUv: (s) => {
      switch (s) {
        case "FRONT":
        case "BACK":
          return { x: 0, y: 0, w: 10, h: 16 };
        case "LEFT":
          return { x: 0, y: 0, w: 1, h: 16 };
        case "RIGHT":
          return { x: 9, y: 0, w: 1, h: 16 };
        case "TOP":
          return { x: 0, y: 0, w: 10, h: 1 };
        case "BOTTOM":
          return { x: 0, y: 15, w: 10, h: 1 };
      }
    },
  };
}

/** 全身 3D 渲染（静止站姿，脚底在下；yaw/pitch 供拖动旋转）。`slim` = 纤细手臂模型。
 *  legacy（64×32）皮肤与头部渲染同规则自动降级：无第二层，左臂/左腿复用右侧 UV。
 *  传入 `capeB64` 时在背后挂上披风（同一套投影与 painter 排序，随身体一起旋转）。
 *
 *  用**固定视口**（宽 20 / 高 34 voxel，模型居中）：贴合裁切会让画布随 yaw 每帧变化，
 *  拖动时图像会忽大忽小；固定尺寸后由容器统一缩放，旋转过程稳定。
 *  高度 34 = 身高 32 + 上下各 1 voxel 余量；宽度取最宽姿态（躯干 8 + 双臂 4×2 = 16）留余量。
 */
export async function renderSkinFull3D(
  b64: string,
  slim = false,
  yawDeg = 0,
  pitchDeg = 0,
  ppv = 6,
  capeB64?: string,
): Promise<string> {
  const skin = await loadImage(`data:image/png;base64,${b64}`);
  const boxes = bodyBoxes(slim, skin.height === 32);
  const tex: TextureSet = { skin };
  if (capeB64) {
    tex.cape = await loadImage(`data:image/png;base64,${capeB64}`);
    boxes.push(capeBox());
  }
  const viewport = { w: Math.ceil(20 * ppv), h: Math.ceil(34 * ppv) };
  return renderBoxes(tex, boxes, yawDeg, pitchDeg, ppv, viewport);
}

/** 皮肤全身预览的默认观察视角（度）：3/4 正面（30°，能看到脸又有一点立体感）
 *  + 15° 微俯（能看到一点头顶）。全身预览与衣柜缩略图共用，保证两处视角一致。 */
export const DEFAULT_VIEW_YAW = 30;
export const DEFAULT_VIEW_PITCH = 15;

/** 动画帧缓存上限（超出按插入顺序淘汰最旧）。 */
const SKIN_THUMB_CACHE_MAX = 64;
/** 衣柜 3D 缩略图缓存：key = `S|slim|披风|皮肤base64` → dataURL。
 *  衣柜网格一屏十几张卡，按需重渲会卡；贴图 base64 不变即同一张图，可直接复用。 */
const skinThumbCache = new Map<string, string>();

/** 衣柜卡片的 3D 缩略图：与左侧预览同一视角（[DEFAULT_VIEW_YAW]/[DEFAULT_VIEW_PITCH]，
 *  3/4 正面 + 微俯 → 看得到脸与一点头顶），固定 ppv=3（60×102）。
 *  传 `capeB64` 时连同披风一起渲染（与预览同一套渲染参数；正面看不到披风，转到侧面才有）。
 *  结果按「皮肤 base64 + 模型 + 披风 base64」模块级缓存，列表滚动 / 重进页面不重渲；
 *  渲染失败由调用方回落到平面皮肤图。 */
export async function renderSkinThumb3D(b64: string, slim = false, capeB64?: string): Promise<string> {
  if (!b64) return "";
  const key = `S|${slim ? 1 : 0}|${capeB64 || ""}|${b64}`;
  const hit = skinThumbCache.get(key);
  if (hit !== undefined) return hit;
  const url = await renderSkinFull3D(b64, slim, DEFAULT_VIEW_YAW, DEFAULT_VIEW_PITCH, 3, capeB64);
  if (skinThumbCache.size >= SKIN_THUMB_CACHE_MAX) {
    const oldest = skinThumbCache.keys().next().value;
    if (oldest !== undefined) skinThumbCache.delete(oldest);
  }
  skinThumbCache.set(key, url);
  return url;
}

/** 第三方登录（皮肤站）头像：拉 skin + 模型 → 本地 3D 渲染。 */
export async function renderExternalAvatar(apiUrl: string, uuid: string, force = false) {
  const key = `${apiUrl}|${uuid}`;
  if (!uuid || !apiUrl) {
    avatarState.externalAvatar = "";
    lastExternalKey = "";
    return;
  }
  if (!force && lastExternalKey === key) return;
  lastExternalKey = key;
  try {
    const profile = await invoke<{ skin: string; model: string }>("fetch_external_skin_profile", { apiUrl, uuid });
    avatarState.externalAvatar = await renderSkinHead3D(profile.skin).catch(() => "");
    avatarState.avatarError = false;
  } catch (e) {
    console.error("第三方头像加载失败:", e);
    avatarState.externalAvatar = "";
    lastExternalKey = "";
  }
}

/** 按用户名生成 Minecraft 风格 8×8 像素脸（确定性，左右镜像）——离线用户名非真实账号时的回退。
 * 独立纯函数：离线主头像兜底与历史列表行头像共用（同用户名同脸）。返回 dataURL。 */
export function generatePixelFace(name: string): string {
  const trimmed = name.trim();
  const seed = hashName(trimmed);
  const rng = mulberry32(seed);
  const skinTones = ["#c68642", "#b5763a", "#a0662f", "#d1a579", "#e0ac69", "#8d5524", "#e8b57e"];
  const hairColors = ["#2a2222", "#3b2f2f", "#4a3728", "#1f1f1f", "#5c4033", "#6b4a2a"];

  const base = skinTones[seed % skinTones.length];
  const hair = hairColors[(seed >> 3) % hairColors.length];
  const eye = "#2f2f2f";

  // 8×8 像素脸
  const canvas = document.createElement("canvas");
  canvas.width = 8;
  canvas.height = 8;
  const ctx = ctx2d(canvas);
  if (!ctx) return ""; // 拿不到上下文返回空串，调用方按无头像降级

  // 皮肤底色
  ctx.fillStyle = base;
  ctx.fillRect(0, 0, 8, 8);
  // 头发：顶部两行，左半随机 + 镜像
  ctx.fillStyle = hair;
  for (let x = 0; x < 4; x++) {
    for (let y = 0; y < 2; y++) {
      if (rng() > 0.35) {
        ctx.fillRect(x, y, 1, 1);
        ctx.fillRect(7 - x, y, 1, 1);
      }
    }
  }
  // 眼睛
  ctx.fillStyle = eye;
  ctx.fillRect(2, 3, 1, 1);
  ctx.fillRect(5, 3, 1, 1);
  // 嘴
  ctx.fillRect(3, 6, 1, 1);
  ctx.fillRect(4, 6, 1, 1);

  // 放大到 96×96（匹配 96px 头像容器）
  const out = document.createElement("canvas");
  out.width = 96;
  out.height = 96;
  const octx = ctx2d(out);
  if (!octx) return ""; // 同上：拿不到上下文返回空串
  octx.drawImage(canvas, 0, 0, 96, 96);
  return out.toDataURL("image/png");
}

/** 用户名输入防抖 1.337s 后刷新离线头像。 */
export function scheduleOfflineRender(name: string) {
  avatarState.avatarError = false;
  if (avatarTimer) clearTimeout(avatarTimer);
  avatarTimer = window.setTimeout(() => void renderOfflineAvatar(name), 1337);
}

// ── 账号列表 3D 头像（sessionserver 皮肤头部渲染，按 uuid 模块级缓存）──

/** 正版账号 3D 头部缓存（uuid → dataURL；失败也占位空串防重复请求）。 */
export const accountHeads = reactive<Record<string, string>>({});

/** 拉取正版账号皮肤并 3D 渲染头部，返回 dataURL；失败（无网络/未设置皮肤）返回 ""（调用方回退图标）。 */
export async function renderAccountHead(uuid: string): Promise<string> {
  const cached = accountHeads[uuid];
  if (cached !== undefined) return cached;
  try {
    const b64 = await invoke<string>("fetch_account_skin", { uuid });
    const head = await renderSkinHead3D(b64);
    accountHeads[uuid] = head;
    return head;
  } catch {
    accountHeads[uuid] = "";
    return "";
  }
}
