<script setup lang="ts">
// 皮肤绘画器：在 64×64 展开图上绘制皮肤，右侧实时等距 3D 预览。与衣柜无关，不要求正版账号，离线/第三方可用。
// 成果以 PNG 导入 / 导出，不写衣柜索引；像素缓冲在 store/skinEditor（刻意非响应式），本组件只管视口变换、指针交互与重绘调度。
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  Brush, Download, Eraser, Frame, Grid3x3, Hand, Maximize, PaintBucket, Pipette, Redo2,
  RotateCcw, Undo2, Upload, ZoomIn, ZoomOut,
} from "@lucide/vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  DEFAULT_VIEW_PITCH, DEFAULT_VIEW_YAW, drawFullCanvas,
} from "../../store/skin/skinRender";
import type { SkinPart } from "../../store/skin/skinRender";
import type { Animation, PickFace } from "../../store/skin/skinRender";
import {
  EDITOR_REGIONS, SKIN_SIZE, beginStroke, editorPixels, endStroke, exportEditorPng,
  exportSkinPng,
  floodFillAt, loadImageIntoEditor, newEditorCanvas, paintAt, pickColorAt, redo, regionKeyAt, regionLabel,
  importSkinPng,
  skinEditor, undo,
} from "../../store/skin/skinEditor";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import DropSelect from "../common/DropSelect.vue";

const { t, locale } = useI18n();

/** 工具定义（顺序即左侧工具栏顺序）。 */
const TOOLS = [
  { value: "brush", labelKey: "tools.skinpainter.tools.brush", icon: Brush },
  { value: "eraser", labelKey: "tools.skinpainter.tools.eraser", icon: Eraser },
  { value: "fill", labelKey: "tools.skinpainter.tools.fill", icon: PaintBucket },
  { value: "picker", labelKey: "tools.skinpainter.tools.picker", icon: Pipette },
  { value: "hand", labelKey: "tools.skinpainter.tools.hand", icon: Hand },
] as const;

/** 手臂模型选项（slim 手臂贴图仅 3 列宽，选错会采样错位）。 */
const MODEL_OPTIONS = [
  { value: "CLASSIC", labelKey: "tools.skinpainter.model.classic", descKey: "tools.skinpainter.model.classicDesc" },
  { value: "SLIM", labelKey: "tools.skinpainter.model.slim", descKey: "tools.skinpainter.model.slimDesc" },
] as const;

/** 笔刷边长选项。 */
const SIZE_OPTIONS = [
  { value: "1", label: "1 px" },
  { value: "2", label: "2 px" },
  { value: "3", label: "3 px" },
];

/** 预览动作选项（与衣柜页同一套语义）。 */
const ANIM_OPTIONS = [
  { value: "none", labelKey: "tools.skinpainter.anim.none" },
  { value: "idle", labelKey: "tools.skinpainter.anim.idle" },
  { value: "walk", labelKey: "tools.skinpainter.anim.walk" },
] as const;

/** 编辑模式选项。 */
const MODE_OPTIONS = [
  { value: "2d", labelKey: "tools.skinpainter.mode.2d" },
  { value: "3d", labelKey: "tools.skinpainter.mode.3d" },
] as const;

/** 3D 直绘图层：MC 1.8+ 双层皮肤结构，手动指定落笔层；不做「透明处自动穿透」，否则外层永远画不到。 */
const LAYER_OPTIONS = [
  { value: "0", labelKey: "tools.skinpainter.layer.inner" },
  { value: "1", labelKey: "tools.skinpainter.layer.outer" },
] as const;

/** 画布缩放范围（每皮肤像素的 CSS px），上限 96 以便单像素铺满屏做细节。 */
const MIN_SCALE = 2;
const MAX_SCALE = 96;
/** 3D 预览缩放范围（3D 直绘时预览是主画面，上限需够大以点准单像素）。 */
const PREVIEW_ZOOM_MIN = 0.4;
const PREVIEW_ZOOM_MAX = 4;
/** 俯仰角绝对范围：±90° 覆盖正顶面 / 正底面（与衣柜页 skinStudio 一致）。 */
const PREVIEW_PITCH_MIN = -90;
const PREVIEW_PITCH_MAX = 90;
/** 部位标签仅在缩放 ≥ LABEL_MIN_SCALE 时绘制（否则重叠糊成一团）。 */
const LABEL_MIN_SCALE = 6;

const viewport = ref<HTMLElement | null>(null);
const paintCanvas = ref<HTMLCanvasElement | null>(null);
const previewBox = ref<HTMLElement | null>(null);
const previewCanvas = ref<HTMLCanvasElement | null>(null);
/** 光标覆盖层：与预览同尺寸叠加，只画即将落笔的像素。 */
const cursorCanvas = ref<HTMLCanvasElement | null>(null);
/** 指针是否在画布区域内（空格临时抓手仅在此范围内切换工具）。 */
const inCanvas = ref(false);
/** 空格是否按住：即使先在画布外按下、再移入画布，也能按抓手处理。 */
const spaceHeld = ref(false);
/** 编辑模式：2d = 展开图逐像素；3d = 在模型上直接画。 */
const mode = ref<"2d" | "3d">("2d");
/** 3D 直绘目标图层：0 = 内层（本体），1 = 第二层（帽子/外套/袖腿）。 */
const paintLayer = ref<0 | 1>(0);

/** 画布视口变换：scale = 每皮肤像素的 CSS 尺寸，x/y = 画布左上角在视口内的位置。 */
const view = reactive({ scale: 8, x: 0, y: 0 });
/** 3D 预览相机（独立于衣柜的预览状态）。 */
const preview = reactive({
  yaw: DEFAULT_VIEW_YAW,
  pitch: DEFAULT_VIEW_PITCH,
  zoom: 1,
  /** 画面平移量（设备像素），重置视角时归零。 */
  offsetX: 0,
  offsetY: 0,
  /** 被隐藏的身体部位（可藏起挡住视线的其他部分）。 */
  hiddenParts: [] as SkinPart[],
  animation: "none" as Animation,
});

let painting = false;
let panning = false;
let lastPixel: { x: number; y: number } | null = null;
let panFrom = { x: 0, y: 0, vx: 0, vy: 0 };
let frame = 0;
let previewFrame = 0;
/** 64×64 中转画布（主视图放大与预览共用，避免每帧新建）。 */
let source: HTMLCanvasElement | null = null;
/** 预览专用贴图（透明像素垫棋盘格），与 `source` 分开：展开图不该看到这层底纹。 */
let previewSource: HTMLCanvasElement | null = null;
/** 棋盘格贴图（两个中性灰交替，每皮肤像素一格），只生成一次。 */
let checkerTex: HTMLCanvasElement | null = null;
let checkerPattern: CanvasPattern | null = null;
/** 最近一次预览绘制的可拾取面（绘制顺序：远 → 近），3D 直绘靠它做屏幕 → 贴图反查。 */
let previewHits: PickFace[] = [];
/** 指针最近一次的画布设备像素坐标：相机变化后按同一点重算光标。 */
let lastPointer: { x: number; y: number } | null = null;
/** 当前悬停命中的贴图像素（决定光标是十字准星还是手型）。 */
const hoverPick = ref<{ face: PickFace; x: number; y: number } | null>(null);
/** 3D 直绘笔画进行中。 */
let stroking = false;
/** 视口隐藏期间被请求的「适应窗口」（见 fitToViewport）。 */
let pendingFit = false;
/** 最近一次 fitToViewport 时的视口尺寸；模式切换后尺寸变化才重新适配。 */
let fittedW = 0;
let fittedH = 0;
/** 笔画上一个命中的面（同面内连线；跨面则直接落新点）。 */
let strokeFace: PickFace | null = null;
let strokeLast: { x: number; y: number } | null = null;

/** 把当前像素同步进中转画布并返回它。 */
function syncSource(): HTMLCanvasElement | null {
  const px = editorPixels();
  if (!px) return null;
  if (!source) {
    source = document.createElement("canvas");
    source.width = SKIN_SIZE;
    source.height = SKIN_SIZE;
  }
  const ctx = source.getContext("2d");
  if (!ctx) return null;
  ctx.putImageData(new ImageData(new Uint8ClampedArray(px), SKIN_SIZE, SKIN_SIZE), 0, 0);
  return source;
}

/** 透明区棋盘衬底（2×2 图案随视口变换缩放，每皮肤像素恰好一格）。 */
function checkerOf(ctx: CanvasRenderingContext2D): CanvasPattern | null {
  if (checkerPattern) return checkerPattern;
  const tile = document.createElement("canvas");
  tile.width = 2;
  tile.height = 2;
  const g = tile.getContext("2d");
  if (!g) return null;
  g.fillStyle = "rgba(128, 138, 152, 0.22)";
  g.fillRect(0, 0, 1, 1);
  g.fillRect(1, 1, 1, 1);
  checkerPattern = ctx.createPattern(tile, "repeat");
  return checkerPattern;
}

/** 视口尺寸变化时把画布内部分辨率对齐（含设备像素比）。 */
function fitCanvas(el: HTMLCanvasElement, box: HTMLElement): CanvasRenderingContext2D | null {
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(box.clientWidth * dpr));
  const h = Math.max(1, Math.round(box.clientHeight * dpr));
  if (el.width !== w || el.height !== h) {
    el.width = w;
    el.height = h;
  }
  return el.getContext("2d");
}

/** 重绘画布视口：棋盘衬底 → 像素 → 网格 → 部位标注（全部走视口变换）。 */
function drawEditor() {
  const el = paintCanvas.value;
  const box = viewport.value;
  const src = syncSource();
  if (!el || !box || !src) return;
  const ctx = fitCanvas(el, box);
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, el.width, el.height);
  ctx.imageSmoothingEnabled = false;

  // 后续绘制使用皮肤像素坐标（0..64）
  ctx.setTransform(view.scale * dpr, 0, 0, view.scale * dpr, view.x * dpr, view.y * dpr);

  const pattern = checkerOf(ctx);
  if (pattern) {
    ctx.fillStyle = pattern;
    ctx.fillRect(0, 0, SKIN_SIZE, SKIN_SIZE);
  }
  ctx.drawImage(src, 0, 0);

  // 线宽按缩放反算，否则放大后线粗会盖住像素
  const lw = 1 / view.scale;
  if (skinEditor.showGrid) {
    ctx.strokeStyle = "rgba(128, 138, 152, 0.5)";
    ctx.lineWidth = lw;
    ctx.beginPath();
    for (let i = 8; i < SKIN_SIZE; i += 8) {
      ctx.moveTo(i, 0);
      ctx.lineTo(i, SKIN_SIZE);
      ctx.moveTo(0, i);
      ctx.lineTo(SKIN_SIZE, i);
    }
    ctx.stroke();
  }
  if (skinEditor.showRegions) {
    // 每块一色（内层深 / 第二层浅），否则相邻块分界难辨
    const labelPx = 10 / view.scale;
    ctx.font = `600 ${labelPx}px "Microsoft YaHei", "PingFang SC", sans-serif`;
    ctx.textBaseline = "top";
    for (const r of EDITOR_REGIONS) {
      const active = skinEditor.hoverKey === r.key;
      // 悬停块：淡填充 + 加粗描边
      ctx.fillStyle = active ? `${r.color}2e` : `${r.color}0a`;
      ctx.fillRect(r.x, r.y, r.w, r.h);
      ctx.strokeStyle = r.color;
      ctx.lineWidth = active ? lw * 2.5 : lw;
      // 描边内缩半个线宽，否则一半画在矩形外裁到邻居
      ctx.strokeRect(r.x + lw / 2, r.y + lw / 2, r.w - lw, r.h - lw);
      if (view.scale >= LABEL_MIN_SCALE) {
        // 标签垫同色底以在皮肤图案上可读
        const label = regionLabel(r);
        const tw = ctx.measureText(label).width;
        ctx.fillStyle = `${r.color}dd`;
        ctx.fillRect(r.x + lw, r.y + lw, tw + labelPx * 0.6, labelPx * 1.3);
        ctx.fillStyle = "#ffffff";
        ctx.fillText(label, r.x + lw + labelPx * 0.3, r.y + lw + labelPx * 0.15);
      }
    }
  }
  ctx.setTransform(1, 0, 0, 1, 0, 0);
}

/** 棋盘格贴图：两个中性灰交替、每皮肤像素一格；固定灰阶，与主题无关。 */
function checkerTexture(): HTMLCanvasElement {
  if (checkerTex) return checkerTex;
  const c = document.createElement("canvas");
  c.width = SKIN_SIZE;
  c.height = SKIN_SIZE;
  const ctx = c.getContext("2d");
  if (ctx) {
    const img = ctx.createImageData(SKIN_SIZE, SKIN_SIZE);
    const light = [204, 209, 217];
    const dark = [182, 188, 198];
    for (let y = 0; y < SKIN_SIZE; y++) {
      for (let x = 0; x < SKIN_SIZE; x++) {
        const rgb = ((x + y) & 1) === 0 ? light : dark;
        const i = (y * SKIN_SIZE + x) * 4;
        img.data[i] = rgb[0] ?? 0;
        img.data[i + 1] = rgb[1] ?? 0;
        img.data[i + 2] = rgb[2] ?? 0;
        img.data[i + 3] = 255;
      }
    }
    ctx.putImageData(img, 0, 0);
  }
  checkerTex = c;
  return c;
}

/** 预览用贴图：空白像素垫棋盘（否则空皮肤在 3D 里看不见模型）。只垫第一层，第二层空着若也垫会盖住内层；皮肤以 source-over 叠加，alpha=0 不覆盖棋盘。 */
function syncPreviewSource(): HTMLCanvasElement | null {
  const src = syncSource();
  if (!src) return null;
  if (!previewSource) {
    previewSource = document.createElement("canvas");
    previewSource.width = SKIN_SIZE;
    previewSource.height = SKIN_SIZE;
  }
  const ctx = previewSource.getContext("2d");
  if (!ctx) return null;
  ctx.clearRect(0, 0, SKIN_SIZE, SKIN_SIZE);
  ctx.save();
  ctx.beginPath();
  // 一次 clip 覆盖全部第一层矩形（十字形角落不属于任何面）
  for (const r of EDITOR_REGIONS) {
    if (r.layer === 0) ctx.rect(r.x, r.y, r.w, r.h);
  }
  ctx.clip();
  ctx.drawImage(checkerTexture(), 0, 0);
  ctx.restore();
  ctx.drawImage(src, 0, 0);
  return previewSource;
}

/** 重绘 3D 预览（相机由 preview 状态驱动），并回填可拾取面。 */
function drawPreview() {
  const el = previewCanvas.value;
  const src = syncPreviewSource();
  if (!el || !src) return;
  const box = previewBox.value;
  if (!fitCanvas(el, box ?? el)) return;
  previewHits = [];
  drawFullCanvas(el, src, {
    yaw: preview.yaw,
    pitch: preview.pitch,
    slim: skinEditor.variant === "SLIM",
    // 旧版 64×32 皮肤下半为空：须按 legacy 走，否则用到不存在的左臂/左腿 UV
    legacy: skinEditor.sourceHeight > 0 && skinEditor.sourceHeight < 64,
    animation: preview.animation,
    phase: (performance.now() - animStart) / 1000,
    // 占比 0.9：填满可用空间并留边距（renderer 取 min(宽/20, 高/34)）
    fill: 0.9 * preview.zoom,
    offsetX: preview.offsetX,
    offsetY: preview.offsetY,
    hiddenParts: preview.hiddenParts,
    hits: previewHits,
  });
  // 相机 / 模型变化后按新矩阵重算悬停像素
  hoverPick.value = lastPointer ? pickAtDevice(lastPointer.x, lastPointer.y) : null;
  drawCursor();
}

/** 标出即将落到哪个贴图像素（该面仿射把 1×1 像素映成平行四边形）。 */
function drawCursor() {
  const el = cursorCanvas.value;
  const box = previewBox.value;
  if (!el || !box) return;
  const ctx = fitCanvas(el, box);
  if (!ctx) return;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, el.width, el.height);
  const hit = hoverPick.value;
  if (!hit) return;
  const [a, b, c, d, e, f] = hit.face.m;
  const at = (ux: number, uy: number): [number, number] => [a * ux + c * uy + e, b * ux + d * uy + f];
  const quad = [at(hit.x, hit.y), at(hit.x + 1, hit.y), at(hit.x + 1, hit.y + 1), at(hit.x, hit.y + 1)];
  ctx.beginPath();
  ctx.moveTo(quad[0]![0], quad[0]![1]);
  ctx.lineTo(quad[1]![0], quad[1]![1]);
  ctx.lineTo(quad[2]![0], quad[2]![1]);
  ctx.lineTo(quad[3]![0], quad[3]![1]);
  ctx.closePath();
  ctx.globalAlpha = 0.65;
  ctx.fillStyle = skinEditor.color;
  ctx.fill();
  ctx.globalAlpha = 1;
  ctx.lineWidth = 1.5;
  ctx.strokeStyle = "#ffffff";
  ctx.stroke();
}

/** 指针事件 → 预览画布的设备像素坐标（内部分辨率含 devicePixelRatio）。 */
function devicePoint(e: { clientX: number; clientY: number }): { x: number; y: number } | null {
  const el = previewCanvas.value;
  if (!el) return null;
  const r = el.getBoundingClientRect();
  if (r.width <= 0 || r.height <= 0) return null;
  const dpr = el.width / r.width;
  return { x: (e.clientX - r.left) * dpr, y: (e.clientY - r.top) * dpr };
}

/** 设备像素坐标 → 命中的面 + 贴图像素：求面仿射逆，落在其 uv 矩形内即命中（等价于点在四边形内）；previewHits 远 → 近排列，从后往前第一个即最前可见面。 */
function pickAtDevice(dx: number, dy: number): { face: PickFace; x: number; y: number } | null {
  for (let i = previewHits.length - 1; i >= 0; i--) {
    const face = previewHits[i]!;
    if (face.cape || face.layer !== paintLayer.value) continue;
    const [a, b, c, d, e, f] = face.m;
    const det = a * d - b * c;
    if (Math.abs(det) < 1e-9) continue;
    const ux = ((dx - e) * d - (dy - f) * c) / det;
    const uy = ((dy - f) * a - (dx - e) * b) / det;
    const [u, v, uw, uh] = face.uv;
    if (ux < u || ux >= u + uw || uy < v || uy >= v + uh) continue;
    return { face, x: Math.floor(ux), y: Math.floor(uy) };
  }
  return null;
}

/** 合并重绘（一帧内多次像素改动只重绘一次）。 */
function scheduleDraw() {
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    drawEditor();
    if (!previewFrame) {
      previewFrame = requestAnimationFrame(() => {
        previewFrame = 0;
        drawPreview();
      });
    }
  });
}

/** 视口内坐标（CSS px）。 */
function viewportPoint(e: { clientX: number; clientY: number }): { x: number; y: number } | null {
  const el = paintCanvas.value;
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  return { x: e.clientX - rect.left, y: e.clientY - rect.top };
}

/** 视口坐标 → 皮肤像素坐标（越界返回 null）。 */
function pixelAt(e: { clientX: number; clientY: number }): { x: number; y: number } | null {
  const p = viewportPoint(e);
  if (!p) return null;
  const x = Math.floor((p.x - view.x) / view.scale);
  const y = Math.floor((p.y - view.y) / view.scale);
  if (x < 0 || y < 0 || x >= SKIN_SIZE || y >= SKIN_SIZE) return null;
  return { x, y };
}

/** 以视口点为锚缩放（锚点下的像素保持不动）。 */
function zoomAt(px: number, py: number, factor: number) {
  const next = Math.min(MAX_SCALE, Math.max(MIN_SCALE, view.scale * factor));
  if (next === view.scale) return;
  const skinX = (px - view.x) / view.scale;
  const skinY = (py - view.y) / view.scale;
  view.scale = next;
  view.x = px - skinX * next;
  view.y = py - skinY * next;
  drawEditor();
}

/** 可单独隐藏的身体部位（对应渲染器 SkinPart，配色沿用展开图区域标注）。 */
const PART_OPTIONS: { key: SkinPart; labelKey: string; color: string }[] = [
  { key: "head", labelKey: "tools.skinpainter.parts.head", color: "#f59e0b" },
  { key: "body", labelKey: "tools.skinpainter.parts.body", color: "#4f8ef7" },
  { key: "rightArm", labelKey: "tools.skinpainter.parts.rightArm", color: "#10b981" },
  { key: "leftArm", labelKey: "tools.skinpainter.parts.leftArm", color: "#8b5cf6" },
  { key: "rightLeg", labelKey: "tools.skinpainter.parts.rightLeg", color: "#ef4444" },
  { key: "leftLeg", labelKey: "tools.skinpainter.parts.leftLeg", color: "#ec4899" },
];

/** 切换部位显示；`alt` = 只看该部位。隐藏的盒子不进可拾取面，故隐藏处不会被误画。 */
function togglePart(part: SkinPart, alt: boolean) {
  const all = PART_OPTIONS.map((p) => p.key);
  const others = all.filter((k) => k !== part);
  if (alt) {
    const soloed =
      preview.hiddenParts.length === others.length &&
      others.every((k) => preview.hiddenParts.includes(k));
    preview.hiddenParts = soloed ? [] : others;
  } else {
    const idx = preview.hiddenParts.indexOf(part);
    if (idx >= 0) preview.hiddenParts.splice(idx, 1);
    else preview.hiddenParts.push(part);
  }
  drawPreview();
}

/** 显示全部部位。 */
function showAllParts() {
  preview.hiddenParts = [];
  drawPreview();
}

/** 重置 3D 预览相机（视角 / 缩放 / 平移）。 */
function resetPreview() {
  preview.yaw = DEFAULT_VIEW_YAW;
  preview.pitch = DEFAULT_VIEW_PITCH;
  preview.zoom = 1;
  preview.offsetX = 0;
  preview.offsetY = 0;
  drawPreview();
}

/** 缩放按钮：2D 模式以画布中心为锚，3D 模式作用于预览缩放。 */
function zoomByButton(factor: number) {
  if (mode.value === "3d") {
    preview.zoom = Math.min(PREVIEW_ZOOM_MAX, Math.max(PREVIEW_ZOOM_MIN, preview.zoom * factor));
    drawPreview();
    return;
  }
  const box = viewport.value;
  if (!box) return;
  zoomAt(box.clientWidth / 2, box.clientHeight / 2, factor);
}

/** 适应视口：整数倍缩放到尽量大并居中（保证像素边界锐利）；3D 下展开图尺寸为 0，此时只记请求待视口可见再执行。 */
function fitToViewport() {
  const box = viewport.value;
  if (!box || box.clientWidth === 0 || box.clientHeight === 0) {
    pendingFit = true;
    return;
  }
  pendingFit = false;
  const side = Math.min(box.clientWidth / SKIN_SIZE, box.clientHeight / SKIN_SIZE);
  view.scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.floor(side)));
  view.x = (box.clientWidth - SKIN_SIZE * view.scale) / 2;
  view.y = (box.clientHeight - SKIN_SIZE * view.scale) / 2;
  fittedW = box.clientWidth;
  fittedH = box.clientHeight;
  drawEditor();
}

/** Bresenham 连线：指针移动快时事件会跳像素，逐点上色才不会断续。 */
function strokeBetween(from: { x: number; y: number }, to: { x: number; y: number }) {
  let x0 = from.x;
  let y0 = from.y;
  const dx = Math.abs(to.x - x0);
  const dy = -Math.abs(to.y - y0);
  const sx = x0 < to.x ? 1 : -1;
  const sy = y0 < to.y ? 1 : -1;
  let err = dx + dy;
  for (;;) {
    paintAt(x0, y0);
    if (x0 === to.x && y0 === to.y) break;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x0 += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y0 += sy;
    }
  }
}

function onPointerDown(e: PointerEvent) {
  const el = paintCanvas.value;
  const p = viewportPoint(e);
  if (!el || !p) return;
  // 抓手工具、空格、中键、右键都用于平移；其余按键才绘制
  if (skinEditor.tool === "hand" || spaceHeld.value || e.button === 1 || e.button === 2) {
    panning = true;
    panFrom = { x: p.x, y: p.y, vx: view.x, vy: view.y };
    el.setPointerCapture(e.pointerId);
    return;
  }
  if (e.button !== 0) return;
  const px = pixelAt(e);
  if (!px) return;
  if (skinEditor.tool === "picker") {
    const hex = pickColorAt(px.x, px.y);
    if (hex) skinEditor.color = hex;
    return;
  }
  if (skinEditor.tool === "fill") {
    floodFillAt(px.x, px.y);
    return;
  }
  el.setPointerCapture(e.pointerId);
  painting = true;
  lastPixel = px;
  beginStroke();
  paintAt(px.x, px.y);
}

function onPointerMove(e: PointerEvent) {
  const p = viewportPoint(e);
  const px = p ? pixelAt(e) : null;
  skinEditor.hoverKey = px ? regionKeyAt(px.x, px.y) : "";

  if (panning && p) {
    view.x = panFrom.vx + (p.x - panFrom.x);
    view.y = panFrom.vy + (p.y - panFrom.y);
    drawEditor();
    return;
  }
  if (!painting || !px) return;
  if (lastPixel) strokeBetween(lastPixel, px);
  else paintAt(px.x, px.y);
  lastPixel = px;
}

function onPointerUp() {
  panning = false;
  if (!painting) return;
  painting = false;
  lastPixel = null;
  endStroke();
}

function onWheel(e: WheelEvent) {
  const p = viewportPoint(e);
  if (!p) return;
  e.preventDefault();
  zoomAt(p.x, p.y, e.deltaY < 0 ? 1.15 : 1 / 1.15);
}

/** 3D 预览拖动旋转。 */
let previewDrag = false;
let previewFrom = { x: 0, y: 0, yaw: 0, pitch: 0 };
/** 3D 预览拖动视角（pan）会话：起点设备像素 + 起始平移量。 */
let previewPan = false;
let previewPanFrom = { x: 0, y: 0, ox: 0, oy: 0 };

function startPreviewPan(dev: { x: number; y: number }) {
  previewPan = true;
  previewPanFrom = { x: dev.x, y: dev.y, ox: preview.offsetX, oy: preview.offsetY };
}

function startPreviewRotate(e: PointerEvent) {
  previewDrag = true;
  previewFrom = { x: e.clientX, y: e.clientY, yaw: preview.yaw, pitch: preview.pitch };
}
/** 动作动画循环句柄（0 = 未运行，静止时不空转）。 */
let animRaf = 0;
/** 动画相位基准时刻。 */
let animStart = 0;

/** 动作切换：起停动画循环；静止时停帧并重绘一张定格图。 */
function syncAnimation() {
  if (preview.animation !== "none") {
    if (!animRaf) {
      animStart = performance.now();
      animRaf = requestAnimationFrame(animTick);
    }
    return;
  }
  if (animRaf) {
    cancelAnimationFrame(animRaf);
    animRaf = 0;
    drawPreview();
  }
}

function animTick() {
  drawPreview();
  animRaf = requestAnimationFrame(animTick);
}

function onPreviewDown(e: PointerEvent) {
  const dev = devicePoint(e);
  if (!dev) return;
  if (e.button !== 0 && e.button !== 1 && e.button !== 2) return;
  e.preventDefault();
  lastPointer = dev;
  const hand = skinEditor.tool === "hand" || spaceHeld.value;
  // 3D 直绘：左键落在皮肤上即落笔（取色 / 填充 / 绘制）
  if (mode.value === "3d" && e.button === 0 && !hand) {
    const hit = pickAtDevice(dev.x, dev.y);
    if (hit) {
      if (skinEditor.tool === "picker") {
        const hex = pickColorAt(hit.x, hit.y);
        if (hex) skinEditor.color = hex;
        return;
      }
      if (skinEditor.tool === "fill") {
        floodFillAt(hit.x, hit.y);
        return;
      }
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      stroking = true;
      strokeFace = hit.face;
      strokeLast = { x: hit.x, y: hit.y };
      beginStroke();
      paintAt(hit.x, hit.y);
      return;
    }
    // 左键从模型外空白开始：旋转视角；落笔只在命中皮肤时发生
    startPreviewRotate(e);
  } else if (e.button === 1 || e.button === 2) {
    // 右键 / 中键：平移。右键不再与左键空白旋转重复
    startPreviewPan(dev);
  } else if (hand) {
    // 抓手工具或按住空格：左键平移，命中皮肤也不落笔
    startPreviewPan(dev);
  } else {
    // 2D 预览左键旋转（3D 左键空白已在上方处理）
    startPreviewRotate(e);
  }
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
}

function onPreviewMove(e: PointerEvent) {
  const dev = devicePoint(e);
  if (dev) lastPointer = dev;

  // 3D 直绘且为绘图工具时，标出即将落笔的像素
  if (dev && mode.value === "3d" && skinEditor.tool !== "hand" && !spaceHeld.value) {
    hoverPick.value = pickAtDevice(dev.x, dev.y);
    drawCursor();
  } else if (hoverPick.value) {
    hoverPick.value = null;
    drawCursor();
  }

  if (dev && previewPan) {
    preview.offsetX = previewPanFrom.ox + (dev.x - previewPanFrom.x);
    preview.offsetY = previewPanFrom.oy + (dev.y - previewPanFrom.y);
    drawPreview();
  }

  if (dev && stroking) {
    const hit = pickAtDevice(dev.x, dev.y);
    if (hit) {
      // 同面内按贴图坐标连直线；跨面直接落新点（跨面连线会画到别处）
      if (strokeFace === hit.face && strokeLast) strokeBetween(strokeLast, { x: hit.x, y: hit.y });
      else paintAt(hit.x, hit.y);
      strokeFace = hit.face;
      strokeLast = { x: hit.x, y: hit.y };
    } else {
      // 拖出模型：断线，回到模型时重新落点
      strokeFace = null;
      strokeLast = null;
    }
  }

  if (!previewDrag) return;
  // 符号须与衣柜页（skinStudio YAW_SENS / PITCH_SENS）一致，否则两页拖动方向相反
  preview.yaw = previewFrom.yaw - (e.clientX - previewFrom.x) * 0.6;
  preview.pitch = Math.min(
    PREVIEW_PITCH_MAX,
    Math.max(PREVIEW_PITCH_MIN, previewFrom.pitch + (e.clientY - previewFrom.y) * 0.5),
  );
  drawPreview();
}

function onPreviewUp() {
  previewDrag = false;
  previewPan = false;
  if (!stroking) return;
  stroking = false;
  strokeFace = null;
  strokeLast = null;
  endStroke();
}

function onPreviewLeave() {
  inCanvas.value = false;
  lastPointer = null;
  if (!hoverPick.value) return;
  hoverPick.value = null;
  drawCursor();
}

function onPreviewWheel(e: WheelEvent) {
  e.preventDefault();
  preview.zoom = Math.min(
    PREVIEW_ZOOM_MAX,
    Math.max(PREVIEW_ZOOM_MIN, preview.zoom * (e.deltaY < 0 ? 1.12 : 1 / 1.12)),
  );
  drawPreview();
}

async function doImport() {
  const picked = await open({
    multiple: false,
    filters: [{ name: t("tools.skinpainter.importFilter"), extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
  });
  if (!picked || Array.isArray(picked)) return;
  try {
    const b64 = await importSkinPng(picked);
    const file = picked.split(/[\\/]/).pop() ?? t("tools.skinpainter.importedSkinName");
    await loadImageIntoEditor(b64, file.replace(/\.[^.]+$/, ""), skinEditor.variant);
    fitToViewport();
    showToast(t("tools.skinpainter.imported", { file }));
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function doExport() {
  const b64 = exportEditorPng();
  if (!b64) {
    showToast(t("tools.skinpainter.canvasEmpty"));
    return;
  }
  const target = await save({
    defaultPath: `${skinEditor.name.trim() || "skin"}.png`,
    filters: [{ name: t("tools.skinpainter.pngFilter"), extensions: ["png"] }],
  });
  if (!target) return;
  try {
    await exportSkinPng(target, b64);
    showToast(t("tools.skinpainter.exported"));
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 输入框 / 文本域内不接管快捷键（交给原生编辑行为）。 */
function isEditable(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable;
}

/** 空格按住期间的临时工具（松开还原）。 */
let toolBeforeSpace: string | null = null;

function releaseSpace() {
  spaceHeld.value = false;
  if (!toolBeforeSpace) return;
  skinEditor.tool = toolBeforeSpace as typeof skinEditor.tool;
  toolBeforeSpace = null;
}

function onKeyDown(e: KeyboardEvent) {
  if (isEditable(e.target)) return;
  if (e.ctrlKey || e.metaKey) {
    const k = e.key.toUpperCase();
    if (k === "Z" && !e.shiftKey) {
      e.preventDefault();
      undo();
    } else if (k === "Y" || (k === "Z" && e.shiftKey)) {
      e.preventDefault();
      redo();
    }
    return;
  }
  // 空格：全局记录按住状态；指针在画布区域内时同时临时切抓手
  if (e.code === "Space" && !e.repeat) {
    spaceHeld.value = true;
    if (inCanvas.value && !toolBeforeSpace) {
      e.preventDefault();
      toolBeforeSpace = skinEditor.tool;
      skinEditor.tool = "hand";
    }
  }
}

function onKeyUp(e: KeyboardEvent) {
  if (e.code === "Space") releaseSpace();
}

/** 缩放读数：2D 模式是「每皮肤像素多少 CSS px」，3D 模式是预览缩放的百分比。 */
const zoomLabel = computed(() =>
  mode.value === "3d" ? `${Math.round(preview.zoom * 100)}%` : `${Math.round(view.scale)}×`,
);

let ro: ResizeObserver | null = null;

onMounted(() => {
  // 默认给一张空白皮肤画布（非编辑现有皮肤）
  if (!skinEditor.ready) newEditorCanvas();

  ro = new ResizeObserver(() => {
    const box = viewport.value;
    const resized = !!box && (box.clientWidth !== fittedW || box.clientHeight !== fittedH);
    if (pendingFit || (mode.value === "2d" && resized)) fitToViewport();
    drawEditor();
    drawPreview();
  });
  if (viewport.value) ro.observe(viewport.value);
  if (previewBox.value) ro.observe(previewBox.value);

  fitToViewport();
  drawPreview();
  window.addEventListener("keydown", onKeyDown);
  window.addEventListener("keyup", onKeyUp);
  window.addEventListener("blur", releaseSpace);
});

onUnmounted(() => {
  ro?.disconnect();
  ro = null;
  if (frame) cancelAnimationFrame(frame);
  if (previewFrame) cancelAnimationFrame(previewFrame);
  if (animRaf) cancelAnimationFrame(animRaf);
  frame = 0;
  previewFrame = 0;
  animRaf = 0;
  toolBeforeSpace = null;
  spaceHeld.value = false;
  window.removeEventListener("keydown", onKeyDown);
  window.removeEventListener("keyup", onKeyUp);
  window.removeEventListener("blur", releaseSpace);
});

watch(() => skinEditor.revision, scheduleDraw);
watch(
  () => [skinEditor.showGrid, skinEditor.showRegions] as const,
  () => drawEditor(),
);
watch(() => skinEditor.variant, () => drawPreview());
watch(spaceHeld, (held) => {
  if (!held) return;
  hoverPick.value = null;
  drawCursor();
});
watch(() => preview.animation, () => syncAnimation());
watch(locale, () => drawEditor());
// 模式切换改变布局（画布 ↔ 预览互换），须待 DOM 重排后按新尺寸重绘
watch(mode, () => {
  lastPointer = null;
  hoverPick.value = null;
  void nextTick(() => {
    const box = viewport.value;
    const resized = !!box && (box.clientWidth !== fittedW || box.clientHeight !== fittedH);
    if (pendingFit || (mode.value === "2d" && resized)) fitToViewport();
    else drawEditor();
    drawPreview();
  });
});
</script>

<template>
  <div class="painter" :class="{ 'mode-3d': mode === '3d' }">
    <header class="pp-hd">
      <div class="pp-hd-text">
        <h3>{{ t("tools.skinpainter.title") }}</h3>
        <p v-if="mode === '2d'">{{ t("tools.skinpainter.hint2d") }}</p>
        <p v-else>{{ t("tools.skinpainter.hint3d") }}</p>
      </div>
      <div class="pp-hd-actions">
        <div class="pp-kind">
          <button
            v-for="m in MODE_OPTIONS"
            :key="m.value"
            :class="{ on: mode === m.value }"
            @click="mode = m.value"
          >
            {{ t(m.labelKey) }}
          </button>
        </div>
        <!-- 模型：slim 手臂贴图仅 3 列宽，按 classic 采样会错位 -->
        <div class="pp-kind">
          <button
            v-for="m in MODEL_OPTIONS"
            :key="m.value"
            :class="{ on: skinEditor.variant === m.value }"
            v-tooltip="t(m.descKey)"
            @click="skinEditor.variant = m.value"
          >
            {{ t(m.labelKey) }}
          </button>
        </div>
        <button class="pp-ghost" @click="doImport">
          <Upload :size="14" />
          {{ t("tools.skinpainter.import") }}
        </button>
        <button class="pp-primary" @click="doExport">
          <Download :size="14" />
          {{ t("tools.skinpainter.export") }}
        </button>
      </div>
    </header>

    <div class="pp-body">
      <nav class="pp-tools">
        <button
          v-for="tool in TOOLS"
          :key="tool.value"
          class="pp-tool"
          :class="{ on: skinEditor.tool === tool.value }"
          v-tooltip="t(tool.labelKey)"
          @click="skinEditor.tool = tool.value"
        >
          <component :is="tool.icon" :size="16" />
        </button>
      </nav>

      <div
        v-show="mode === '2d'"
        ref="viewport"
        class="pp-viewport"
        @pointerenter="inCanvas = true"
        @pointerleave="inCanvas = false"
      >
        <canvas
          ref="paintCanvas"
          class="pp-canvas"
          :class="{ 'is-hand': skinEditor.tool === 'hand' }"
          @pointerdown="onPointerDown"
          @pointermove="onPointerMove"
          @pointerup="onPointerUp"
          @pointercancel="onPointerUp"
          @pointerleave="skinEditor.hoverKey = ''"
          @wheel="onWheel"
          @contextmenu.prevent
        />
      </div>

      <aside class="pp-side">
        <div
          ref="previewBox"
          class="pp-preview-box"
          :class="{ 'on-skin': mode === '3d' && skinEditor.tool !== 'hand' && !spaceHeld && !!hoverPick }"
          @pointerdown="onPreviewDown"
          @pointerenter="inCanvas = true"
          @pointermove="onPreviewMove"
          @pointerup="onPreviewUp"
          @pointercancel="onPreviewUp"
          @pointerleave="onPreviewLeave"
          @wheel="onPreviewWheel"
          @contextmenu.prevent
        >
          <canvas ref="previewCanvas" class="pp-preview" />
          <!-- 光标覆盖层：只画即将落笔的像素，不参与模型重绘 -->
          <canvas ref="cursorCanvas" class="pp-cursor" />
        </div>

        <!-- 右侧控件列（3D 下与预览并排，避免占据高度压缩预览） -->
        <div class="pp-ctl-col">
          <!-- 图层：MC 1.8+ 双层结构，指定本次落笔层（否则外层永远画不到） -->
          <div v-if="mode === '3d'" class="pp-kind pp-layer-kind">
            <button
              v-for="l in LAYER_OPTIONS"
              :key="l.value"
              :class="{ on: String(paintLayer) === l.value }"
              @click="paintLayer = Number(l.value) as 0 | 1"
            >
              {{ t(l.labelKey) }}
            </button>
          </div>

          <div class="pp-anim">
            <button
              v-for="a in ANIM_OPTIONS"
              :key="a.value"
              class="pp-anim-btn"
              :class="{ on: preview.animation === a.value }"
              @click="preview.animation = a.value"
            >
              {{ t(a.labelKey) }}
            </button>
          </div>

          <!-- 部位显示开关（仅 3D）：画某一处时把挡住视线的其他部分藏起来 -->
          <div v-if="mode === '3d'" class="pp-parts">
            <div class="pp-parts-hd">
              <span>{{ t("tools.skinpainter.showParts") }}</span>
              <button
                class="pp-parts-all"
                :disabled="!preview.hiddenParts.length"
                @click="showAllParts"
              >
                {{ t("tools.skinpainter.allParts") }}
              </button>
            </div>
            <div class="pp-parts-grid">
              <button
                v-for="p in PART_OPTIONS"
                :key="p.key"
                class="pp-part"
                :class="{ off: preview.hiddenParts.includes(p.key) }"
                v-tooltip="t('tools.skinpainter.partTooltip', { part: t(p.labelKey) })"
                @click="togglePart(p.key, $event.altKey)"
              >
                <span class="pp-part-dot" :style="{ background: p.color }" />
                <span class="pp-part-label">{{ t(p.labelKey) }}</span>
              </button>
            </div>
          </div>
        </div>
      </aside>
    </div>

    <footer class="pp-foot">
      <label class="pp-color" v-tooltip="t('tools.skinpainter.brushColor')">
        <span
          class="pp-swatch"
          :style="{ background: skinEditor.color, opacity: Math.max(0.15, skinEditor.alpha / 255) }"
        />
        <input v-model="skinEditor.color" type="color" />
      </label>
      <label class="pp-alpha" v-tooltip="t('tools.skinpainter.opacity')">
        <span class="pp-foot-label">A</span>
        <input v-model.number="skinEditor.alpha" type="range" min="0" max="255" />
      </label>

      <DropSelect
        :model-value="String(skinEditor.size)"
        :options="SIZE_OPTIONS"
        width="88px"
        @update:model-value="skinEditor.size = Number($event)"
      />

      <span class="pp-sep" />

      <!-- 网格 / 标注只对展开图有意义 -->
      <button
        v-if="mode === '2d'"
        class="pp-tool flat"
        :class="{ on: skinEditor.showGrid }"
        v-tooltip="t('tools.skinpainter.pixelGrid')"
        @click="skinEditor.showGrid = !skinEditor.showGrid"
      >
        <Grid3x3 :size="15" />
      </button>
      <button
        v-if="mode === '2d'"
        class="pp-tool flat"
        :class="{ on: skinEditor.showRegions }"
        v-tooltip="t('tools.skinpainter.partLabels')"
        @click="skinEditor.showRegions = !skinEditor.showRegions"
      >
        <Frame :size="15" />
      </button>
      <span v-if="mode === '2d'" class="pp-sep" />

      <button
        class="pp-tool flat"
        :disabled="!skinEditor.undoDepth"
        v-tooltip="t('tools.skinpainter.undo')"
        @click="undo()"
      >
        <Undo2 :size="15" />
      </button>
      <button
        class="pp-tool flat"
        :disabled="!skinEditor.redoDepth"
        v-tooltip="t('tools.skinpainter.redo')"
        @click="redo()"
      >
        <Redo2 :size="15" />
      </button>

      <span class="pp-spacer" />

      <button class="pp-tool flat" v-tooltip="t('tools.skinpainter.zoomOut')" @click="zoomByButton(1 / 1.25)">
        <ZoomOut :size="15" />
      </button>
      <span class="pp-zoom pp-mono">{{ zoomLabel }}</span>
      <button class="pp-tool flat" v-tooltip="t('tools.skinpainter.zoomIn')" @click="zoomByButton(1.25)">
        <ZoomIn :size="15" />
      </button>
      <button
        v-if="mode === '2d'"
        class="pp-tool flat"
        v-tooltip="t('tools.skinpainter.fit')"
        @click="fitToViewport()"
      >
        <Maximize :size="15" />
      </button>
      <button
        v-if="mode === '3d'"
        class="pp-tool flat"
        v-tooltip="t('tools.skinpainter.resetView')"
        @click="resetPreview"
      >
        <RotateCcw :size="15" />
      </button>
    </footer>
  </div>
</template>

<style scoped>
.painter {
  box-sizing: border-box;
  height: 100%;
  min-height: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* ===== 头部 ===== */
.pp-hd {
  flex: none;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.pp-hd-text h3 {
  margin: 0 0 3px;
  font-size: 15px;
  color: var(--text-primary);
}

/* 标题块允许收缩，超出用省略号 */
.pp-hd-text {
  min-width: 0;
}

.pp-hd-text p {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.pp-hd-actions {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
}

.pp-kind {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-sm);
  background-color: var(--surface-hover);
}

.pp-kind button {
  height: 26px;
  padding: 0 12px;
  border: none;
  border-radius: var(--radius-sm);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  cursor: pointer;
  transition: color 0.15s, background-color 0.15s;
}

.pp-kind button.on {
  color: var(--color-brand);
  background-color: var(--surface-card);
}

.pp-ghost,
.pp-primary {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 14px;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
}

.pp-ghost {
  border: 1px solid var(--border-default);
  background-color: var(--surface-elevated);
  color: var(--text-secondary);
}

.pp-ghost:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.pp-primary {
  border: none;
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
}

.pp-primary:hover:not(:disabled) {
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

/* ===== 主体：工具 | 画布 | 预览 ===== */
.pp-body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 10px;
}

.pp-tools {
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background-color: var(--surface-elevated);
}

/* 工具按钮：激活态用实心品牌色 */
.pp-tool {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background-color: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: color 0.15s, background-color 0.15s, box-shadow 0.15s;
}

.pp-tool:hover:not(:disabled) {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.pp-tool.on {
  background-color: var(--color-brand);
  color: var(--text-inverse);
  box-shadow: var(--shadow-sm);
}

.pp-tool.flat {
  width: 28px;
  height: 28px;
}

.pp-tool:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.pp-viewport {
  flex: 1;
  min-width: 0;
  min-height: 0;
  position: relative;
  overflow: hidden;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background-color: var(--surface-page);
}

.pp-canvas {
  display: block;
  width: 100%;
  height: 100%;
  cursor: crosshair;
  touch-action: none;
}

.pp-canvas.is-hand {
  cursor: grab;
}

/* ===== 右侧预览：整列高度独占，下方动作切换 ===== */
.pp-side {
  flex: 0 0 210px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* 控件列：2D 下在预览框下方竖排，3D 下移到右侧（见 .mode-3d），避免压缩预览高度 */
.pp-ctl-col {
  flex: none;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* 3D 直绘：预览顶替画布成为主画面（视口已隐藏，右栏占满余下宽度）；210px 宽点不准单像素。 */
.mode-3d .pp-side {
  flex: 1 1 auto;
  flex-direction: row;
  align-items: stretch;
}

/* 3D：控件列挤到右侧窄栏（内容多时自身滚动，不挤压预览） */
.mode-3d .pp-ctl-col {
  flex: 0 0 118px;
  overflow-y: auto;
}

.pp-layer-kind {
  flex: none;
  width: 100%;
  box-sizing: border-box;
}
.pp-layer-kind button {
  flex: 1;
  min-width: 0;
  padding: 0 4px;
}

/* 窄栏里动画按钮竖排，避免平分 118px 挤成一团 */
.mode-3d .pp-anim {
  flex-direction: column;
  align-items: stretch;
}
.mode-3d .pp-anim-btn {
  flex: none;
}

.pp-preview-box {
  position: relative;
  flex: 1;
  min-height: 0;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: radial-gradient(circle at 50% 34%, var(--color-brand-bg), transparent 70%),
    var(--surface-page);
  overflow: hidden;
  cursor: grab;
  touch-action: none;
}

/* 3D 直绘：光标在皮肤上时换十字准星（可落笔），移出则回手型（靠 .pp-preview-box 的 grab） */
.pp-preview-box.on-skin {
  cursor: crosshair;
}

/* 预览与光标覆盖层叠在同一坐标系里（都按容器尺寸计算内部分辨率） */
.pp-preview,
.pp-cursor {
  position: absolute;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
}

.pp-cursor {
  pointer-events: none;
}

/* 部位显示开关：两列小按钮，隐藏的变暗划掉 */
.pp-parts {
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}
.pp-parts-hd {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.pp-parts-all {
  padding: 1px 6px;
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 4px;
  cursor: pointer;
}
.pp-parts-all:disabled {
  opacity: 0.45;
  cursor: default;
}
.pp-parts-grid {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.pp-part {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 4px 6px;
  font-size: 11.5px;
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 4px;
  cursor: pointer;
}
.pp-part.off {
  opacity: 0.4;
}
.pp-part.off .pp-part-label {
  text-decoration: line-through;
}
.pp-part-dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 2px;
}
.pp-part-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pp-anim {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-sm);
  background-color: var(--surface-hover);
}

.pp-anim-btn {
  flex: 1;
  height: 24px;
  border: none;
  border-radius: var(--radius-sm);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: color 0.15s, background-color 0.15s;
}

.pp-anim-btn:hover {
  color: var(--text-primary);
}

.pp-anim-btn.on {
  color: var(--color-brand);
  background-color: var(--surface-card);
}

/* ===== 底部视图控制条 ===== */
.pp-foot {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background-color: var(--surface-elevated);
}

.pp-color {
  position: relative;
  flex: none;
  width: 28px;
  height: 28px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  overflow: hidden;
  cursor: pointer;
}

.pp-swatch {
  position: absolute;
  inset: 3px;
  border-radius: 2px;
  pointer-events: none;
}

.pp-color input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  padding: 0;
  border: none;
  opacity: 0;
  cursor: pointer;
}

.pp-alpha {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  width: 96px;
}

.pp-alpha input {
  flex: 1;
  min-width: 0;
  accent-color: var(--color-brand);
}

.pp-foot-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.pp-sep {
  flex: none;
  width: 1px;
  height: 20px;
  background-color: var(--border-default);
}

.pp-spacer {
  flex: 1;
}

.pp-zoom {
  min-width: 38px;
  text-align: center;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.pp-mono {
  font-variant-numeric: tabular-nums;
}
</style>
