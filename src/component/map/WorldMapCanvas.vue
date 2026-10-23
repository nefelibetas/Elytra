<script setup lang="ts">
// 世界预览画布：只读已加载 region，不生成预测地形。
// 相机采用「当前 + 目标」双状态：平移即时跟手，滚轮缩放走 rAF 指数平滑。
import { onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { getMapBlock, getMapTile, listMapRegions, type MapBlockInfo, type MapRegionInfo } from "../../api/map";

interface MapHover {
  x: number;
  z: number;
  y: number | null;
  block: string | null;
  biome: string | null;
  pending?: boolean;
}

const props = defineProps<{
  versionName: string;
  worldName: string;
  dimension: string;
  /** topdown 或 slice@Y */
  renderStyle: string;
  showGrid: boolean;
  chunkBorders: boolean;
  modColors: boolean;
}>();

const emit = defineEmits<{
  hover: [value: MapHover | null];
  pin: [value: MapHover | null];
  state: [value: "loading" | "ready" | "empty" | "error", detail?: string];
}>();

const wrapRef = ref<HTMLDivElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);
const pinned = ref<MapHover | null>(null);

const REGION_BLOCKS = 512;
const MIN_SCALE = 0.05;
const MAX_SCALE = 8;
const MAX_INFLIGHT = 4;
const MAX_TILES = 96;
const PROBE_DELAY = 120;
const SMOOTH_TAU = 80;
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

let tiles = new Map<string, TileEntry>();
let loading = new Set<string>();
let failed = new Map<string, number>();
let inFlight = 0;
let generation = 0;
let refreshBusy = false;
let pollTimer: number | null = null;
let raf = 0;
let lastFrame = 0;
let dirty = true;
let firstFit = true;
let drawH = 0;
let ro: ResizeObserver | null = null;

const dragging = ref(false);
let pointerId: number | null = null;
let pointerButton = -1;
let dragMoved = false;
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

function sliceY(): number | null {
  const m = /^slice@(-?\d+)$/.exec(props.renderStyle);
  return m ? Number(m[1]) : null;
}

async function probeBlock(x: number, z: number): Promise<MapBlockInfo | null> {
  const y = sliceY();
  try {
    return await getMapBlock(
      props.versionName,
      props.worldName,
      props.dimension,
      x,
      z,
      y ?? undefined,
    );
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
  const y = sliceY();
  emitHover({ x, z, y, block: null, biome: null, pending: true });
  probeTimer = window.setTimeout(async () => {
    probeTimer = null;
    const info = await probeBlock(x, z);
    if (seq !== probeSeq) return;
    emitHover({
      x,
      z,
      y: info?.y ?? y,
      block: info?.block ?? null,
      biome: info?.biome ?? null,
      pending: false,
    });
  }, PROBE_DELAY);
}

function updateHover(clientX: number, clientY: number) {
  const b = blockAt(clientX, clientY);
  const key = `${props.dimension}|${b.x}|${b.z}|${props.renderStyle}`;
  if (key === lastHoverKey) return;
  lastHoverKey = key;
  scheduleHoverProbe(b.x, b.z);
}

async function togglePin(clientX: number, clientY: number) {
  const b = blockAt(clientX, clientY);
  if (pinned.value && pinned.value.x === b.x && pinned.value.z === b.z) {
    pinned.value = null;
    emit("pin", null);
    invalidate();
    return;
  }
  const info = await probeBlock(b.x, b.z);
  const next: MapHover = {
    x: b.x,
    z: b.z,
    y: info?.y ?? sliceY(),
    block: info?.block ?? null,
    biome: info?.biome ?? null,
    pending: false,
  };
  pinned.value = next;
  emit("pin", next);
  invalidate();
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
  const scale = clampScale(Math.min(rect.width / spanX, rect.height / spanZ) * 0.88);
  cam.x = target.x = (minX + maxX) / 2;
  cam.z = target.z = (minZ + maxZ) / 2;
  cam.scale = target.scale = scale;
  invalidate();
}

function resetView() {
  firstFit = true;
  fitRegions();
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
  const style = props.renderStyle;
  getMapTile(
    props.versionName,
    props.worldName,
    props.dimension,
    region.x,
    region.z,
    style,
    props.modColors,
  )
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

  const marker = pinned.value;
  if (marker) {
    const sx = toScreenX(marker.x);
    const sy = toScreenY(marker.z);
    ctx.strokeStyle = getComputedStyle(canvas).getPropertyValue("--color-brand") || "#39d98a";
    ctx.lineWidth = 2;
    ctx.strokeRect(sx, sy, scale, scale);
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
    const list = await listMapRegions(props.versionName, props.worldName, props.dimension);
    if (gen !== generation) return;
    regions.value = list;
    if (list.length === 0) {
      setState("empty");
    } else {
      setState("ready");
      if (firstFit) {
        firstFit = false;
        fitRegions();
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
  pinned.value = null;
  emit("pin", null);
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
  pinned.value = null;
  emit("pin", null);
  emitHover(null);
  regions.value = [];
  firstFit = true;
  setState("loading");
  void refreshRegions();
}

function onPointerDown(e: PointerEvent) {
  if (e.button !== 0 && e.button !== 2) return;
  e.preventDefault();
  pointerId = e.pointerId;
  pointerButton = e.button;
  dragMoved = false;
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
  if (Math.abs(dx) + Math.abs(dy) > 1) dragMoved = true;
  cam.x -= dx / cam.scale;
  cam.z -= dy / cam.scale;
  target.x = cam.x;
  target.z = cam.z;
  target.scale = cam.scale;
  invalidate();
}

function endPointer(e: PointerEvent) {
  if (pointerId !== e.pointerId) return;
  const button = pointerButton;
  const moved = dragMoved;
  pointerId = null;
  pointerButton = -1;
  dragging.value = false;
  try {
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  } catch {
    // 指针已丢失
  }
  if (button === 0 && !moved) {
    void togglePin(e.clientX, e.clientY);
  } else {
    updateHover(e.clientX, e.clientY);
  }
}

function onPointerLeave() {
  if (dragging.value) return;
  clearProbe();
  lastHoverKey = "";
  emitHover(null);
}

function onWheel(e: WheelEvent) {
  e.preventDefault();
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
  () => props.dimension,
  () => resetForView(),
);
watch(
  () => [props.renderStyle, props.modColors],
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

defineExpose({ zoomIn, zoomOut, resetView, refresh: refreshRegions });
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
