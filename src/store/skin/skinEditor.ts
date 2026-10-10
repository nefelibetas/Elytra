// 皮肤绘画器 store（模块级 reactive 单例，项目约定无 Pinia）。
//
// 像素缓冲**刻意不走响应式**：64×64×4 = 16KB 的 Uint8ClampedArray 一旦被 Vue 深度代理，
// 每次读写都要过 Proxy，画笔拖动会明显掉帧。约定：像素存普通模块变量，视图只订阅
// `revision` 计数，写入端改完像素后调 `bumpRevision()`（组件侧再用 rAF 合并重绘）。

import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { loadSkinImage } from "../skin/skinRender";

/** 导出 PNG 到指定路径（IPC：写文件在后端完成）。 */
export async function exportSkinPng(path: string, b64: string): Promise<void> {
  await invoke("export_skin_png", { path, b64 });
}

/** 从文件读取 PNG，返回 base64（IPC：读文件在后端完成）。 */
export async function importSkinPng(path: string): Promise<string> {
  return invoke<string>("import_skin_png", { path });
}

/** 画布边长（Mojang 公开布局）。 */
export const SKIN_SIZE = 64;

/** 编辑工具（hand = 抓手，只用于平移画布，不改像素）。 */
export type EditorTool = "brush" | "eraser" | "fill" | "picker" | "hand";

/** 皮肤模型（取值与后端 variant、官方 API 一致）。 */
export type EditorVariant = "CLASSIC" | "SLIM";

/** 画布内的部位矩形（UV 原点 + 展开区尺寸，与 skinRender 的 SKIN_UV 同源）。 */
export interface EditorRegion {
  key: string;
  label: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** 标注色：内层深、第二层浅（同一部位的两层同色系，一眼看出对应关系）。 */
  color: string;
  /** 0 = 第一层（本体），1 = 第二层（帽子/外套/袖腿）。预览底纹只垫第一层。 */
  layer: 0 | 1;
}

/**
 * 皮肤 64×64 的部位标注。矩形是该立方体在展开图上的**取样区**（宽 2(d+w)、高 d+h），
 * 十字形之外的四个角是空像素（不属于任何面），故矩形只作定位参考、不做像素归属校验。
 *
 * 每块一个颜色：十几个矩形同色时，相邻两块的分界线根本认不出属于哪个部位；
 * 第二层（帽子/外套/袖子/裤腿）取本体色系的浅色，内层外层一眼可分。
 */
export const EDITOR_REGIONS: readonly EditorRegion[] = [
  { key: "head", label: "头", x: 0, y: 0, w: 32, h: 16, color: "#f59e0b", layer: 0 },
  { key: "hat", label: "帽子层", x: 32, y: 0, w: 32, h: 16, color: "#fcd34d", layer: 1 },
  { key: "rightLeg", label: "右腿", x: 0, y: 16, w: 16, h: 16, color: "#ef4444", layer: 0 },
  { key: "body", label: "身体", x: 16, y: 16, w: 24, h: 16, color: "#4f8ef7", layer: 0 },
  { key: "rightArm", label: "右臂", x: 40, y: 16, w: 16, h: 16, color: "#10b981", layer: 0 },
  { key: "rightPants", label: "右裤层", x: 0, y: 32, w: 16, h: 16, color: "#fca5a5", layer: 1 },
  { key: "jacket", label: "外套层", x: 16, y: 32, w: 24, h: 16, color: "#93c5fd", layer: 1 },
  { key: "rightSleeve", label: "右袖层", x: 40, y: 32, w: 16, h: 16, color: "#6ee7b7", layer: 1 },
  { key: "leftPants", label: "左裤层", x: 0, y: 48, w: 16, h: 16, color: "#f9a8d4", layer: 1 },
  { key: "leftLeg", label: "左腿", x: 16, y: 48, w: 16, h: 16, color: "#ec4899", layer: 0 },
  { key: "leftArm", label: "左臂", x: 32, y: 48, w: 16, h: 16, color: "#8b5cf6", layer: 0 },
  { key: "leftSleeve", label: "左袖层", x: 48, y: 48, w: 16, h: 16, color: "#c4b5fd", layer: 1 },
];

/** 撤销栈上限（每帧快照 16KB，30 步约 480KB）。 */
const UNDO_LIMIT = 30;

/** 像素缓冲（非响应式，见文件头）。null = 尚未载入。 */
let pixels: Uint8ClampedArray | null = null;
/** 撤销 / 重做栈：整帧快照（这个量级下差分不值得）。 */
let undoStack: Uint8ClampedArray[] = [];
let redoStack: Uint8ClampedArray[] = [];
/** 笔画起点快照（pointerdown 取，pointerup 时无变化则丢弃，避免占栈）。 */
let strokeBase: Uint8ClampedArray | null = null;

export const skinEditor = reactive({
  /** 像素版本号：任何像素改动 +1，视图据此（合并后）重绘。 */
  revision: 0,
  /** 是否已有可编辑画布。 */
  ready: false,
  tool: "brush" as EditorTool,
  /** 当前颜色（#rrggbb）。 */
  color: "#4f8ef7",
  /** 当前颜色的不透明度（0-255）。 */
  alpha: 255,
  /** 笔刷边长（像素）：1 = 单像素点，2/3 = 方块笔刷（大面铺色用）。 */
  size: 1,
  showGrid: true,
  showRegions: true,
  /** 导出时使用的文件名（不含扩展名）。 */
  name: "",
  variant: "CLASSIC" as EditorVariant,
  /** 导入图片的原始高度（0 = 空白画布）：< 64 说明是旧版 64×32 皮肤，渲染要按 legacy 走。 */
  sourceHeight: 0,
  /** 撤销 / 重做可用深度（视图据此禁用按钮）。 */
  undoDepth: 0,
  redoDepth: 0,
  /** 当前指针所在部位的 key（空串 = 不在任何标注区内），画布据此高亮该块。 */
  hoverKey: "",
});

/** 像素缓冲（活引用，写后需 `bumpRevision`）。 */
export function editorPixels(): Uint8ClampedArray | null {
  return pixels;
}

/** 通知视图像素已变。 */
export function bumpRevision() {
  skinEditor.revision++;
}

function syncDepths() {
  skinEditor.undoDepth = undoStack.length;
  skinEditor.redoDepth = redoStack.length;
}

/** 重置所有编辑态（载入 / 新建共用）。 */
function resetState() {
  undoStack = [];
  redoStack = [];
  strokeBase = null;
  skinEditor.ready = true;
  syncDepths();
  bumpRevision();
}

/** 新建空白画布（全透明 64×64）。 */
export function newEditorCanvas() {
  pixels = new Uint8ClampedArray(SKIN_SIZE * SKIN_SIZE * 4);
  skinEditor.name = "新皮肤";
  skinEditor.sourceHeight = 0;
  resetState();
}

/** 用图片 base64 载入画布（旧版 64×32 皮肤下半留透明）。 */
export async function loadImageIntoEditor(
  b64: string,
  name: string,
  variant: EditorVariant = skinEditor.variant,
): Promise<void> {
  const img = await loadSkinImage(
    b64.startsWith("data:") ? b64 : `data:image/png;base64,${b64}`,
  );
  const canvas = document.createElement("canvas");
  canvas.width = SKIN_SIZE;
  canvas.height = SKIN_SIZE;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("画布不可用");
  ctx.clearRect(0, 0, SKIN_SIZE, SKIN_SIZE);
  ctx.drawImage(img, 0, 0);
  pixels = ctx.getImageData(0, 0, SKIN_SIZE, SKIN_SIZE).data;
  skinEditor.name = name;
  // 模型自动判定：slim 皮肤的手臂贴图末两列基本留白，classic 的有内容。
  // 判错只会让手臂差一列宽，且用户随时可在界面上覆盖，所以值得自动猜一次。
  skinEditor.variant = variant === "SLIM" || isSlimArm(pixels) ? "SLIM" : "CLASSIC";
  skinEditor.sourceHeight = img.naturalHeight;
  resetState();
}

/** 按手臂贴图**末两列**的填充比例判断 slim。
 *
 *  slim 的手臂只有 3 列宽，展开占 x=40..53（右臂）/ 32..45（左臂），而贴图为 4 列布局
 *  预留了 16 宽 —— 于是末两列（右臂 54/55、左臂 46/47）基本空着；classic 用满 4 列，
 *  那两列是手臂背面的贴图，几乎填满。
 *
 *  用**比例**而不是「必须全空」：皮肤作者常在空白处随手点几个像素
 *  （实测某张 slim 皮肤第 55 列有 2 个点），要求全空会把纤细误判成经典。 */
function isSlimArm(px: Uint8ClampedArray): boolean {
  const tailFill = (x0: number, y0: number): number => {
    let filled = 0;
    for (let x = x0; x < x0 + 2; x++) {
      for (let y = y0; y < y0 + 16; y++) {
        if ((px[(y * SKIN_SIZE + x) * 4 + 3] ?? 0) > 0) filled++;
      }
    }
    return filled / 32;
  };
  return tailFill(54, 16) < 0.25 && tailFill(46, 48) < 0.25;
}

/** 当前像素导出为 PNG base64（无画布返回空串）。 */
export function exportEditorPng(): string {
  if (!pixels) return "";
  const canvas = document.createElement("canvas");
  canvas.width = SKIN_SIZE;
  canvas.height = SKIN_SIZE;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  // 传副本：ImageData 会接管传入的缓冲，直接给活引用等于把编辑缓冲交出去
  ctx.putImageData(
    new ImageData(new Uint8ClampedArray(pixels), SKIN_SIZE, SKIN_SIZE),
    0,
    0,
  );
  const url = canvas.toDataURL("image/png");
  return url.slice(url.indexOf(",") + 1);
}

function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex.trim());
  if (!m) return { r: 0, g: 0, b: 0 };
  return {
    r: Number.parseInt(m[1] ?? "0", 16),
    g: Number.parseInt(m[2] ?? "0", 16),
    b: Number.parseInt(m[3] ?? "0", 16),
  };
}

function sameBuffer(a: Uint8ClampedArray, b: Uint8ClampedArray): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/** 取色：把该像素的颜色与不透明度写回当前色，返回 #rrggbb（无画布返回 null）。 */
export function pickColorAt(x: number, y: number): string | null {
  if (!pixels) return null;
  const i = (y * SKIN_SIZE + x) * 4;
  const r = pixels[i] ?? 0;
  const g = pixels[i + 1] ?? 0;
  const b = pixels[i + 2] ?? 0;
  skinEditor.alpha = pixels[i + 3] ?? 0;
  return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

/** 落笔（画笔 / 橡皮）：按当前笔刷边长画一个方块，以点击像素为中心。 */
export function paintAt(x: number, y: number) {
  if (!pixels) return;
  const { r, g, b } = hexToRgb(skinEditor.color);
  const erase = skinEditor.tool === "eraser";
  const half = Math.floor(skinEditor.size / 2);
  for (let dy = 0; dy < skinEditor.size; dy++) {
    for (let dx = 0; dx < skinEditor.size; dx++) {
      const px = x + dx - half;
      const py = y + dy - half;
      if (px < 0 || py < 0 || px >= SKIN_SIZE || py >= SKIN_SIZE) continue;
      const i = (py * SKIN_SIZE + px) * 4;
      if (erase) {
        pixels[i] = 0;
        pixels[i + 1] = 0;
        pixels[i + 2] = 0;
        pixels[i + 3] = 0;
      } else {
        pixels[i] = r;
        pixels[i + 1] = g;
        pixels[i + 2] = b;
        pixels[i + 3] = skinEditor.alpha;
      }
    }
  }
  bumpRevision();
}

/** 洪水填充（4 邻域，全画布范围；同色完全匹配，不做容差 —— 像素画要的是可预期）。 */
export function floodFillAt(x: number, y: number) {
  if (!pixels) return;
  const start = (y * SKIN_SIZE + x) * 4;
  const target = [
    pixels[start] ?? 0,
    pixels[start + 1] ?? 0,
    pixels[start + 2] ?? 0,
    pixels[start + 3] ?? 0,
  ];
  const { r, g, b } = hexToRgb(skinEditor.color);
  const next = [r, g, b, skinEditor.alpha] as const;
  if (
    target[0] === next[0] &&
    target[1] === next[1] &&
    target[2] === next[2] &&
    target[3] === next[3]
  ) {
    return;
  }

  beginStroke();
  const seen = new Uint8Array(SKIN_SIZE * SKIN_SIZE);
  const stack: number[] = [x, y];
  while (stack.length > 0) {
    const cy = stack.pop() ?? 0;
    const cx = stack.pop() ?? 0;
    if (cx < 0 || cy < 0 || cx >= SKIN_SIZE || cy >= SKIN_SIZE) continue;
    const idx = cy * SKIN_SIZE + cx;
    if (seen[idx] === 1) continue;
    seen[idx] = 1;
    const p = idx * 4;
    if (
      pixels[p] !== target[0] ||
      pixels[p + 1] !== target[1] ||
      pixels[p + 2] !== target[2] ||
      pixels[p + 3] !== target[3]
    ) {
      continue;
    }
    pixels[p] = next[0];
    pixels[p + 1] = next[1];
    pixels[p + 2] = next[2];
    pixels[p + 3] = next[3];
    stack.push(cx + 1, cy, cx - 1, cy, cx, cy + 1, cx, cy - 1);
  }
  bumpRevision();
  endStroke();
}

/** 笔画开始：取起点快照（重复调用无副作用）。 */
export function beginStroke() {
  if (!pixels || strokeBase) return;
  strokeBase = new Uint8ClampedArray(pixels);
}

/** 笔画结束：与起点不同才入撤销栈。 */
export function endStroke() {
  if (!pixels || !strokeBase) return;
  if (!sameBuffer(strokeBase, pixels)) {
    undoStack.push(strokeBase);
    if (undoStack.length > UNDO_LIMIT) undoStack.shift();
    redoStack = [];
    syncDepths();
  }
  strokeBase = null;
}

export function undo() {
  const prev = undoStack.pop();
  if (!prev || !pixels) return;
  redoStack.push(new Uint8ClampedArray(pixels));
  pixels.set(prev);
  syncDepths();
  bumpRevision();
}

export function redo() {
  const next = redoStack.pop();
  if (!next || !pixels) return;
  undoStack.push(new Uint8ClampedArray(pixels));
  pixels.set(next);
  syncDepths();
  bumpRevision();
}

/** 指针像素坐标 → 部位 key（不在任何标注区返回空串）。 */
export function regionKeyAt(x: number, y: number): string {
  const hit = EDITOR_REGIONS.find(
    (r) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h,
  );
  return hit?.key ?? "";
}
