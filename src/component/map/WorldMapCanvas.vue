<script setup lang="ts">
// 世界预览画布：只读已加载 region，不生成预测地形。
// 相机采用「当前 + 目标」双状态：平移即时跟手，滚轮缩放走 rAF 指数平滑。
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import {
  getMapBlock,
  getMapFolderBlock,
  getMapFolderTile,
  getMapSlimeChunks,
  getMapTile,
  listMapFolderRegions,
  listMapRegions,
  type MapBlockInfo,
  type MapRegionInfo,
  type SeedMapSlimeChunk,
} from "../../api/map";

interface MapHover {
  x: number;
  z: number;
  biome: string | null;
  pending?: boolean;
}

const props = defineProps<{
  versionName: string;
  worldName: string;
  /** 非空时按任意地图根目录读取（工具箱文件夹入口），忽略 version/world */
  rootPath: string;
  dimension: string;
  /** 俯视向下扫描的最高 Y（降低可看洞穴/地下） */
  maxY: number;
  showGrid: boolean;
  chunkBorders: boolean;
  modColors: boolean;
  /** 十进制种子；空串时不请求史莱姆区块 */
  seed?: string;
  /** 仅主世界显示史莱姆区块覆盖层 */
  showSlime?: boolean;
}>();

const folderMode = computed(() => props.rootPath.length > 0);
const overworld = computed(() => props.dimension === "");
const hasSeed = computed(() => (props.seed ?? "").length > 0);

const emit = defineEmits<{
  hover: [value: MapHover | null];
  state: [value: "loading" | "ready" | "empty" | "error", detail?: string];
}>();

const wrapRef = ref<HTMLDivElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);

const REGION_BLOCKS = 512;
const MIN_SCALE = 0.05;
const MAX_SCALE = 8;
const MAX_INFLIGHT = 4;
const MAX_TILES = 96;
const PROBE_DELAY = 120;
const SMOOTH_TAU = 80;
/** region 范围兜底适配留白（瓦片未就绪时） */
const FIT_MARGIN = 0.88;
/** 内容自适应：地形外留白（方块）与视口留白 */
const CONTENT_PADDING = 24;
const CONTENT_MARGIN = 0.94;
const reduceMotion =
  typeof window !== "undefined" &&
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

const cam = reactive({ x: 0, z: 0, scale: 1 });
const target = reactive({ x: 0, z: 0, scale: 1 });
const regions = ref<MapRegionInfo[]>([]);

interface TileEntry {
  img: HTMLImageElement;
  url: string;
  mtime: number;
}

interface MapBounds {
  minX: number;
  minZ: number;
  maxX: number;
  maxZ: number;
}

let tiles = new Map<string, TileEntry>();
let loading = new Set<string>();
let failed = new Map<string, number>();
let slimeChunks = new Map<string, SeedMapSlimeChunk[]>();
let slimeLoading = new Set<string>();
let slimeFailed = new Map<string, number>();
let inFlight = 0;
let generation = 0;
let refreshBusy = false;
let pollTimer: number | null = null;
let raf = 0;
let lastFrame = 0;
let dirty = true;
let firstFit = true;
/** 瓦片加载完成后按实际不透明像素范围放大一次，避免空 region 撑大视野 */
let autoFitPending = false;
let contentBounds: MapBounds | null = null;
let drawH = 0;
let ro: ResizeObserver | null = null;

const dragging = ref(false);
let pointerId: number | null = null;
let lastClientX = 0;
let lastClientY = 0;

let probeTimer: number | null = null;
let probeSeq = 0;
let lastHoverKey = "";
let styleTimer: number | null = null;

function regionKey(r: MapRegionInfo): string {
  return `${r.x},${r.z}`;
}

function setState(value: "loading" | "ready" | "empty" | "error", detail?: string) {
  emit("state", value, detail);
}

function clampScale(value: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));
}

function clearTiles() {
  for (const entry of tiles.values()) URL.revokeObjectURL(entry.url);
  tiles = new Map();
  loading.clear();
  failed.clear();
  slimeChunks = new Map();
  slimeLoading.clear();
  slimeFailed.clear();
  inFlight = 0;
}

function clearProbe() {
  if (probeTimer !== null) {
    clearTimeout(probeTimer);
    probeTimer = null;
  }
  probeSeq++;
}

function screenToWorld(clientX: number, clientY: number): { x: number; z: number } {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return { x: cam.x, z: cam.z };
  return {
    x: cam.x + (clientX - rect.left - rect.width / 2) / cam.scale,
    z: cam.z + (clientY - rect.top - rect.height / 2) / cam.scale,
  };
}

function blockAt(clientX: number, clientY: number): { x: number; z: number } {
  const p = screenToWorld(clientX, clientY);
  return { x: Math.floor(p.x), z: Math.floor(p.z) };
}

async function probeBlock(x: number, z: number): Promise<MapBlockInfo | null> {
  try {
    return await (folderMode.value
      ? getMapFolderBlock(props.rootPath, props.dimension, x, z, props.maxY)
      : getMapBlock(
          props.versionName,
          props.worldName,
          props.dimension,
          x,
          z,
          props.maxY,
        ));
  } catch {
    return null;
  }
}

function emitHover(value: MapHover | null) {
  emit("hover", value);
}

function scheduleHoverProbe(x: number, z: number) {
  if (probeTimer !== null) clearTimeout(probeTimer);
  const seq = ++probeSeq;
  emitHover({ x, z, biome: null, pending: true });
  probeTimer = window.setTimeout(async () => {
    probeTimer = null;
    const info = await probeBlock(x, z);
    if (seq !== probeSeq) return;
    emitHover({
      x,
      z,
      biome: info?.biome ?? null,
      pending: false,
    });
  }, PROBE_DELAY);
}

function updateHover(clientX: number, clientY: number) {
  const b = blockAt(clientX, clientY);
  const key = `${props.dimension}|${b.x}|${b.z}|${props.maxY}`;
  if (key === lastHoverKey) return;
  lastHoverKey = key;
  scheduleHoverProbe(b.x, b.z);
}

function normalizeWheel(event: WheelEvent): number {
  let delta = event.deltaY;
  if (event.deltaMode === 1) delta *= 16;
  else if (event.deltaMode === 2) delta *= drawH;
  return Math.max(-240, Math.min(240, delta));
}

function zoomAt(clientX: number, clientY: number, factor: number) {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return;
  const world = screenToWorld(clientX, clientY);
  const nextScale = clampScale(target.scale * factor);
  target.scale = nextScale;
  target.x = world.x - (clientX - rect.left - rect.width / 2) / nextScale;
  target.z = world.z - (clientY - rect.top - rect.height / 2) / nextScale;
  invalidate();
}

function zoomIn() {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return;
  zoomAt(rect.left + rect.width / 2, rect.top + rect.height / 2, 1.25);
}

function zoomOut() {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return;
  zoomAt(rect.left + rect.width / 2, rect.top + rect.height / 2, 1 / 1.25);
}

function fitRegions() {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return;
  if (regions.value.length === 0) {
    cam.x = cam.z = 0;
    cam.scale = 1;
    target.x = target.z = 0;
    target.scale = 1;
    invalidate();
    return;
  }
  let minX = Infinity;
  let minZ = Infinity;
  let maxX = -Infinity;
  let maxZ = -Infinity;
  for (const r of regions.value) {
    minX = Math.min(minX, r.x * REGION_BLOCKS);
    minZ = Math.min(minZ, r.z * REGION_BLOCKS);
    maxX = Math.max(maxX, (r.x + 1) * REGION_BLOCKS);
    maxZ = Math.max(maxZ, (r.z + 1) * REGION_BLOCKS);
  }
  const spanX = Math.max(1, maxX - minX);
  const spanZ = Math.max(1, maxZ - minZ);
  const scale = clampScale(Math.min(rect.width / spanX, rect.height / spanZ) * FIT_MARGIN);
  cam.x = target.x = (minX + maxX) / 2;
  cam.z = target.z = (minZ + maxZ) / 2;
  cam.scale = target.scale = scale;
  invalidate();
}

/** 已加载瓦片的不透明像素并集（世界方块坐标）；全空/未加载返回 null。 */
function measureContentBounds(): MapBounds | null {
  const probe = document.createElement("canvas");
  probe.width = REGION_BLOCKS;
  probe.height = REGION_BLOCKS;
  const ctx = probe.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  let minX = Infinity;
  let minZ = Infinity;
  let maxX = -Infinity;
  let maxZ = -Infinity;
  for (const region of regions.value) {
    const entry = tiles.get(regionKey(region));
    if (!entry) continue;
    ctx.clearRect(0, 0, REGION_BLOCKS, REGION_BLOCKS);
    ctx.drawImage(entry.img, 0, 0, REGION_BLOCKS, REGION_BLOCKS);
    const data = ctx.getImageData(0, 0, REGION_BLOCKS, REGION_BLOCKS).data;
    for (let z = 0; z < REGION_BLOCKS; z++) {
      for (let x = 0; x < REGION_BLOCKS; x++) {
        if (data[(z * REGION_BLOCKS + x) * 4 + 3] === 0) continue;
        const bx = region.x * REGION_BLOCKS + x;
        const bz = region.z * REGION_BLOCKS + z;
        if (bx < minX) minX = bx;
        if (bx > maxX) maxX = bx;
        if (bz < minZ) minZ = bz;
        if (bz > maxZ) maxZ = bz;
      }
    }
  }
  if (minX > maxX || minZ > maxZ) return null;
  return { minX, minZ, maxX: maxX + 1, maxZ: maxZ + 1 };
}

/** 按地形内容适配；无内容范围返回 false。immediate=true 时不做缩放动画。 */
function fitContent(immediate = false): boolean {
  const rect = wrapRef.value?.getBoundingClientRect();
  const bounds = contentBounds;
  if (!rect || !bounds) return false;
  const minX = bounds.minX - CONTENT_PADDING;
  const minZ = bounds.minZ - CONTENT_PADDING;
  const maxX = bounds.maxX + CONTENT_PADDING;
  const maxZ = bounds.maxZ + CONTENT_PADDING;
  const spanX = Math.max(1, maxX - minX);
  const spanZ = Math.max(1, maxZ - minZ);
  const scale = clampScale(Math.min(rect.width / spanX, rect.height / spanZ) * CONTENT_MARGIN);
  const cx = (minX + maxX) / 2;
  const cz = (minZ + maxZ) / 2;
  if (immediate) {
    cam.x = target.x = cx;
    cam.z = target.z = cz;
    cam.scale = target.scale = scale;
  } else {
    target.x = cx;
    target.z = cz;
    target.scale = scale;
  }
  invalidate();
  return true;
}

function resetView() {
  firstFit = false;
  autoFitPending = false;
  if (!fitContent(true)) fitRegions();
}

/** 首个批次全部解码后按内容适配一次；部分 region 加载失败时保留 region 概览。 */
function maybeAutoFit() {
  if (!autoFitPending || inFlight > 0) return;
  const expected = Math.min(regions.value.length, MAX_TILES);
  if (tiles.size < expected || tiles.size === 0) return;
  autoFitPending = false;
  contentBounds = measureContentBounds();
  fitContent();
}

function requestTile(region: MapRegionInfo) {
  const key = regionKey(region);
  const existing = tiles.get(key);
  if (existing && existing.mtime === region.mtime) return;
  if (loading.has(key) || inFlight >= MAX_INFLIGHT) return;
  const failedAt = failed.get(key);
  if (failedAt !== undefined && Date.now() - failedAt < 10_000) return;
  const gen = generation;
  loading.add(key);
  inFlight++;
  const tile = folderMode.value
    ? getMapFolderTile(
        props.rootPath,
        props.dimension,
        region.x,
        region.z,
        props.maxY,
        props.modColors,
      )
    : getMapTile(
        props.versionName,
        props.worldName,
        props.dimension,
        region.x,
        region.z,
        props.maxY,
        props.modColors,
      );
  tile
    .then((blob) => {
      if (gen !== generation) return;
      const url = URL.createObjectURL(blob);
      const img = new Image();
      img.onload = () => {
        if (gen !== generation) {
          URL.revokeObjectURL(url);
          return;
        }
        const prev = tiles.get(key);
        if (prev) URL.revokeObjectURL(prev.url);
        tiles.set(key, { img, url, mtime: region.mtime });
        failed.delete(key);
        while (tiles.size > MAX_TILES) {
          const oldest = tiles.keys().next().value;
          if (!oldest) break;
          const entry = tiles.get(oldest);
          if (entry) URL.revokeObjectURL(entry.url);
          tiles.delete(oldest);
        }
        invalidate();
        maybeAutoFit();
      };
      img.onerror = () => {
        URL.revokeObjectURL(url);
        failed.set(key, Date.now());
        invalidate();
      };
      img.src = url;
    })
    .catch(() => {
      if (gen === generation) failed.set(key, Date.now());
    })
    .finally(() => {
      if (gen === generation) {
        loading.delete(key);
        inFlight = Math.max(0, inFlight - 1);
        invalidate();
      }
    });
}

function requestSlime(region: MapRegionInfo) {
  const key = regionKey(region);
  if (!props.showSlime || !overworld.value || !hasSeed.value) return;
  if (slimeChunks.has(key) || slimeLoading.has(key)) return;
  const failedAt = slimeFailed.get(key);
  if (failedAt !== undefined && Date.now() - failedAt < 10_000) return;
  slimeLoading.add(key);
  const gen = generation;
  getMapSlimeChunks(props.seed ?? "", region.x, region.z)
    .then((list) => {
      if (gen !== generation) return;
      slimeChunks.set(key, list);
      slimeFailed.delete(key);
      invalidate();
    })
    .catch(() => {
      if (gen === generation) slimeFailed.set(key, Date.now());
    })
    .finally(() => {
      if (gen === generation) slimeLoading.delete(key);
    });
}

function invalidate() {
  dirty = true;
  if (raf === 0) {
    lastFrame = 0;
    raf = requestAnimationFrame(tick);
  }
}

function drawChecker(ctx: CanvasRenderingContext2D, x: number, y: number, size: number) {
  const cell = Math.max(8, Math.min(24, size / 24));
  ctx.fillStyle = "rgba(127,127,127,0.08)";
  for (let yy = 0; yy < size; yy += cell) {
    for (let xx = 0; xx < size; xx += cell) {
      if (((xx / cell) + (yy / cell)) % 2 === 0) {
        ctx.fillRect(x + xx, y + yy, Math.min(cell, size - xx), Math.min(cell, size - yy));
      }
    }
  }
}

function draw() {
  const canvas = canvasRef.value;
  const wrap = wrapRef.value;
  if (!canvas || !wrap) return;
  const rect = wrap.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.floor(rect.width));
  const h = Math.max(1, Math.floor(rect.height));
  drawH = h;
  if (canvas.width !== Math.floor(w * dpr) || canvas.height !== Math.floor(h * dpr)) {
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.imageSmoothingEnabled = cam.scale < 1;
  ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--surface-page") || "#111";
  ctx.fillRect(0, 0, w, h);

  const scale = cam.scale;
  const minX = cam.x - w / 2 / scale;
  const maxX = cam.x + w / 2 / scale;
  const minZ = cam.z - h / 2 / scale;
  const maxZ = cam.z + h / 2 / scale;
  const rx0 = Math.floor(minX / REGION_BLOCKS);
  const rx1 = Math.floor(maxX / REGION_BLOCKS);
  const rz0 = Math.floor(minZ / REGION_BLOCKS);
  const rz1 = Math.floor(maxZ / REGION_BLOCKS);
  const toScreenX = (x: number) => (x - cam.x) * scale + w / 2;
  const toScreenY = (z: number) => (z - cam.z) * scale + h / 2;
  const size = REGION_BLOCKS * scale;

  for (const region of regions.value) {
    if (region.x < rx0 || region.x > rx1 || region.z < rz0 || region.z > rz1) continue;
    const key = regionKey(region);
    const entry = tiles.get(key);
    const x = toScreenX(region.x * REGION_BLOCKS);
    const y = toScreenY(region.z * REGION_BLOCKS);
    if (entry && entry.mtime === region.mtime) {
      ctx.drawImage(entry.img, x, y, size, size);
    } else {
      drawChecker(ctx, x, y, size);
      requestTile(region);
    }
  }

  if (props.showSlime && overworld.value && hasSeed.value) {
    for (const region of regions.value) {
      if (region.x < rx0 || region.x > rx1 || region.z < rz0 || region.z > rz1) continue;
      requestSlime(region);
    }
    const slimeSize = 16 * scale;
    ctx.fillStyle = "rgba(74, 222, 128, 0.28)";
    ctx.strokeStyle = "rgba(34, 197, 94, 0.55)";
    ctx.lineWidth = 1;
    for (const region of regions.value) {
      if (region.x < rx0 || region.x > rx1 || region.z < rz0 || region.z > rz1) continue;
      const list = slimeChunks.get(regionKey(region));
      if (!list) continue;
      for (const c of list) {
        const sx = toScreenX(c.x * 16);
        const sy = toScreenY(c.z * 16);
        if (sx + slimeSize < -2 || sy + slimeSize < -2 || sx > w + 2 || sy > h + 2) continue;
        ctx.fillRect(sx, sy, slimeSize, slimeSize);
        if (slimeSize > 6) ctx.strokeRect(sx + 0.5, sy + 0.5, slimeSize - 1, slimeSize - 1);
      }
    }
  }

  if (props.chunkBorders) {
    ctx.strokeStyle = "rgba(255,255,255,0.12)";
    ctx.lineWidth = 1;
    const step = 16;
    const bx0 = Math.floor(minX / step) * step;
    const bz0 = Math.floor(minZ / step) * step;
    for (let bx = bx0; bx <= maxX; bx += step) {
      const sx = toScreenX(bx);
      ctx.beginPath();
      ctx.moveTo(sx, 0);
      ctx.lineTo(sx, h);
      ctx.stroke();
    }
    for (let bz = bz0; bz <= maxZ; bz += step) {
      const sy = toScreenY(bz);
      ctx.beginPath();
      ctx.moveTo(0, sy);
      ctx.lineTo(w, sy);
      ctx.stroke();
    }
  }

  if (props.showGrid && scale >= 2) {
    ctx.strokeStyle = "rgba(255,255,255,0.06)";
    ctx.lineWidth = 1;
    const bx0 = Math.floor(minX);
    const bz0 = Math.floor(minZ);
    for (let bx = bx0; bx <= maxX; bx++) {
      const sx = toScreenX(bx);
      ctx.beginPath();
      ctx.moveTo(sx, 0);
      ctx.lineTo(sx, h);
      ctx.stroke();
    }
    for (let bz = bz0; bz <= maxZ; bz++) {
      const sy = toScreenY(bz);
      ctx.beginPath();
      ctx.moveTo(0, sy);
      ctx.lineTo(w, sy);
      ctx.stroke();
    }
  }
}

function tick(ts: number) {
  const dt = lastFrame ? Math.min(100, ts - lastFrame) : 16;
  lastFrame = ts;
  const k = reduceMotion ? 1 : 1 - Math.exp(-dt / SMOOTH_TAU);
  cam.x += (target.x - cam.x) * k;
  cam.z += (target.z - cam.z) * k;
  cam.scale += (target.scale - cam.scale) * k;
  const done =
    Math.abs(target.x - cam.x) < 0.01 &&
    Math.abs(target.z - cam.z) < 0.01 &&
    Math.abs(target.scale - cam.scale) < 0.0005;
  if (done) {
    cam.x = target.x;
    cam.z = target.z;
    cam.scale = target.scale;
  }
  if (dirty || !done) {
    draw();
    dirty = false;
  }
  raf = done ? 0 : requestAnimationFrame(tick);
}

async function refreshRegions() {
  if (refreshBusy) return;
  refreshBusy = true;
  const gen = generation;
  try {
    const list = folderMode.value
      ? await listMapFolderRegions(props.rootPath, props.dimension)
      : await listMapRegions(props.versionName, props.worldName, props.dimension);
    if (gen !== generation) return;
    regions.value = list;
    if (list.length === 0) {
      setState("empty");
    } else {
      setState("ready");
      if (firstFit) {
        firstFit = false;
        fitRegions();
        autoFitPending = true;
      }
      for (const r of list) requestTile(r);
    }
    for (const [key, entry] of [...tiles]) {
      const still = list.some((r) => regionKey(r) === key && r.mtime === entry.mtime);
      if (!still) {
        URL.revokeObjectURL(entry.url);
        tiles.delete(key);
      }
    }
    invalidate();
  } catch (e) {
    if (gen === generation) setState("error", e instanceof Error ? e.message : String(e));
  } finally {
    refreshBusy = false;
  }
}

function resetForStyle() {
  generation++;
  clearTiles();
  clearProbe();
  lastHoverKey = "";
  emitHover(null);
  for (const r of regions.value) requestTile(r);
  invalidate();
}

function resetForView() {
  if (styleTimer !== null) {
    clearTimeout(styleTimer);
    styleTimer = null;
  }
  generation++;
  clearTiles();
  clearProbe();
  lastHoverKey = "";
  emitHover(null);
  regions.value = [];
  firstFit = true;
  autoFitPending = false;
  contentBounds = null;
  setState("loading");
  void refreshRegions();
}

function onPointerDown(e: PointerEvent) {
  if (e.button !== 0 && e.button !== 2) return;
  e.preventDefault();
  autoFitPending = false;
  pointerId = e.pointerId;
  dragging.value = true;
  lastClientX = e.clientX;
  lastClientY = e.clientY;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  clearProbe();
}

function onPointerMove(e: PointerEvent) {
  if (!dragging.value) {
    updateHover(e.clientX, e.clientY);
    return;
  }
  const dx = e.clientX - lastClientX;
  const dy = e.clientY - lastClientY;
  lastClientX = e.clientX;
  lastClientY = e.clientY;
  cam.x -= dx / cam.scale;
  cam.z -= dy / cam.scale;
  target.x = cam.x;
  target.z = cam.z;
  target.scale = cam.scale;
  invalidate();
}

function endPointer(e: PointerEvent) {
  if (pointerId !== e.pointerId) return;
  pointerId = null;
  dragging.value = false;
  try {
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  } catch {
    // 指针已丢失
  }
  updateHover(e.clientX, e.clientY);
}

function onPointerLeave() {
  if (dragging.value) return;
  clearProbe();
  lastHoverKey = "";
  emitHover(null);
}

function onWheel(e: WheelEvent) {
  e.preventDefault();
  autoFitPending = false;
  const delta = normalizeWheel(e);
  if (delta === 0) return;
  const factor = Math.exp(-delta * 0.0018);
  zoomAt(e.clientX, e.clientY, factor);
}

function onContextMenu(e: Event) {
  e.preventDefault();
}

function resize() {
  invalidate();
}

watch(
  () => [props.rootPath, props.versionName, props.worldName, props.seed],
  () => resetForView(),
);
watch(
  () => props.showSlime,
  () => invalidate(),
);
watch(
  () => props.dimension,
  () => resetForView(),
);
watch(
  () => [props.maxY, props.modColors],
  () => {
    if (styleTimer !== null) clearTimeout(styleTimer);
    styleTimer = window.setTimeout(() => {
      styleTimer = null;
      resetForStyle();
    }, 120);
  },
);
watch(() => [props.showGrid, props.chunkBorders], invalidate);

onMounted(() => {
  setState("loading");
  ro = new ResizeObserver(resize);
  if (wrapRef.value) ro.observe(wrapRef.value);
  void refreshRegions();
  pollTimer = window.setInterval(() => {
    if (document.visibilityState === "visible") void refreshRegions();
  }, 3000);
  invalidate();
});

onBeforeUnmount(() => {
  generation++;
  clearTiles();
  clearProbe();
  if (styleTimer !== null) clearTimeout(styleTimer);
  styleTimer = null;
  if (pollTimer !== null) clearInterval(pollTimer);
  pollTimer = null;
  ro?.disconnect();
  ro = null;
  if (raf !== 0) cancelAnimationFrame(raf);
  raf = 0;
});

function focusOn(x: number, z: number) {
  target.x = x;
  target.z = z;
  invalidate();
}

defineExpose({
  zoomIn,
  zoomOut,
  resetView,
  refresh: refreshRegions,
  center: () => ({ x: cam.x, z: cam.z }),
  focusOn,
});
</script>

<template>
  <div ref="wrapRef" class="map-wrap">
    <canvas
      ref="canvasRef"
      class="map-canvas"
      :class="{ dragging }"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="endPointer"
      @pointercancel="endPointer"
      @pointerleave="onPointerLeave"
      @wheel="onWheel"
      @contextmenu="onContextMenu"
    />
  </div>
</template>

<style scoped>
.map-wrap {
  position: absolute;
  inset: 0;
  overflow: hidden;
  background: var(--surface-page);
}

.map-canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.map-canvas.dragging {
  cursor: grabbing;
}
</style>
