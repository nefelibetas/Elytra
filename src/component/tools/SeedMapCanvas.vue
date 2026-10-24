<script setup lang="ts">
// 种子地图画布：cubiomes 预测瓦片拼接 + 结构候选标记；只依赖种子/版本。
import { onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { Crosshair, Minus, Plus } from "@lucide/vue";
import {
  getSeedMapBlock,
  getSeedMapSlimeChunks,
  getSeedMapStructures,
  getSeedMapTile,
  type SeedMapSlimeChunk,
  type SeedMapStructure,
} from "../../api/seedmap";
import { errMsg } from "../../utils/error";

interface SeedMapHover {
  x: number;
  z: number;
  biome: string;
}

const props = withDefaults(
  defineProps<{
    seed: string;
    version: string;
    showStructures: boolean;
    showSlime: boolean;
    initialScale?: number;
  }>(),
  { initialScale: 0.25 },
);

const emit = defineEmits<{
  hover: [value: SeedMapHover | null];
  loading: [value: boolean];
  error: [message: string];
  camera: [value: { x: number; z: number }];
}>();

const wrapRef = ref<HTMLDivElement | null>(null);
const canvasRef = ref<HTMLCanvasElement | null>(null);
const cam = reactive({ x: 0, z: 0, scale: props.initialScale });

const TILE_BLOCKS = 512;
const MIN_SCALE = 0.15;
const MAX_SCALE = 4;
const MAX_INFLIGHT = 3;
const MAX_TILES = 160;
const HOVER_DELAY = 80;

interface TileEntry {
  img: HTMLImageElement;
  url: string;
}

let tiles = new Map<string, TileEntry>();
let tileOrder: string[] = [];
let loading = new Set<string>();
let failed = new Map<string, number>();
let structures = new Map<string, SeedMapStructure[]>();
let structureLoading = new Set<string>();
let structureQueue: string[] = [];
let structureInFlight = 0;
let slimeChunks = new Map<string, SeedMapSlimeChunk[]>();
let slimeLoading = new Set<string>();
let slimeQueue: string[] = [];
let slimeInFlight = 0;
let inFlight = 0;
let generation = 0;
let dirty = true;
let raf = 0;
let ro: ResizeObserver | null = null;
let dragging = false;
let dragPointer = -1;
let lastX = 0;
let lastY = 0;
let hoverTimer: number | null = null;
let hoverSeq = 0;
let lastHoverKey = "";

function tileKey(rx: number, rz: number): string {
  return `${rx},${rz}`;
}

function parseKey(key: string): [number, number] {
  const [rx, rz] = key.split(",");
  return [Number(rx), Number(rz)];
}

function emitLoading() {
  emit(
    "loading",
    inFlight > 0 ||
      structureInFlight > 0 ||
      structureQueue.length > 0 ||
      slimeInFlight > 0 ||
      slimeQueue.length > 0,
  );
}

function emitError(e: unknown) {
  emit("error", errMsg(e));
}

let lastCameraEmit = 0;
function emitCamera(force = false) {
  const now = performance.now();
  if (!force && now - lastCameraEmit < 100) return;
  lastCameraEmit = now;
  emit("camera", { x: Math.round(cam.x), z: Math.round(cam.z) });
}

function revokeAll() {
  for (const entry of tiles.values()) URL.revokeObjectURL(entry.url);
  tiles = new Map();
  tileOrder = [];
  loading.clear();
  failed.clear();
  inFlight = 0;
  structures = new Map();
  structureLoading.clear();
  structureQueue = [];
  structureInFlight = 0;
  slimeChunks = new Map();
  slimeLoading.clear();
  slimeQueue = [];
  slimeInFlight = 0;
  emitLoading();
}

function reset() {
  generation++;
  revokeAll();
  cam.x = 0;
  cam.z = 0;
  cam.scale = props.initialScale;
  dirty = true;
  emitCamera(true);
}

function screenToWorld(clientX: number, clientY: number): { x: number; z: number } {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return { x: cam.x, z: cam.z };
  return {
    x: cam.x + (clientX - rect.left - rect.width / 2) / cam.scale,
    z: cam.z + (clientY - rect.top - rect.height / 2) / cam.scale,
  };
}

function blockAt(clientX: number, clientY: number) {
  const p = screenToWorld(clientX, clientY);
  return { x: Math.floor(p.x), z: Math.floor(p.z) };
}

function visibleSlots(): Array<{ rx: number; rz: number; dist: number }> {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect || rect.width <= 0 || rect.height <= 0) return [];
  const halfW = rect.width / 2 / cam.scale;
  const halfZ = rect.height / 2 / cam.scale;
  const minRx = Math.floor((cam.x - halfW) / TILE_BLOCKS);
  const maxRx = Math.floor((cam.x + halfW) / TILE_BLOCKS);
  const minRz = Math.floor((cam.z - halfZ) / TILE_BLOCKS);
  const maxRz = Math.floor((cam.z + halfZ) / TILE_BLOCKS);
  const centerRx = cam.x / TILE_BLOCKS;
  const centerRz = cam.z / TILE_BLOCKS;
  const slots: Array<{ rx: number; rz: number; dist: number }> = [];
  for (let rx = minRx; rx <= maxRx; rx++) {
    for (let rz = minRz; rz <= maxRz; rz++) {
      const dx = rx - centerRx;
      const dz = rz - centerRz;
      slots.push({ rx, rz, dist: dx * dx + dz * dz });
    }
  }
  slots.sort((a, b) => a.dist - b.dist);
  return slots;
}

function requestTile(rx: number, rz: number) {
  const key = tileKey(rx, rz);
  if (tiles.has(key) || loading.has(key) || inFlight >= MAX_INFLIGHT) return;
  if ((failed.get(key) ?? 0) >= 2) return;
  loading.add(key);
  inFlight++;
  emitLoading();
  const gen = generation;
  getSeedMapTile(props.seed, props.version, rx, rz)
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
        tiles.set(key, { img, url });
        const oldIdx = tileOrder.indexOf(key);
        if (oldIdx >= 0) tileOrder.splice(oldIdx, 1);
        tileOrder.push(key);
        while (tileOrder.length > MAX_TILES) {
          const oldest = tileOrder.shift();
          if (!oldest || oldest === key) continue;
          const entry = tiles.get(oldest);
          if (entry) {
            tiles.delete(oldest);
            URL.revokeObjectURL(entry.url);
          }
        }
        loading.delete(key);
        inFlight = Math.max(0, inFlight - 1);
        failed.delete(key);
        emitLoading();
        dirty = true;
        if (props.showSlime) requestSlime(rx, rz);
        if (props.showStructures) requestStructures(rx, rz);
      };
      img.onerror = () => {
        URL.revokeObjectURL(url);
        if (gen === generation) failTile(key);
      };
      img.src = url;
    })
    .catch((e) => {
      if (gen === generation) failTile(key, e);
    });
}

function failTile(key: string, e?: unknown) {
  loading.delete(key);
  inFlight = Math.max(0, inFlight - 1);
  failed.set(key, (failed.get(key) ?? 0) + 1);
  if (e !== undefined) emitError(e);
  emitLoading();
  dirty = true;
}

function requestStructures(rx: number, rz: number) {
  const key = tileKey(rx, rz);
  if (structures.has(key) || structureLoading.has(key)) return;
  structureLoading.add(key);
  structureQueue.push(key);
  pumpStructures();
}

function pumpStructures() {
  while (structureInFlight < 2 && structureQueue.length > 0) {
    const key = structureQueue.shift();
    if (!key || structures.has(key)) {
      if (key) structureLoading.delete(key);
      continue;
    }
    const [rx, rz] = parseKey(key);
    structureInFlight++;
    emitLoading();
    const gen = generation;
    getSeedMapStructures(props.seed, props.version, rx, rz)
      .then((list) => {
        if (gen !== generation) return;
        structures.set(key, list);
        dirty = true;
      })
      .catch((e) => {
        if (gen === generation) emitError(e);
      })
      .finally(() => {
        structureLoading.delete(key);
        structureInFlight = Math.max(0, structureInFlight - 1);
        emitLoading();
        pumpStructures();
        dirty = true;
      });
  }
}

function requestSlime(rx: number, rz: number) {
  const key = tileKey(rx, rz);
  if (slimeChunks.has(key) || slimeLoading.has(key)) return;
  slimeLoading.add(key);
  slimeQueue.push(key);
  pumpSlime();
}

function pumpSlime() {
  while (slimeInFlight < 2 && slimeQueue.length > 0) {
    const key = slimeQueue.shift();
    if (!key || slimeChunks.has(key)) {
      if (key) slimeLoading.delete(key);
      continue;
    }
    const [rx, rz] = parseKey(key);
    slimeInFlight++;
    emitLoading();
    const gen = generation;
    getSeedMapSlimeChunks(props.seed, props.version, rx, rz)
      .then((list) => {
        if (gen !== generation) return;
        slimeChunks.set(key, list);
        dirty = true;
      })
      .catch((e) => {
        if (gen === generation) emitError(e);
      })
      .finally(() => {
        slimeLoading.delete(key);
        slimeInFlight = Math.max(0, slimeInFlight - 1);
        emitLoading();
        pumpSlime();
        dirty = true;
      });
  }
}

function pump() {
  const slots = visibleSlots();
  for (const slot of slots) {
    if (inFlight >= MAX_INFLIGHT) break;
    requestTile(slot.rx, slot.rz);
  }
}

function draw() {
  const wrap = wrapRef.value;
  const canvas = canvasRef.value;
  if (!wrap || !canvas) return;
  const rect = wrap.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.round(rect.width * dpr);
  const h = Math.round(rect.height * dpr);
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, rect.width, rect.height);

  const slots = visibleSlots();
  for (const { rx, rz } of slots) {
    const entry = tiles.get(tileKey(rx, rz));
    if (!entry) continue;
    const sx = (rx * TILE_BLOCKS - cam.x) * cam.scale + rect.width / 2;
    const sy = (rz * TILE_BLOCKS - cam.z) * cam.scale + rect.height / 2;
    const size = TILE_BLOCKS * cam.scale;
    ctx.drawImage(entry.img, sx, sy, size, size);
  }

  if (props.showSlime) {
    const size = 16 * cam.scale;
    ctx.fillStyle = "rgba(74, 222, 128, 0.28)";
    ctx.strokeStyle = "rgba(34, 197, 94, 0.55)";
    ctx.lineWidth = 1;
    for (const list of slimeChunks.values()) {
      for (const c of list) {
        const sx = (c.x * 16 - cam.x) * cam.scale + rect.width / 2;
        const sy = (c.z * 16 - cam.z) * cam.scale + rect.height / 2;
        if (sx + size < -2 || sy + size < -2 || sx > rect.width + 2 || sy > rect.height + 2) continue;
        ctx.fillRect(sx, sy, size, size);
        if (size > 6) ctx.strokeRect(sx + 0.5, sy + 0.5, size - 1, size - 1);
      }
    }
  }

  if (props.showStructures) {
    for (const list of structures.values()) {
      for (const s of list) {
        const sx = (s.x - cam.x) * cam.scale + rect.width / 2;
        const sy = (s.z - cam.z) * cam.scale + rect.height / 2;
        if (sx < -12 || sy < -12 || sx > rect.width + 12 || sy > rect.height + 12) continue;
        ctx.beginPath();
        ctx.arc(sx, sy, 3.5, 0, Math.PI * 2);
        ctx.fillStyle = STRUCTURE_COLORS[s.kind] ?? "#e5e7eb";
        ctx.fill();
        ctx.lineWidth = 1;
        ctx.strokeStyle = "rgba(10, 14, 18, 0.75)";
        ctx.stroke();
      }
    }
  }
}

const STRUCTURE_COLORS: Record<string, string> = {
  village: "#f2c14e",
  desert_pyramid: "#e0b15a",
  jungle_temple: "#5fbf77",
  swamp_hut: "#7a9e5a",
  igloo: "#bfe6ff",
  ocean_ruin: "#7bc8d9",
  shipwreck: "#b08b62",
  monument: "#59b6a5",
  mansion: "#b28bd6",
  outpost: "#db7b6b",
  ruined_portal: "#c084fc",
  ancient_city: "#6d7cff",
  treasure: "#f59e0b",
  trail_ruins: "#d4a373",
  trial_chambers: "#e879f9",
  abandoned_camp: "#f97316",
};

function loop() {
  raf = requestAnimationFrame(loop);
  if (!dirty) return;
  dirty = false;
  draw();
  pump();
}

function zoomAt(clientX: number, clientY: number, factor: number) {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return;
  const world = screenToWorld(clientX, clientY);
  const next = Math.min(MAX_SCALE, Math.max(MIN_SCALE, cam.scale * factor));
  cam.scale = next;
  cam.x = world.x - (clientX - rect.left - rect.width / 2) / next;
  cam.z = world.z - (clientY - rect.top - rect.height / 2) / next;
  dirty = true;
  emitCamera();
}

function onWheel(e: WheelEvent) {
  const factor = Math.exp(-e.deltaY * 0.0015);
  zoomAt(e.clientX, e.clientY, factor);
}

function zoomBy(factor: number) {
  const rect = wrapRef.value?.getBoundingClientRect();
  if (!rect) return;
  zoomAt(rect.left + rect.width / 2, rect.top + rect.height / 2, factor);
}

function resetCam() {
  cam.x = 0;
  cam.z = 0;
  cam.scale = props.initialScale;
  dirty = true;
  emitCamera(true);
}

function focusOn(x: number, z: number) {
  cam.x = x;
  cam.z = z;
  cam.scale = Math.max(cam.scale, 0.5);
  dirty = true;
  emitCamera(true);
}

defineExpose({ focusOn });

function onPointerDown(e: PointerEvent) {
  if (e.button !== 0) return;
  dragging = true;
  dragPointer = e.pointerId;
  lastX = e.clientX;
  lastY = e.clientY;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  if (hoverTimer !== null) clearTimeout(hoverTimer);
  hoverTimer = null;
  emit("hover", null);
}

function onPointerMove(e: PointerEvent) {
  if (dragging && e.pointerId === dragPointer) {
    cam.x -= (e.clientX - lastX) / cam.scale;
    cam.z -= (e.clientY - lastY) / cam.scale;
    lastX = e.clientX;
    lastY = e.clientY;
    dirty = true;
    emitCamera();
    return;
  }
  scheduleHover(e.clientX, e.clientY);
}

function onPointerUp(e: PointerEvent) {
  if (!dragging || e.pointerId !== dragPointer) return;
  dragging = false;
  dragPointer = -1;
  (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  scheduleHover(e.clientX, e.clientY);
}

function onPointerLeave() {
  if (dragging) return;
  if (hoverTimer !== null) clearTimeout(hoverTimer);
  hoverTimer = null;
  lastHoverKey = "";
  emit("hover", null);
}

function scheduleHover(clientX: number, clientY: number) {
  const b = blockAt(clientX, clientY);
  const key = `${b.x},${b.z}`;
  if (key === lastHoverKey) return;
  lastHoverKey = key;
  if (hoverTimer !== null) clearTimeout(hoverTimer);
  const seq = ++hoverSeq;
  hoverTimer = window.setTimeout(async () => {
    hoverTimer = null;
    try {
      const info = await getSeedMapBlock(props.seed, props.version, b.x, b.z);
      if (seq === hoverSeq) emit("hover", { x: b.x, z: b.z, biome: info.biome });
    } catch {
      if (seq === hoverSeq) emit("hover", { x: b.x, z: b.z, biome: "" });
    }
  }, HOVER_DELAY);
}

watch(
  () => [props.seed, props.version],
  () => reset(),
);
watch(
  () => props.showStructures,
  (on) => {
    if (!on) return;
    for (const { rx, rz } of visibleSlots()) {
      if (tiles.has(tileKey(rx, rz))) requestStructures(rx, rz);
    }
    dirty = true;
  },
);
watch(
  () => props.showSlime,
  (on) => {
    if (!on) return;
    for (const { rx, rz } of visibleSlots()) requestSlime(rx, rz);
    dirty = true;
  },
);

onMounted(() => {
  ro = new ResizeObserver(() => {
    dirty = true;
  });
  if (wrapRef.value) ro.observe(wrapRef.value);
  raf = requestAnimationFrame(loop);
  emitCamera(true);
});

onBeforeUnmount(() => {
  cancelAnimationFrame(raf);
  ro?.disconnect();
  if (hoverTimer !== null) clearTimeout(hoverTimer);
  revokeAll();
});
</script>

<template>
  <div
    ref="wrapRef"
    class="smc"
    @wheel.prevent="onWheel"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
    @pointerleave="onPointerLeave"
  >
    <canvas ref="canvasRef"></canvas>
    <div class="smc-zoom">
      <button type="button" title="+" @pointerdown.stop @click.stop="zoomBy(1.25)"><Plus :size="14" /></button>
      <button type="button" title="-" @pointerdown.stop @click.stop="zoomBy(1 / 1.25)"><Minus :size="14" /></button>
      <button type="button" title="Reset" @pointerdown.stop @click.stop="resetCam"><Crosshair :size="14" /></button>
    </div>
  </div>
</template>

<style scoped>
.smc {
  position: relative;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: var(--surface-page);
  touch-action: none;
  cursor: grab;
  user-select: none;
}

.smc:active {
  cursor: grabbing;
}

canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
}

.smc-zoom {
  position: absolute;
  top: 8px;
  left: 8px;
  z-index: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.smc-zoom button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--text-secondary);
  cursor: pointer;
}

.smc-zoom button:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
</style>
