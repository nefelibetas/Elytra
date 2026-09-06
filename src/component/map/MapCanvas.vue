<script setup lang="ts">
// 地图画布（Canvas 2D）：瓦片拼接 + 拖拽平移 + 滚轮缩放。
// 每个 region = 一张 512×512 后端渲染 PNG；瓦片按需加载，相机变换纯前端。
// 坐标系：世界方块坐标 ↔ 屏幕。scale=1 时 1 方块 = 1px（region 512px）。
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { errMsg } from "../../utils/error";
import { biomeLabel, blockLabel } from "../../utils/blockNames";
import LoadingSpinner from "../common/LoadingSpinner.vue";

const props = defineProps<{
	/** 世界根目录（含 level.dat）。 */
	worldPath: string;
	/** 维度目录名：'' 主世界 / DIM-1 / DIM1。 */
	dimension: string;
	/** 渲染样式：topdown 或 slice@Y。 */
	style: string;
	/** 预测未探索区（仅主世界 vanilla 语义；region 缺失槽位用预测瓦片补齐）。 */
	showPrediction?: boolean;
}>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const wrapRef = ref<HTMLDivElement | null>(null);

/** 相机：中心点（世界方块坐标）+ 缩放（px/方块）。×1.00 ⇔ 1 方块 = 1px。 */
const cam = ref({ cx: 0, cz: 0, scale: 1.0 });
const MIN_SCALE = 0.05;
const MAX_SCALE = 8;

/** 已加载瓦片：key = "rx,rz" → HTMLImageElement。 */
const tiles = ref<Map<string, HTMLImageElement>>(new Map());
/** 真实瓦片最大并发（每瓦片后端内部还有并行解析，防多瓦片洪泛拉满 CPU）。 */
const REAL_MAX_INFLIGHT = 2;
/** 真实瓦片在途计数。 */
let realInFlight = 0;
/** 真实瓦片排队集合（超限挂起；Map 值带启动所需参数，done 后主动排水）。 */
const realQueued = new Map<string, { rx: number; rz: number; mtime: number }>();
/** 真实瓦片连续失败计数 / 冷却截止（ms）——防失败风暴空转 CPU。 */
const realFails = new Map<string, number>();
const realCooldown = new Map<string, number>();

/** 错误收集：console + window.__mapTileErrors（远程诊断用，上限 100）。 */
function reportMapError(msg: string, detail?: unknown) {
	console.error("[MapCanvas]", msg, detail ?? "");
	try {
		const g = window as unknown as { __mapTileErrors?: string[] };
		g.__mapTileErrors ??= [];
		g.__mapTileErrors.push(`${new Date().toISOString()} ${msg} ${detail !== undefined ? JSON.stringify(detail) : ""}`);
		if (g.__mapTileErrors.length > 100) g.__mapTileErrors.shift();
	} catch {
		// 记录失败不影响主流程
	}
}

/** invoke 二进制载荷归一为 PNG Blob：tauri ipc::Response 经 invoke 到达 JS 时
 *  大载荷走二进制通道（ArrayBuffer），小载荷退化为 JSON 数字数组（实测
 *  预测瓦片 ~12KB 必走数组）——直接 `new Blob([arr])` 会被字符串化导致
 *  image decode failed，统一转 Uint8Array 再入 Blob。 */
function pngBlob(payload: unknown): Blob {
	const u8: Uint8Array = payload instanceof ArrayBuffer
		? new Uint8Array(payload)
		: Array.isArray(payload)
			? Uint8Array.from(payload as number[])
			: new Uint8Array(payload as ArrayBufferLike);
	// TS6 TypedArray 泛型：显式以 ArrayBuffer 视图包装满足 BlobPart
	return new Blob([new Uint8Array(u8.buffer as ArrayBuffer)], { type: "image/png" });
}
/** 预测瓦片（未探索区）：key = "worldPath|rx,rz"（跨世界切换防串图）。 */
const predTiles = new Map<string, HTMLImageElement>();
/** 预测瓦片在途集合。 */
const predLoading = new Set<string>();
/** 预测瓦片最大并发（1：工作线程半核跑，避免 CPU 长期打满）。 */
const PRED_MAX_INFLIGHT = 1;
/** 当前在途预测请求数。 */
let predInFlight = 0;
/** 以已加载 region 为界的外扩光环半径（region 单位）。3 ≈ 1536 格：
 *  1.0 倍率下视口最远边缘距加载边界 ≤2 region，留 1 环余量全屏可预测。 */
const PRED_HALO = 3;
/** 已加载 region 包围盒（region 坐标，每帧实时），光环判定用。 */
const realBounds = { minX: 0, maxX: -1, minZ: 0, maxZ: -1 };

/** slot 到已加载包围盒的 Chebyshev 距离（内部=0）。 */
function distToReal(rx: number, rz: number): number {
	if (realBounds.maxX < realBounds.minX) return Number.POSITIVE_INFINITY;
	const dx = Math.max(0, realBounds.minX - rx, rx - realBounds.maxX);
	const dz = Math.max(0, realBounds.minZ - rz, rz - realBounds.maxZ);
	return Math.max(dx, dz);
}
/** 预测瓦片对应的 region mtime（轮询时检测变更：变了才重取）。 */
const tileMtimes = new Map<string, number>();
/** 加载中集合（防重复请求）。 */
const loading = new Set<string>();
/** 在途瓦片计数（驱动单一整体加载指示）。 */
const pendingTiles = ref(0);
/** 视口内 region 列表（含 mtime/full；轮询增量同步，游戏边玩边出图）。 */
const regions = ref<{ x: number; z: number; mtime: number; full: boolean }[]>([]);
/** 首次定位标记：仅在首个 region 列表到达时做包围盒居中，轮询不打断用户视角。 */
let centered = false;
/** 画布状态：渲染中 / 空维度 / 就绪（驱动等待覆盖层，避免加载期空白）。 */
const viewState = ref<"loading" | "empty" | "ready">("loading");
/** 是否已画出至少一块瓦片（首块就位后 loading 覆盖层撤下）。 */
let firstTileShown = false;
/** region 轮询定时器。 */
let pollTimer: number | undefined;
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
	const live = new Set<string>();
	const fullKeys = new Set<string>();
	for (const r of regions.value) {
		live.add(`${r.x},${r.z}`);
		if (r.full) fullKeys.add(`${r.x},${r.z}`);
	}
	// 预测层：画预测底图。加载策略 = 有界光环（距已加载区 ≤PRED_HALO 逐环填充，
	// 更外层视口内懒加载）。真实槽位规则：满 region 跳过（无空洞）；**未满 region
	// 也画预测垫底** —— 已生成 chunk 的真实瓦片不透明地盖在预测上，洞即预测。
	if (props.showPrediction && props.dimension === "") {
		// 视口外扩 1 region 作为预取边距：光环内槽位未进屏即开算，
		// 平移进入时已就绪（1.0 倍率下贴边连续平移不露白）
		const r0x = Math.floor(minX / 512) - 1;
		const r1x = Math.floor(maxX / 512) + 1;
		const r0z = Math.floor(minZ / 512) - 1;
		const r1z = Math.floor(maxZ / 512) + 1;
		const v0x = Math.floor(minX / 512);
		const v1x = Math.floor(maxX / 512);
		const v0z = Math.floor(minZ / 512);
		const v1z = Math.floor(maxZ / 512);
		const spanOk = (r1x - r0x + 1) * (r1z - r0z + 1) <= 4096;
		let budget = PRED_MAX_INFLIGHT - predInFlight;
		const halo: { rx: number; rz: number; d: number; d2: number }[] = [];
		const lazy: { rx: number; rz: number; d2: number }[] = [];
		for (let rx = r0x; rx <= r1x; rx++) {
			for (let rz = r0z; rz <= r1z; rz++) {
				const slotKey = `${rx},${rz}`;
				if (live.has(slotKey) && fullKeys.has(slotKey)) continue;
				const ox = rx * 512, oz = rz * 512;
				const pkey = `${props.worldPath}|${slotKey}`;
				const img = predTiles.get(pkey);
				if (img) {
					ctx.imageSmoothingEnabled = scale < 1;
					ctx.drawImage(img, toScreenX(ox), toScreenY(oz), size, size);
					continue;
				}
				if (!spanOk || budget <= 0) continue;
				const d2 = (ox + 256 - cx) * (ox + 256 - cx) + (oz + 256 - cz) * (oz + 256 - cz);
				const d = distToReal(rx, rz);
				if (d <= PRED_HALO) {
					halo.push({ rx, rz, d, d2 });
				} else if (rx >= v0x && rx <= v1x && rz >= v0z && rz <= v1z) {
					// 光环外：仅视口内才懒加载
					lazy.push({ rx, rz, d2 });
				}
			}
		}
		// 光环内：距离近的先填（逐环外扩观感）；光环外懒加载按中心就近
		halo.sort((a, b) => a.d - b.d || a.d2 - b.d2);
		lazy.sort((a, b) => a.d2 - b.d2);
		for (const s of [...halo, ...lazy]) {
			if (budget <= 0) break;
			if (predLoading.has(`${s.rx},${s.rz}`)) continue;
			requestPredTile(s.rx, s.rz);
			budget--;
		}
	}
	for (const r of regions.value) {
		const ox = r.x * 512, oz = r.z * 512; // region 原点（世界方块）
		if (ox + 512 < minX || ox > maxX || oz + 512 < minZ || oz > maxZ) continue;
		const key = `${r.x},${r.z}`;
		const img = tiles.value.get(key);
		if (!img) {
			requestTile(r.x, r.z, r.mtime);
			drawPendingRegion(ctx, toScreenX(ox), toScreenY(oz), size);
			continue;
		}
		ctx.imageSmoothingEnabled = scale < 1;
		ctx.drawImage(img, toScreenX(ox), toScreenY(oz), size, size);
		firstTileShown = true;
	}
	// 整体加载态：首块瓦片落地前保持等待覆盖层；落地后即便还有瓦片在途
	// 也转为就绪——剩余瓦片渐进浮现（避免慢瓦片拖住整屏 loading）
	if (pendingTiles.value > 0 && regions.value.length > 0 && viewState.value !== "empty" && !firstTileShown) {
		viewState.value = "loading";
	} else if (firstTileShown && viewState.value !== "ready") {
		viewState.value = "ready";
	}
	// 方块网格：≥×2 时画世界方块边界线（悬停取块/核对坐标的瞄准参考）
	if (scale >= 2) {
		drawBlockGrid(ctx, w, h, cx, cz, scale, toScreenX, toScreenY);
	}
}

/** 世界方块网格：每个整数方块坐标一条细线（对齐 cam 原点，无漂移）。 */
function drawBlockGrid(
	ctx: CanvasRenderingContext2D,
	w: number,
	h: number,
	cx: number,
	cz: number,
	scale: number,
	sx: (wx: number) => number,
	sy: (wz: number) => number,
) {
	const halfW = w / 2 / scale;
	const halfH = h / 2 / scale;
	ctx.strokeStyle = "rgba(255, 255, 255, 0.06)";
	ctx.lineWidth = 1;
	ctx.beginPath();
	for (let gx = Math.floor(cx - halfW); gx <= Math.ceil(cx + halfW); gx++) {
		const px = sx(gx);
		if (px < 0 || px > w) continue;
		ctx.moveTo(px, 0);
		ctx.lineTo(px, h);
	}
	for (let gz = Math.floor(cz - halfH); gz <= Math.ceil(cz + halfH); gz++) {
		const py = sy(gz);
		if (py < 0 || py > h) continue;
		ctx.moveTo(0, py);
		ctx.lineTo(w, py);
	}
	ctx.stroke();
}

/** 未加载瓦片的占位：仅半透明底（无动画——加载指示收敛到单个整体层）。 */
function drawPendingRegion(ctx: CanvasRenderingContext2D, x: number, y: number, size: number) {
	if (size <= 0) return;
	ctx.fillStyle = "rgba(16, 18, 24, 0.42)";
	ctx.fillRect(x, y, size, size);
}
/** 请求瓦片（后端缓存键含 region mtime；前端 mtime 戳用于轮询变更检测）。
 *  settle 保证：成功/解码失败/超时（25s）都必然解除占用并触发重试，
 *  连续失败进入 10s 冷却，杜绝"永久 pending → 全屏占位卡死"。 */
function requestTile(rx: number, rz: number, mtime: number) {
	const key = `${rx},${rz}`;
	if (loading.has(key) || tiles.value.has(key) || realQueued.has(key)) return;
	const cdUntil = realCooldown.get(key) ?? 0;
	if (Date.now() < cdUntil) return;
	if (realInFlight >= REAL_MAX_INFLIGHT) {
		realQueued.set(key, { rx, rz, mtime });
		return;
	}
	startRealTile(rx, rz, mtime);
}

/** 真实瓦片请求主体（调用方已保证有并发额度）。 */
function startRealTile(rx: number, rz: number, mtime: number) {
	const key = `${rx},${rz}`;
	realQueued.delete(key);
	loading.add(key);
	pendingTiles.value++;
	realInFlight++;
	let settled = false;
	const failCount = realFails.get(key) ?? 0;
	const done = (kind: "ok" | "err" | "timeout", detail?: unknown) => {
		if (settled) return;
		settled = true;
		clearTimeout(watchdog);
		loading.delete(key);
		pendingTiles.value = Math.max(0, pendingTiles.value - 1);
		realInFlight = Math.max(0, realInFlight - 1);
		if (kind !== "ok") {
			realFails.set(key, failCount + 1);
			if (failCount >= 3) realCooldown.set(key, Date.now() + 10000);
			reportMapError(`real tile ${key}: ${kind}`, detail);
		} else {
			realFails.delete(key);
		}
		// 主动排水：队列非空且有空位时立即启动下一个（不能只靠 invalidate）
		const nxt = realQueued.entries().next().value;
		if (nxt) {
			const { rx: nrx, rz: nrz, mtime: nmtime } = nxt[1];
			realQueued.delete(nxt[0]);
			if (!loading.has(nxt[0]) && !tiles.value.has(nxt[0])) {
				startRealTile(nrx, nrz, nmtime);
			}
		}
		invalidate();
	};
	// 兜底：任何路径漏 settle 都不允许永久占用（解码/回调异常均覆盖）
	const watchdog = window.setTimeout(() => done("timeout", "no settle in 25s"), 25000);
	invoke<ArrayBuffer>("get_map_tile", {
		worldPath: props.worldPath,
		dimension: props.dimension,
		rx,
		rz,
		style: props.style,
	})
		.then(buf => {
			const blob = pngBlob(buf);
			const url = URL.createObjectURL(blob);
			const img = new Image();
			img.onload = () => {
				// 旧图（region 更新前的残留）在此被覆盖并 revoke——先到后到都收敛到最新
				try {
					const prev = tiles.value.get(key);
					if (prev) URL.revokeObjectURL(prev.src);
					tiles.value.set(key, img);
					tileMtimes.set(key, mtime);
				} catch (e) {
					done("err", e);
					return;
				}
				done("ok");
			};
			img.onerror = () => {
				URL.revokeObjectURL(url);
				done("err", "image decode failed");
			};
			img.src = url;
		})
		.catch((e: unknown) => {
			done("err", e);
		});
}

/** 请求预测瓦片（主世界未探索区；后端磁盘缓存；并发由调用方预算控制）。
 *  与真实瓦片同款 settle 保证 + 超时兜底。 */
function requestPredTile(rx: number, rz: number) {
	const slotKey = `${rx},${rz}`;
	const pkey = `${props.worldPath}|${slotKey}`;
	if (predLoading.has(slotKey) || predTiles.has(pkey)) return;
	predLoading.add(slotKey);
	predInFlight++;
	let settled = false;
	const done = (kind: "ok" | "err" | "timeout", detail?: unknown) => {
		if (settled) return;
		settled = true;
		clearTimeout(watchdog);
		predLoading.delete(slotKey);
		predInFlight = Math.max(0, predInFlight - 1);
		if (kind !== "ok") {
			reportMapError(`pred tile ${slotKey}: ${kind}`, detail);
		}
		invalidate();
	};
	// 兜底：冷算（debug 后端渲染可达 40~60s/张）不应触发误报超时；90s 兜底
	const watchdog = window.setTimeout(() => done("timeout", "no settle in 90s"), 90000);
	invoke<ArrayBuffer>("get_prediction_tile", {
		worldPath: props.worldPath,
		dimension: props.dimension,
		rx,
		rz,
	})
		.then(buf => {
			const blob = pngBlob(buf);
			const url = URL.createObjectURL(blob);
			const img = new Image();
			img.onload = () => {
				try {
					const prev = predTiles.get(pkey);
					if (prev) URL.revokeObjectURL(prev.src);
					predTiles.set(pkey, img);
				} catch (e) {
					done("err", e);
					return;
				}
				done("ok");
			};
			img.onerror = () => {
				URL.revokeObjectURL(url);
				done("err", "image decode failed");
			};
			img.src = url;
		})
		.catch((e: unknown) => {
			done("err", e);
		});
}

/** 拉取 region 列表并增量同步瓦片：新增/mtime 变化的重取，已消失的移除。
 *  轮询调用（3s）：游戏运行中边玩边渲染；存档未变时后端磁盘缓存直接命中。 */
async function refreshRegions() {
	let list: { x: number; z: number; mtime: number; full: boolean }[];
	try {
		list = await invoke<{ x: number; z: number; mtime: number; full: boolean }[]>(
			"list_map_regions",
			{
				worldPath: props.worldPath,
				dimension: props.dimension,
			},
		);
	} catch (e) {
		console.warn("list_map_regions:", errMsg(e));
		return;
	}
	regions.value = list;
	// 光环判定用的已加载包围盒（region 坐标）
	if (list.length === 0) {
		realBounds.minX = 0;
		realBounds.maxX = -1;
		realBounds.minZ = 0;
		realBounds.maxZ = -1;
	} else {
		realBounds.minX = Infinity;
		realBounds.maxX = -Infinity;
		realBounds.minZ = Infinity;
		realBounds.maxZ = -Infinity;
		for (const r of list) {
			realBounds.minX = Math.min(realBounds.minX, r.x);
			realBounds.maxX = Math.max(realBounds.maxX, r.x);
			realBounds.minZ = Math.min(realBounds.minZ, r.z);
			realBounds.maxZ = Math.max(realBounds.maxZ, r.z);
		}
	}
	if (list.length === 0) {
		// 空维度（该存档未生成此维度区块）：给明确提示而非死空白
		viewState.value = "empty";
	} else if (!firstTileShown) {
		// 首块瓦片未就位：保持等待覆盖层（新区域加载/切换维度时）
		viewState.value = "loading";
	}
	// 首次（每个 存档/维度/样式 组合）：视角中心对准世界原点 (0,0)，倍率 ×4.00
	// （1 方块 = 4px，块面清晰）。坐标芯片因此开档即显示中心 X 0 Z 0；后续轮询保持用户视角
	if (!centered && list.length > 0 && wrapRef.value) {
		centered = true;
		cam.value.cx = 0;
		cam.value.cz = 0;
		cam.value.scale = 1.0;
	}
	const live = new Set<string>();
	for (const r of list) {
		const key = `${r.x},${r.z}`;
		live.add(key);
		if (tiles.value.has(key) && tileMtimes.get(key) === r.mtime) continue;
		requestTile(r.x, r.z, r.mtime);
	}
	for (const key of [...tiles.value.keys()]) {
		if (live.has(key)) continue;
		URL.revokeObjectURL(tiles.value.get(key)!.src);
		tiles.value.delete(key);
		tileMtimes.delete(key);
	}
	// 本轮列表出现的新**满**真实 region → 吊销同槽过期预测图（无洞无需垫底；
	// 未满 region 保留预测垫底，chunk 洞由预测填充——覆盖仍由 draw 的 live 检查保证）
	const fullLive = new Set<string>();
	for (const r of list) {
		if (r.full) fullLive.add(`${r.x},${r.z}`);
	}
	const prefix = `${props.worldPath}|`;
	for (const key of [...predTiles.keys()]) {
		if (!key.startsWith(prefix)) continue;
		const slot = key.slice(prefix.length);
		if (fullLive.has(slot)) {
			URL.revokeObjectURL(predTiles.get(key)!.src);
			predTiles.delete(key);
			predLoading.delete(slot);
		}
	}
	invalidate();
}


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

// ── 存档/维度/样式变化：清缓存重定位（轮询居中标记同步复位）──
function resetForWorld() {
	for (const img of tiles.value.values()) URL.revokeObjectURL(img.src);
	tiles.value.clear();
	tileMtimes.clear();
	loading.clear();
	realQueued.clear();
	realInFlight = 0;
	realFails.clear();
	realCooldown.clear();
	// 仅清非当前世界的预测瓦片（同世界复用缓存图片；维度切换世界路径不变）
	for (const [k, img] of [...predTiles]) {
		if (!k.startsWith(`${props.worldPath}|`)) {
			URL.revokeObjectURL(img.src);
			predTiles.delete(k);
			predLoading.delete(k.slice(k.indexOf("|") + 1));
		}
	}
	centered = false;
	firstTileShown = false;
	viewState.value = "loading";
	void refreshRegions();
}

watch(
	() => [props.worldPath, props.dimension, props.style],
	() => resetForWorld(),
);

onMounted(() => {
	resetForWorld();
	raf = requestAnimationFrame(loop);
	// 存档实时性轮询：游戏运行中新建/更新的 region 自动出图（不可见时跳过）
	pollTimer = window.setInterval(() => {
		if (document.visibilityState !== "visible") return;
		void refreshRegions();
	}, 3000);
});


onUnmounted(() => {
	cancelAnimationFrame(raf);
	clearInterval(pollTimer);
	for (const img of tiles.value.values()) URL.revokeObjectURL(img.src);
	for (const img of predTiles.values()) URL.revokeObjectURL(img.src);
	predTiles.clear();
	predLoading.clear();
});

/** 悬停世界坐标（未悬停为 null → 芯片显示视角中心坐标，常驻不消失）。 */
const mouseWorld = ref<{ x: number | null; z: number | null }>({ x: null, z: null });
/** 悬停方块名（取回前为 null）。 */
const hoverBlock = ref<string | null>(null);
/** 悬停所在群系名（与方块同一请求返回）。 */
const hoverBiome = ref<string | null>(null);
/** 最近一次悬停方块坐标（与已请求坐标比对，防重复/防漏请求）。 */
let lastHover = { x: 0, z: 0, valid: false };
let hoverFetching = false;
let hoverRequested = { x: 0, z: 0 };

function maybeFetchBlock(x: number, z: number) {
	lastHover = { x, z, valid: true };
	if (hoverFetching) return;
	if (hoverRequested.x === x && hoverRequested.z === z && hoverBlock.value !== null) return;
	hoverRequested = { x, z };
	hoverFetching = true;
	hoverBlock.value = null;
	hoverBiome.value = null;
	// 切片模式带当前 Y 层：方块与群系都按该层精确探测
	const sliceY = props.style.startsWith("slice@") ? Number(props.style.slice(6)) : null;
	invoke<{ block: string | null; biome: string | null } | null>("get_map_block", {
		worldPath: props.worldPath,
		dimension: props.dimension,
		x,
		z,
		...(sliceY !== null && !Number.isNaN(sliceY) ? { y: sliceY } : {}),
	})
		.then(info => {
			// 只展示最新悬停位置的结果（快速扫过时丢弃过期响应）
			if (lastHover.valid && lastHover.x === x && lastHover.z === z) {
				// 真实数据缺失时的预测回退：block=null 且 biome=Some → 预测群系。
				// 明确标注"预测区"，避免把"水陆未精画"误读为渲染错误
				// （预测仅到 4×4 群系单元粒度，群系内的湖/湾/浅水无法体现）。
				const isPrediction = !info?.block && !!info?.biome;
				hoverBlock.value = isPrediction
					? "预测区"
					: info?.block
						? blockLabel(info.block)
						: "未知方块";
				hoverBiome.value = info?.biome ? biomeLabel(info.biome) : null;
			}
		})
		.catch(() => {
			if (lastHover.valid && lastHover.x === x && lastHover.z === z) {
				hoverBlock.value = "未知方块";
				hoverBiome.value = null;
			}
		})
		.finally(() => {
			hoverFetching = false;
			if (lastHover.valid && (lastHover.x !== hoverRequested.x || lastHover.z !== hoverRequested.z)) {
				maybeFetchBlock(lastHover.x, lastHover.z);
			}
		});
}

/** 芯片展示值：悬停坐标优先，离开画布回退视角中心（拖拽平移/滚轮缩放后同步）。 */
const shownCoords = computed(() => {
	const h = mouseWorld.value;
	if (h.x !== null && h.z !== null) {
		return { x: h.x, z: h.z, center: false };
	}
	return { x: Math.round(cam.value.cx), z: Math.round(cam.value.cz), center: true };
});
function onMouseMove(e: MouseEvent) {
	const rect = canvasRef.value!.getBoundingClientRect();
	const x = Math.round(cam.value.cx + (e.clientX - rect.left - rect.width / 2) / cam.value.scale);
	const z = Math.round(cam.value.cz + (e.clientY - rect.top - rect.height / 2) / cam.value.scale);
	mouseWorld.value = { x, z };
	maybeFetchBlock(x, z);
}
function onMouseLeave() {
	mouseWorld.value = { x: null, z: null };
	lastHover.valid = false;
	hoverBlock.value = null;
	hoverBiome.value = null;
}

defineExpose({ mouseWorld });
</script>

<template>
	<div ref="wrapRef" class="mc" @pointerdown="onPointerDown" @pointermove="onPointerMove" @pointerup="onPointerUp" @wheel="onWheel" @mousemove="onMouseMove" @mouseleave="onMouseLeave">
		<canvas ref="canvasRef" class="mc-canvas" />
		<Transition name="fade" mode="out-in">
			<div v-if="viewState === 'loading'" key="loading" class="mc-overlay">
				<LoadingSpinner text="地图渲染中…" />
			</div>
			<div v-else-if="viewState === 'empty'" key="empty" class="mc-overlay mc-empty">
				<div>该维度暂无区块数据</div>
				<span>进入该维度游玩后自动出现</span>
			</div>
		</Transition>
		<div class="mc-coords" :class="{ center: shownCoords.center }">
			<template v-if="shownCoords.center">X {{ shownCoords.x }}　Z {{ shownCoords.z }}</template>
			<template v-else>
				X {{ shownCoords.x }}　Z {{ shownCoords.z }}
				<span v-if="hoverBlock" class="mc-block">· {{ hoverBlock }}</span>
				<span v-if="hoverBiome" class="mc-biome">· {{ hoverBiome }}</span>
			</template>
		</div>
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

.mc-overlay {
	position: absolute;
	inset: 0;
	display: flex;
	align-items: center;
	justify-content: center;
	pointer-events: none;
}

.mc-empty {
	flex-direction: column;
	gap: 4px;
	font-size: 12px;
	color: var(--text-secondary);
	line-height: 1.6;
	text-align: center;
}

.mc-empty span {
	font-size: 11px;
	color: var(--text-tertiary);
}

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

.mc-coords {
	position: absolute;
	left: 50%;
	bottom: 10px;
	transform: translateX(-50%);
	font-size: 11px;
	color: var(--text-secondary);
	background: color-mix(in srgb, var(--surface-card) 82%, transparent);
	padding: 3px 10px;
	border-radius: 5px;
	pointer-events: none;
	font-variant-numeric: tabular-nums;
	letter-spacing: 0.02em;
	white-space: nowrap;
}

/* 显示视角中心（未悬停）时弱化，与悬停坐标区分 */
.mc-coords.center {
	color: var(--text-tertiary);
	opacity: 0.72;
}

/* 悬停方块名：品牌色区分于坐标 */
.mc-block {
	color: var(--color-brand);
	margin-left: 6px;
}

/* 悬停群系名：弱一档的品牌色，与方块名区分 */
.mc-biome {
	color: color-mix(in srgb, var(--color-brand) 68%, var(--text-secondary));
	margin-left: 6px;
}
</style>
