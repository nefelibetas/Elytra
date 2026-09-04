<script setup lang="ts">
// 地图画布（Canvas 2D）：瓦片拼接 + 拖拽平移 + 滚轮缩放。
// 每个 region = 一张 512×512 后端渲染 PNG；瓦片按需加载，相机变换纯前端。
// 坐标系：世界方块坐标 ↔ 屏幕。scale=1 时 1 方块 = 1px（region 512px）。
import { onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { errMsg } from "../../utils/error";

const props = defineProps<{
	/** 世界根目录（含 level.dat）。 */
	worldPath: string;
	/** 维度目录名：'' 主世界 / DIM-1 / DIM1。 */
	dimension: string;
	/** 渲染样式：topdown 或 slice@Y。 */
	style: string;
}>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const wrapRef = ref<HTMLDivElement | null>(null);

/** 相机：中心点（世界方块坐标）+ 缩放（px/方块）。 */
const cam = ref({ cx: 0, cz: 0, scale: 0.5 });
const MIN_SCALE = 0.05;
const MAX_SCALE = 8;

/** 已加载瓦片：key = "rx,rz" → HTMLImageElement。 */
const tiles = ref<Map<string, HTMLImageElement>>(new Map());
/** 加载中集合（防重复请求）。 */
const loading = new Set<string>();
/** 视口内 region 列表（后端 list_map_regions 结果，选中存档时拉取）。 */
const regions = ref<{ x: number; z: number }[]>([]);

/** 每帧重绘（dirty 标记 + rAF 合并）。 */
let dirty = true;
let raf = 0;
function invalidate() {
	dirty = true;
}
function loop() {
	if (dirty) {
		draw();
		dirty = false;
	}
	raf = requestAnimationFrame(loop);
}

function draw() {
	const canvas = canvasRef.value;
	const wrap = wrapRef.value;
	if (!canvas || !wrap) return;
	const dpr = window.devicePixelRatio || 1;
	const w = wrap.clientWidth;
	const h = wrap.clientHeight;
	if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
		canvas.width = w * dpr;
		canvas.height = h * dpr;
		canvas.style.width = `${w}px`;
		canvas.style.height = `${h}px`;
	}
	const ctx = canvas.getContext("2d");
	if (!ctx) return;
	ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
	// 背景：棋盘格弱纹理（空区可辨）
	ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--surface-page") || "#111";
	ctx.fillRect(0, 0, w, h);

	const { cx, cz, scale } = cam.value;
	const halfW = w / 2 / scale;
	const halfH = h / 2 / scale;
	// 可见世界范围
	const minX = cx - halfW, maxX = cx + halfW;
	const minZ = cz - halfH, maxZ = cz + halfH;
	// 相机变换：世界 → 屏幕
	const toScreenX = (wx: number) => (wx - cx) * scale + w / 2;
	const toScreenY = (wz: number) => (wz - cz) * scale + h / 2;

	const size = 512 * scale; // region 屏幕尺寸
	for (const r of regions.value) {
		const ox = r.x * 512, oz = r.z * 512; // region 原点（世界方块）
		if (ox + 512 < minX || ox > maxX || oz + 512 < minZ || oz > maxZ) continue;
		const img = tiles.value.get(`${r.x},${r.z}`);
		if (!img) {
			requestTile(r.x, r.z);
			continue;
		}
		ctx.imageSmoothingEnabled = scale < 1;
		ctx.drawImage(img, toScreenX(ox), toScreenY(oz), size, size);
	}
}

/** 请求瓦片（缓存键含 mtime 由后端管理；前端只防并发重复）。 */
function requestTile(rx: number, rz: number) {
	const key = `${rx},${rz}`;
	if (loading.has(key) || tiles.value.has(key)) return;
	loading.add(key);
	invoke<ArrayBuffer>("get_map_tile", {
		worldPath: props.worldPath,
		dimension: props.dimension,
		rx,
		rz,
		style: props.style,
	})
		.then(buf => {
			const blob = new Blob([buf], { type: "image/png" });
			const url = URL.createObjectURL(blob);
			const img = new Image();
			img.onload = () => {
				// URL 保留（drawImage 需要）；Map 覆盖旧图时统一 revoke
				tiles.value.set(key, img);
				loading.delete(key);
				invalidate();
			};
			img.src = url;
		})
		.catch(() => {
			// region 不存在/渲染失败：标记失败防重复请求
			loading.delete(key);
			failed.add(key);
		});
}

const failed = new Set<string>();

// ── 交互：拖拽平移 + 滚轮缩放（以指针为中心）──
let dragging = false;
let lastX = 0, lastY = 0;

function onPointerDown(e: PointerEvent) {
	dragging = true;
	lastX = e.clientX;
	lastY = e.clientY;
	(e.target as HTMLElement).setPointerCapture(e.pointerId);
}

function onPointerMove(e: PointerEvent) {
	if (!dragging) return;
	const scale = cam.value.scale;
	cam.value.cx -= (e.clientX - lastX) / scale;
	cam.value.cz -= (e.clientY - lastY) / scale;
	lastX = e.clientX;
	lastY = e.clientY;
	invalidate();
}

function onPointerUp() {
	dragging = false;
}

function onWheel(e: WheelEvent) {
	e.preventDefault();
	const rect = canvasRef.value!.getBoundingClientRect();
	// 指针下的世界坐标（缩放前后保持不变）
	const px = e.clientX - rect.left;
	const py = e.clientY - rect.top;
	const wx = cam.value.cx + (px - rect.width / 2) / cam.value.scale;
	const wz = cam.value.cz + (py - rect.height / 2) / cam.value.scale;
	const factor = e.deltaY < 0 ? 1.2 : 1 / 1.2;
	cam.value.scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, cam.value.scale * factor));
	// 反解中心使指针处世界坐标不动
	cam.value.cx = wx - (px - rect.width / 2) / cam.value.scale;
	cam.value.cz = wz - (py - rect.height / 2) / cam.value.scale;
	invalidate();
}

// ── 存档/维度/样式变化：清缓存重定位 ──
function resetForWorld() {
	for (const img of tiles.value.values()) {
		const src = img.src;
		URL.revokeObjectURL(src);
	}
	tiles.value.clear();
	loading.clear();
	failed.clear();
	invoke<{ x: number; z: number }[]>("list_map_regions", {
		worldPath: props.worldPath,
		dimension: props.dimension,
	})
		.then(list => {
			regions.value = list;
			if (list.length > 0) {
				// 包围盒中心定位
				let minX = Infinity, minZ = Infinity, maxX = -Infinity, maxZ = -Infinity;
				for (const r of list) {
					minX = Math.min(minX, r.x * 512);
					minZ = Math.min(minZ, r.z * 512);
					maxX = Math.max(maxX, r.x * 512 + 512);
					maxZ = Math.max(maxZ, r.z * 512 + 512);
				}
				cam.value.cx = (minX + maxX) / 2;
				cam.value.cz = (minZ + maxZ) / 2;
				cam.value.scale = Math.min(
					MAX_SCALE,
					Math.max(
						MIN_SCALE,
						Math.min(wrapRef.value!.clientWidth / (maxX - minX + 128), wrapRef.value!.clientHeight / (maxZ - minZ + 128)),
					),
				);
			}
			invalidate();
		})
		.catch(e => console.warn("list_map_regions:", errMsg(e)));
}

watch(
	() => [props.worldPath, props.dimension, props.style],
	() => resetForWorld(),
);

onMounted(() => {
	resetForWorld();
	raf = requestAnimationFrame(loop);
});

onUnmounted(() => {
	cancelAnimationFrame(raf);
	for (const img of tiles.value.values()) URL.revokeObjectURL(img.src);
});

/** 坐标显示（状态栏）。 */
const mouseWorld = ref({ x: 0, z: 0 });
function onMouseMove(e: MouseEvent) {
	const rect = canvasRef.value!.getBoundingClientRect();
	mouseWorld.value = {
		x: Math.round(cam.value.cx + (e.clientX - rect.left - rect.width / 2) / cam.value.scale),
		z: Math.round(cam.value.cz + (e.clientY - rect.top - rect.height / 2) / cam.value.scale),
	};
}

defineExpose({ mouseWorld });
</script>

<template>
	<div ref="wrapRef" class="mc" @pointerdown="onPointerDown" @pointermove="onPointerMove" @pointerup="onPointerUp" @wheel="onWheel" @mousemove="onMouseMove">
		<canvas ref="canvasRef" class="mc-canvas" />
		<div class="mc-zoom">×{{ cam.scale.toFixed(2) }}</div>
	</div>
</template>

<style scoped>
.mc {
	position: absolute;
	inset: 0;
	overflow: hidden;
	cursor: grab;
	touch-action: none;
}

.mc:active { cursor: grabbing; }

.mc-canvas { display: block; }

.mc-zoom {
	position: absolute;
	right: 10px;
	top: 10px;
	font-size: 11px;
	color: var(--text-tertiary);
	background: color-mix(in srgb, var(--surface-card) 80%, transparent);
	padding: 2px 8px;
	border-radius: 4px;
	pointer-events: none;
}
</style>
