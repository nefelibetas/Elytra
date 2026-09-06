<script setup lang="ts">
// 地图编辑器窗口（label=map-editor）：左侧三级树（游戏目录→版本→存档）+ 右侧地图画布。
// 独立 WebView 数据自治：初始选中经 URL query 传入，树/图标自行 invoke 拉取；
// 标题栏自绘（无边框窗口），窗口创建与聚焦在 store/mapEditor.ts 编排。
import { computed, onMounted, onUnmounted, ref } from "vue";
import { ChevronRight, Folder, FolderOpen, Map, Minus, X, RefreshCw, AlertCircle } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { LOADER_TITLES } from "../utils/versionMeta";
import { fmtUnix } from "../utils/format";
import {
	mapTree, treeLoading, treeError, selected, expanded,
	toggleExpand, loadTree, pollTree, initFromQuery, applySelection, selectedWorldPath, syncWindowTitle,
} from "../store/mapEditor";
import { showToast } from "../utils/toast";
import MapCanvas from "../component/map/MapCanvas.vue";

const appWindow = getCurrentWindow();

function minimize() { void appWindow.minimize(); }
function closeWindow() { void appWindow.close(); }

function onTitleBarMouseDown(e: MouseEvent) {
	const target = e.target as HTMLElement;
	if (target.closest("a, button")) return;
	void appWindow.startDragging();
}

/** 目录级展开键。 */
function dirKey(path: string) { return path; }
/** 版本级展开键。 */
function verKey(dirPath: string, version: string) { return `${dirPath}|${version}`; }

function selectWorld(dirPath: string, version: string, world: string) {
	selected.dirPath = dirPath;
	selected.version = version;
	selected.world = world;
	syncWindowTitle();
}

/** 窗口标题跟随模式：种子模式显示种子号。 */
function windowTitleForMode() {
	if (mode.value === "seed" && seedReady.value) {
		void appWindow.setTitle(`种子 ${seedViewSeed.value} · Elytra 地图`);
	} else if (mode.value === "save") {
		syncWindowTitle();
	}
}

/** 查看器模式：存档（三级树选世界）或种子（无存档直开）。 */
const mode = ref<"save" | "seed">("save");
/** 种子输入框文本（数字或 Java 字符串哈希均可）。 */
const seedInput = ref("");
/** 已应用生效的种子（十进制 i64 字符串，可为超 2^53 大数；null = 未开图）。 */
const seedViewSeed = ref<string | null>(null);
/** 种子模式地图可用 = 已解析种子。 */
const seedReady = computed(() => seedViewSeed.value !== null);

/** Java String.hashCode（32 位有符号）——MC 对非数字种子串的哈希规则。 */
function javaSeedHash(text: string): number {
	let h = 0;
	for (let i = 0; i < text.length; i++) {
		h = (h * 31 + text.charCodeAt(i)) | 0;
	}
	return h;
}

/** 解析种子输入 → 十进制 i64 字符串：数字原样（不做 Number 中转，防丢精度），
 *  非数字走 Java 哈希。 */
function parseSeedText(text: string): string | null {
	const t = text.trim();
	if (!t) return null;
	if (/^-?\d+$/.test(t)) return t;
	return String(javaSeedHash(t));
}

/** 应用种子并开图（切种子模式）。 */
function openSeedMap() {
	const seed = parseSeedText(seedInput.value);
	if (seed === null) {
		showToast(seedInput.value.trim() ? "种子无效" : "请输入种子");
		return;
	}
	mode.value = "seed";
	seedViewSeed.value = seed;
	dimension.value = "";
	sliceY.value = null;
	windowTitleForMode();
}

/** 切回存档模式。 */
function openSaveMode() {
	mode.value = "save";
	windowTitleForMode();
}

const hasSelection = computed(() => Boolean(selected.world));
const worldPath = computed(() => selectedWorldPath() ?? "");
/** 画布显示条件：种子模式看种子地图，否则看选中存档。 */
const showCanvas = computed(() => (mode.value === "seed" ? seedReady.value : hasSelection.value));

// ── 查看器状态：维度 + 渲染样式 ──
const dimension = ref("");
const DIMENSIONS = [
	{ key: "", label: "主世界" },
	{ key: "DIM-1", label: "下界" },
	{ key: "DIM1", label: "末地" },
] as const;
const sliceY = ref<number | null>(null);
/** 预测层开关（仅主世界生效；dimension 切换时自动隐藏但保留选择）。 */
const predictOn = ref(false);
/** 当前样式串：topdown 或 slice@Y（滑杆拖动即切切片模式）。 */
const style = computed(() => (sliceY.value === null ? "topdown" : `slice@${sliceY.value}`));
function setTopDown() {
	sliceY.value = null;
}
function setSlice() {
	if (sliceY.value === null) sliceY.value = 62; // 首次切切片：默认海平面
}

// ── 存档缩略图（key = dirPath|version|world 防跨版本重名覆盖）──
const worldIcons = ref<Record<string, string>>({});

async function loadWorldIcon(dirPath: string, version: string, world: string) {
	const key = `${dirPath}|${version}|${world}`;
	if (worldIcons.value[key]) return;
	try {
		// 与主窗口 get_world_icon 不同：编辑器树以目录路径锚定（非激活目录），
		// 复用后端魔数嗅探逻辑（icon_payload）按绝对路径读取
		const icon = await invoke<{ mime: string; data: string } | null>("get_map_world_icon", {
			dirPath, versionName: version, worldName: world,
		});
		if (icon) worldIcons.value[key] = `data:${icon.mime};base64,${icon.data}`;
	} catch {
		// 缺失/读取失败：保持 Map 占位
	}
}

/** 预取全树存档图标（数量级：目录×版本×存档，本地读取廉价；渲染接入后改按需）。 */
function prefetchIcons() {
	for (const d of mapTree.value) {
		for (const v of d.versions) {
			for (const w of v.worlds) void loadWorldIcon(d.path, v.name, w.name);
		}
	}
}

onMounted(async () => {
	// 主窗口对已开启编辑器发起新定位（openMapEditorWindow 复用窗口分支）
	const unlisten = await appWindow.listen<{ version: string; world: string }>(
		"map-editor:navigate",
		e => {
			selected.version = e.payload.version;
			selected.world = e.payload.world;
			selected.dirPath = "";
			applySelection();
			syncWindowTitle();
		},
	);
	onUnmounted(unlisten);

	// 深链：mode=seed&seed=…（种子查看器直开，不依赖存档树）
	const query = new URLSearchParams(location.hash.split("?")[1] ?? "");
	if (query.get("mode") === "seed") {
		seedInput.value = query.get("seed") ?? "";
		await loadTree();
		openSeedMap();
		windowTitleForMode();
		return;
	}

	await loadTree();
	initFromQuery();
	applySelection();
	prefetchIcons();

	// 存档实时性轮询：游戏边玩边建/删存档时树自动更新（本地 readdir，廉价；
	// 窗口不可见时跳过）。选中存档被删除时 pollTree 会清空选中。
	const poll = window.setInterval(() => {
		if (document.visibilityState !== "visible") return;
		void pollTree().then(prefetchIcons);
	}, 3000);
	onUnmounted(() => clearInterval(poll));
});

/** 刷新树（手动按钮；与轮询同路径：保留选中与展开，选中已删除则清空）。 */
async function refreshTree() {
	await pollTree();
	prefetchIcons();
}

/** 打开存档所在文件夹（复用主窗口 open_folder 后端命令）。 */
async function openWorldFolder() {
	if (!worldPath.value) return;
	try {
		await invoke("open_folder", { path: worldPath.value });
	} catch (e) {
		showToast(String(e).split("\n")[0]);
	}
}
</script>

<template>
	<div class="me">
		<!-- 自绘标题栏（精简版：标题居左，最小化/关闭居右） -->
		<div class="me-titlebar" @mousedown="onTitleBarMouseDown">
			<Map :size="17" class="me-tb-icon" />
			<span class="me-tb-title">{{
				mode === "seed" && seedViewSeed !== null
					? `种子 ${seedViewSeed}`
					: selected.world || "Elytra 地图"
			}}</span>
			<div class="me-tb-actions">
				<button class="me-tb-btn" title="最小化" @click="minimize"><Minus :size="16" /></button>
				<button class="me-tb-btn me-tb-close" title="关闭" @click="closeWindow"><X :size="16" /></button>
			</div>
		</div>

		<div class="me-body">
			<!-- 左侧：模式切换 + 存档三级树 / 种子输入 -->
			<aside class="me-side">
				<div class="me-mode-tabs">
					<button class="me-mode-tab" :class="{ active: mode === 'save' }" @click="openSaveMode">
						存档
					</button>
					<button class="me-mode-tab" :class="{ active: mode === 'seed' }" @click="mode = 'seed'">
						种子地图
					</button>
				</div>

				<template v-if="mode === 'save'">
					<div class="me-side-head">
						<span class="me-side-title">存档</span>
						<button class="me-side-refresh" title="刷新" @click="refreshTree">
							<RefreshCw :size="15" :class="{ spinning: treeLoading }" />
						</button>
					</div>

					<AlertCircle v-if="treeError" :size="22" class="me-tree-err-icon" />
					<div v-if="treeError" class="me-tree-err">
						{{ treeError }}
						<button class="me-tree-retry" @click="loadTree">重试</button>
					</div>
					<div v-else-if="treeLoading && mapTree.length === 0" class="me-tree-loading">加载存档树…</div>
					<div v-else-if="mapTree.length === 0" class="me-tree-empty">未配置游戏目录</div>

					<div v-else class="me-tree">
						<template v-for="dir in mapTree" :key="dir.path">
							<!-- 1 级：游戏目录（箭头常驻，展开时旋转 90° + 文件夹开/合） -->
							<button
								class="me-node me-dir"
								:class="{ expanded: expanded[dirKey(dir.path)] }"
								@click="toggleExpand(dirKey(dir.path))"
							>
								<ChevronRight
									:size="15"
									class="me-caret"
									:class="{ open: expanded[dirKey(dir.path)] }"
								/>
								<FolderOpen v-if="expanded[dirKey(dir.path)]" :size="16" class="me-dir-icon" />
								<Folder v-else :size="16" class="me-dir-icon" />
								<span class="me-node-name">{{ dir.name }}</span>
							</button>

							<template v-if="expanded[dirKey(dir.path)]">
								<template v-for="ver in dir.versions" :key="dir.path + '|' + ver.name">
									<!-- 2 级：版本（箭头常驻旋转；无存档展开后无内容行） -->
									<button
										class="me-node me-ver"
										:class="{ expanded: expanded[verKey(dir.path, ver.name)] }"
										@click="toggleExpand(verKey(dir.path, ver.name))"
									>
										<ChevronRight
											:size="13"
											class="me-caret"
											:class="{ open: expanded[verKey(dir.path, ver.name)] }"
										/>
										<span class="me-node-name">{{ ver.name }}</span>
										<span v-if="LOADER_TITLES[ver.loader]" class="me-ver-badge">{{ LOADER_TITLES[ver.loader] }}</span>
									</button>

									<template v-if="expanded[verKey(dir.path, ver.name)]">
										<!-- 3 级：存档 -->
										<div
											v-for="w in ver.worlds"
											:key="dir.path + '|' + ver.name + '|' + w.name"
											class="me-node me-world"
											:class="{ active: selected.dirPath === dir.path && selected.version === ver.name && selected.world === w.name }"
											@click="selectWorld(dir.path, ver.name, w.name)"
										>
											<img
												v-if="worldIcons[`${dir.path}|${ver.name}|${w.name}`]"
												:src="worldIcons[`${dir.path}|${ver.name}|${w.name}`]"
												class="me-world-thumb"
												alt=""
											/>
											<Map v-else :size="14" class="me-world-icon" />
											<span class="me-node-name">{{ w.name }}</span>
											<span class="me-world-meta">{{ fmtUnix(w.last_modified) }}</span>
										</div>
									</template>
								</template>
							</template>
						</template>
					</div>
				</template>

				<div v-else class="me-seed-panel">
					<p class="me-seed-hint">输入种子直接查看未生成世界的地图（无需存档）</p>
					<input
						v-model="seedInput"
						class="me-seed-input"
						placeholder="数字种子，或任意字符串（Java 哈希）"
						spellcheck="false"
						@keyup.enter="openSeedMap"
					/>
					<button class="me-seed-open" :disabled="!seedInput.trim()" @click="openSeedMap">
						查看
					</button>
					<div v-if="seedReady" class="me-seed-active">
						已开图：种子 <b>{{ seedViewSeed }}</b>
					</div>
					<p class="me-seed-meta">
						26.x 参数族（26.1.2 校准）· 主世界表层<br />
						群系 + 水陆预测；平移 / 滚轮可漫游任意坐标
					</p>
				</div>
			</aside>

			<!-- 右侧：地图画布（真实瓦片 / 种子预测共用） -->
			<section class="me-canvas-wrap">
				<template v-if="showCanvas">
					<div v-if="mode === 'save'" class="me-toolbar">
						<div class="me-dim-group">
							<button
								v-for="d in DIMENSIONS"
								:key="d.key"
								class="me-dim-btn"
								:class="{ active: dimension === d.key }"
								@click="dimension = d.key"
							>
								{{ d.label }}
							</button>
						</div>
						<div class="me-style-group">
							<button class="me-dim-btn" :class="{ active: sliceY === null }" @click="setTopDown">
								表面
							</button>
							<button class="me-dim-btn" :class="{ active: sliceY !== null }" @click="setSlice">
								切片
							</button>
							<input
								v-if="sliceY !== null"
								v-model.number="sliceY"
								type="range"
								class="me-y-slider"
								min="-64"
								max="319"
								step="1"
							/>
							<span v-if="sliceY !== null" class="me-y-label">Y={{ sliceY }}</span>
						</div>
						<div class="me-style-group">
							<button
								class="me-dim-btn"
								:class="{ active: predictOn && dimension === '' && sliceY === null }"
								:disabled="dimension !== '' || sliceY !== null"
								title="未探索区生物群系预测（仅主世界表层；切片/下界等维度不可用；含 mod 世界时可能不准）"
								@click="predictOn = !predictOn"
							>
								预测
							</button>
						</div>
					</div>
					<div v-else class="me-toolbar me-toolbar-seed">
						<span class="me-seed-mode-note">种子模式 · 26.x 参数族 · 主世界表层（预测）</span>
					</div>
					<div class="me-canvas-holder">
						<MapCanvas
							:world-path="mode === 'seed' ? '' : worldPath"
							:dimension="mode === 'seed' ? '' : dimension"
							:style="mode === 'seed' ? 'topdown' : style"
							:show-prediction="mode === 'save' && dimension === '' && predictOn"
							:seed-mode="mode === 'seed'"
							:seed="seedViewSeed ?? ''"
							:init-scale="0.25"
						/>
					</div>
					<div v-if="mode === 'save'" class="me-canvas-statusbar">
						<span class="me-status-path" :title="worldPath">{{ worldPath }}</span>
						<button class="me-status-btn" @click="openWorldFolder">
							打开存档文件夹
						</button>
					</div>
					<div v-else class="me-canvas-statusbar">
						<span class="me-status-path">
							种子 {{ seedViewSeed }} — 预测瓦片缓存于本机，漫游即取
						</span>
					</div>
				</template>
				<div v-else class="me-canvas-empty">
					<Map :size="44" :stroke-width="1.2" opacity="0.35" />
					<p>{{ mode === "seed" ? "输入种子后开始查看" : "在左侧选择一个存档开始查看" }}</p>
				</div>
			</section>

		</div>
	</div>
</template>

<style scoped>
.me {
	display: flex;
	flex-direction: column;
	height: 100vh;
	background-color: var(--surface-page);
	color: var(--text-primary);
	font-size: 13px; /* 基础字号：地图编辑器整体偏小，统一抬升一档 */
	overflow: hidden;
}

/* ── 标题栏（更高的点击区 + 大一号图标）── */
.me-titlebar {
	display: flex;
	align-items: center;
	gap: 10px;
	height: 40px;
	padding: 0 6px 0 14px;
	background-color: var(--surface-card);
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.25));
	user-select: none;
	flex-shrink: 0;
}

.me-tb-icon { color: var(--color-brand); }
.me-tb-title {
	font-size: 14px;
	font-weight: 700;
	flex: 1;
	overflow: hidden;
	text-overflow: ellipsis;
	white-space: nowrap;
}

.me-tb-actions { display: flex; gap: 4px; }

.me-tb-btn {
	display: flex;
	align-items: center;
	justify-content: center;
	width: 34px;
	height: 28px;
	border: none;
	border-radius: 5px;
	background: transparent;
	color: var(--text-secondary);
	cursor: pointer;
	transition: background 0.1s;
}

.me-tb-btn:hover { background-color: var(--surface-hover); color: var(--text-primary); }
.me-tb-close:hover { background-color: #e81123; color: #fff; }

/* ── 主体左右布局 ── */
.me-body {
	display: flex;
	flex: 1;
	min-height: 0;
}

/* ── 左侧树（独立区：标题带与列表区背景分层）── */
.me-side {
	width: 250px;
	flex-shrink: 0;
	display: flex;
	flex-direction: column;
	border-right: 1px solid var(--surface-border, rgba(128, 128, 128, 0.28));
	background-color: var(--surface-card);
}

.me-side-head {
	display: flex;
	align-items: center;
	justify-content: space-between;
	padding: 10px 12px;
	background-color: var(--surface-page);
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.22));
	flex-shrink: 0;
}

.me-side-title {
	font-size: 12px;
	font-weight: 700;
	color: var(--text-tertiary);
	letter-spacing: 0.08em;
}

.me-side-refresh {
	display: flex;
	align-items: center;
	justify-content: center;
	width: 24px;
	height: 24px;
	border: none;
	border-radius: 4px;
	background: transparent;
	color: var(--text-tertiary);
	cursor: pointer;
	transition: background 0.1s;
}

.me-side-refresh:hover { background-color: var(--surface-hover); color: var(--text-primary); }
.spinning { animation: me-spin 0.8s linear infinite; }
@keyframes me-spin { to { transform: rotate(360deg); } }

.me-tree {
	flex: 1;
	overflow-y: auto;
	padding: 6px 8px 12px;
}

.me-tree-loading,
.me-tree-empty {
	padding: 10px 12px;
	font-size: 12.5px;
	color: var(--text-tertiary);
}

.me-tree-err {
	padding: 10px 12px;
	font-size: 12.5px;
	color: var(--text-danger, #e05555);
	display: flex;
	flex-direction: column;
	gap: 6px;
	align-items: flex-start;
}

.me-tree-err-icon { color: var(--text-danger, #e05555); margin: 18px auto 0; display: block; }

.me-tree-retry {
	border: 1px solid var(--surface-border, rgba(128, 128, 128, 0.3));
	background: transparent;
	color: var(--text-secondary);
	border-radius: 4px;
	font-size: 12px;
	padding: 4px 12px;
	cursor: pointer;
}

.me-tree-retry:hover { background-color: var(--surface-hover); }

.me-node {
	display: flex;
	align-items: center;
	gap: 6px;
	width: 100%;
	border: none;
	background: transparent;
	color: inherit;
	text-align: left;
	cursor: pointer;
	border-radius: 6px;
	transition: background 0.1s;
	user-select: none;
}

.me-node:hover { background-color: var(--surface-hover); }

/* 游戏文件夹：分组标签卡（浅底圆角，无游离边线），组间距自然分隔 */
.me-dir {
	padding: 8px 10px;
	font-size: 13px;
	font-weight: 700;
	margin-top: 12px;
	background-color: color-mix(in srgb, var(--surface-hover) 55%, transparent);
	color: var(--text-primary);
}

.me-dir:first-child { margin-top: 0; }

.me-dir:hover {
	background-color: color-mix(in srgb, var(--surface-hover) 85%, transparent);
}

.me-ver { padding: 6px 8px 6px 24px; font-size: 12.5px; font-weight: 500; }
.me-world { padding: 6px 8px 6px 38px; font-size: 12.5px; }

.me-node.active { background-color: var(--color-brand); color: #fff; }
.me-node.active .me-world-meta { color: rgba(255, 255, 255, 0.75); }
.me-dir.active { background-color: color-mix(in srgb, var(--color-brand) 18%, transparent); }

.me-caret {
	flex-shrink: 0;
	color: var(--text-tertiary);
	transition: transform 0.15s ease;
}

/* VS Code 式：展开时箭头旋转 90°（收起态为指向右） */
.me-caret.open { transform: rotate(90deg); }

.me-dir-icon { flex-shrink: 0; color: var(--color-brand); }
.me-world-icon { flex-shrink: 0; color: var(--text-tertiary); }
.me-node.active .me-world-icon { color: rgba(255, 255, 255, 0.8); }

.me-node-name {
	flex: 1;
	min-width: 0;
	overflow: hidden;
	text-overflow: ellipsis;
	white-space: nowrap;
}

.me-ver-badge {
	font-size: 11.5px;
	padding: 1px 7px;
	border-radius: 4px;
	background-color: var(--surface-hover);
	color: var(--text-tertiary);
	flex-shrink: 0;
}

.me-world-thumb {
	width: 18px;
	height: 18px;
	border-radius: 3px;
	flex-shrink: 0;
	object-fit: cover;
}

/* ── 右侧工具栏（独立功能区：底边分隔加粗）── */
.me-toolbar {
	display: flex;
	align-items: center;
	gap: 12px;
	padding: 8px 12px;
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.25));
	background-color: var(--surface-card);
	flex-shrink: 0;
	flex-wrap: wrap;
}

/* 工具栏内部小分区（维度 / 样式 / 滑杆）间隔更清晰 */
.me-style-group {
	padding-left: 12px;
	border-left: 1px solid var(--surface-border, rgba(128, 128, 128, 0.2));
}

.me-dim-group,
.me-style-group {
	display: flex;
	align-items: center;
	gap: 4px;
}

.me-dim-btn {
	border: none;
	background: transparent;
	color: var(--text-secondary);
	font-size: 12.5px;
	padding: 4px 12px;
	border-radius: 5px;
	cursor: pointer;
	transition: background 0.1s, color 0.1s;
}

.me-dim-btn:hover { background-color: var(--surface-hover); }
.me-dim-btn.active { background-color: var(--color-brand); color: #fff; }

.me-y-slider {
	width: 150px;
	accent-color: var(--color-brand);
}

.me-y-label {
	font-size: 12.5px;
	color: var(--text-secondary);
	min-width: 56px;
	font-variant-numeric: tabular-nums;
}

.me-canvas-holder {
	flex: 1;
	min-height: 0;
	position: relative;
}

.me-world-meta {
	font-size: 11px;
	color: var(--text-tertiary);
	flex-shrink: 0;
}

/* ── 右侧画布 ── */
.me-canvas-wrap {
	flex: 1;
	min-width: 0;
	display: flex;
	flex-direction: column;
	position: relative;
	background-color: var(--surface-page);
}

.me-canvas-empty,
.me-canvas-placeholder {
	flex: 1;
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	gap: 10px;
	color: var(--text-tertiary);
}

.me-canvas-empty p { font-size: 14px; margin: 0; }

.me-ph-title { font-size: 16px; font-weight: 600; color: var(--text-primary); margin: 0; }
.me-ph-meta { font-size: 13px; margin: 0; }
.me-ph-hint { font-size: 12px; opacity: 0.7; margin: 0; }

.me-canvas-statusbar {
	display: flex;
	align-items: center;
	gap: 10px;
	padding: 7px 12px;
	border-top: 1px solid var(--surface-border, rgba(128, 128, 128, 0.25));
	background-color: var(--surface-card);
	flex-shrink: 0;
}

.me-status-path {
	flex: 1;
	min-width: 0;
	font-size: 12px;
	color: var(--text-tertiary);
	overflow: hidden;
	text-overflow: ellipsis;
	white-space: nowrap;
	direction: rtl; /* 长路径尾部对齐（显示最深的目录名） */
	text-align: left;
}

.me-status-btn {
	display: flex;
	align-items: center;
	gap: 5px;
	border: none;
	background: transparent;
	color: var(--text-secondary);
	font-size: 12px;
	cursor: pointer;
	padding: 4px 10px;
	border-radius: 4px;
	transition: background 0.1s;
}

.me-status-btn:hover { background-color: var(--surface-hover); color: var(--text-primary); }

/* ── 模式切换（存档 / 种子地图）── */
.me-mode-tabs {
	display: flex;
	gap: 6px;
	padding: 10px 12px 8px;
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.25));
	flex-shrink: 0;
}

.me-mode-tab {
	flex: 1;
	padding: 6px 0;
	border-radius: 6px;
	font-size: 12px;
	border: 1px solid transparent;
	background: transparent;
	color: var(--text-secondary);
	cursor: pointer;
	transition: background 0.15s, color 0.15s;
}

.me-mode-tab:hover { background: var(--surface-hover); color: var(--text-primary); }
.me-mode-tab.active {
	background: color-mix(in srgb, var(--color-brand) 18%, transparent);
	color: var(--color-brand);
	border-color: color-mix(in srgb, var(--color-brand) 45%, transparent);
}

/* ── 种子面板 ── */
.me-seed-panel { padding: 14px 12px; display: flex; flex-direction: column; gap: 10px; }

.me-seed-hint { font-size: 12px; line-height: 1.6; color: var(--text-secondary); }

.me-seed-input {
	width: 100%;
	box-sizing: border-box;
	padding: 8px 10px;
	border-radius: 6px;
	font-size: 13px;
	background: var(--surface-input, var(--surface-card));
	border: 1px solid var(--surface-border, rgba(128, 128, 128, 0.35));
	color: var(--text-primary);
	outline: none;
}

.me-seed-input:focus { border-color: var(--color-brand); }

.me-seed-open {
	padding: 8px 0;
	border-radius: 6px;
	font-size: 13px;
	border: 1px solid var(--surface-border, rgba(128, 128, 128, 0.35));
	background: color-mix(in srgb, var(--color-brand) 14%, transparent);
	color: var(--color-brand);
	cursor: pointer;
}

.me-seed-open:disabled { opacity: 0.45; cursor: default; }

.me-seed-active {
	font-size: 12px;
	color: var(--text-secondary);
	padding: 8px 10px;
	border-radius: 6px;
	background: color-mix(in srgb, var(--surface-card) 70%, transparent);
	word-break: break-all;
}

.me-seed-active b { color: var(--text-primary); }

.me-seed-meta {
	font-size: 11px;
	line-height: 1.7;
	color: var(--text-tertiary);
}

.me-toolbar-seed { display: flex; align-items: center; }
.me-seed-mode-note { font-size: 12px; color: var(--text-tertiary); }
</style>
