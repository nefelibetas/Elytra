<script setup lang="ts">
// 地图编辑器窗口（label=map-editor）：左侧三级树（游戏目录→版本→存档）+ 右侧地图画布。
// 独立 WebView 数据自治：初始选中经 URL query 传入，树/图标自行 invoke 拉取；
// 标题栏自绘（无边框窗口），窗口创建与聚焦在 store/mapEditor.ts 编排。
import { computed, onMounted, onUnmounted, ref } from "vue";
import { ChevronDown, ChevronRight, Folder, Map, Minus, X, RefreshCw, AlertCircle } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { LOADER_TITLES } from "../utils/versionMeta";
import { fmtUnix } from "../utils/format";
import {
	mapTree, treeLoading, treeError, selected, expanded,
	toggleExpand, loadTree, initFromQuery, applySelection, selectedWorldPath, syncWindowTitle,
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

const hasSelection = computed(() => Boolean(selected.world));
const worldPath = computed(() => selectedWorldPath() ?? "");

// ── 查看器状态：维度 + 渲染样式 ──
const dimension = ref("");
const DIMENSIONS = [
	{ key: "", label: "主世界" },
	{ key: "DIM-1", label: "下界" },
	{ key: "DIM1", label: "末地" },
] as const;
const sliceY = ref<number | null>(null);
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

	await loadTree();
	initFromQuery();
	applySelection();
	prefetchIcons();
});

/** 刷新树（保留选中与展开状态）。 */
async function refreshTree() {
	const keep = { ...selected };
	await loadTree();
	selected.dirPath = keep.dirPath;
	selected.version = keep.version;
	selected.world = keep.world;
	applySelection();
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
			<Map :size="14" class="me-tb-icon" />
			<span class="me-tb-title">{{ selected.world || "Elytra 地图" }}</span>
			<div class="me-tb-actions">
				<button class="me-tb-btn" title="最小化" @click="minimize"><Minus :size="14" /></button>
				<button class="me-tb-btn me-tb-close" title="关闭" @click="closeWindow"><X :size="14" /></button>
			</div>
		</div>

		<div class="me-body">
			<!-- 左侧：三级树 -->
			<aside class="me-side">
				<div class="me-side-head">
					<span class="me-side-title">存档</span>
					<button class="me-side-refresh" title="刷新" @click="refreshTree">
						<RefreshCw :size="13" :class="{ spinning: treeLoading }" />
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
						<!-- 1 级：游戏目录 -->
						<button class="me-node me-dir" @click="toggleExpand(dirKey(dir.path))">
							<ChevronDown v-if="expanded[dirKey(dir.path)]" :size="13" class="me-caret" />
							<ChevronRight v-else :size="13" class="me-caret" />
							<Folder :size="14" class="me-dir-icon" />
							<span class="me-node-name">{{ dir.name }}</span>
						</button>

						<template v-if="expanded[dirKey(dir.path)]">
							<template v-for="ver in dir.versions" :key="dir.path + '|' + ver.name">
								<!-- 2 级：版本 -->
								<button class="me-node me-ver" @click="toggleExpand(verKey(dir.path, ver.name))">
									<ChevronDown v-if="expanded[verKey(dir.path, ver.name)]" :size="12" class="me-caret" />
									<ChevronRight v-else :size="12" class="me-caret" />
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
										<Map v-else :size="13" class="me-world-icon" />
										<span class="me-node-name">{{ w.name }}</span>
										<span class="me-world-meta">{{ fmtUnix(w.last_modified) }}</span>
									</div>
									<div v-if="ver.worlds.length === 0" class="me-leaf-empty">无存档</div>
								</template>
							</template>
							<div v-if="dir.versions.length === 0" class="me-leaf-empty">无版本</div>
						</template>
					</template>
				</div>
			</aside>

			<!-- 右侧：地图画布（阶段 A：瓦片渲染已接入） -->
			<section class="me-canvas-wrap">
				<template v-if="hasSelection">
					<div class="me-toolbar">
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
					</div>
					<div class="me-canvas-holder">
						<MapCanvas :world-path="worldPath" :dimension="dimension" :style="style" />
					</div>
					<div class="me-canvas-statusbar">
						<span class="me-status-path" :title="worldPath">{{ worldPath }}</span>
						<button class="me-status-btn" @click="openWorldFolder">
							打开存档文件夹
						</button>
					</div>
				</template>
				<div v-else class="me-canvas-empty">
					<Map :size="44" :stroke-width="1.2" opacity="0.35" />
					<p>在左侧选择一个存档开始查看</p>
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
	overflow: hidden;
}

/* ── 标题栏 ── */
.me-titlebar {
	display: flex;
	align-items: center;
	gap: 8px;
	height: 32px;
	padding: 0 8px 0 12px;
	background-color: var(--surface-card);
	user-select: none;
	flex-shrink: 0;
}

.me-tb-icon { color: var(--color-brand); }
.me-tb-title {
	font-size: 12px;
	font-weight: 600;
	flex: 1;
	overflow: hidden;
	text-overflow: ellipsis;
	white-space: nowrap;
}

.me-tb-actions { display: flex; gap: 2px; }

.me-tb-btn {
	display: flex;
	align-items: center;
	justify-content: center;
	width: 28px;
	height: 24px;
	border: none;
	border-radius: 4px;
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

/* ── 左侧树 ── */
.me-side {
	width: 240px;
	flex-shrink: 0;
	display: flex;
	flex-direction: column;
	border-right: 1px solid var(--surface-border, rgba(128, 128, 128, 0.2));
	background-color: var(--surface-card);
}

.me-side-head {
	display: flex;
	align-items: center;
	justify-content: space-between;
	padding: 10px 12px 6px;
}

.me-side-title {
	font-size: 11px;
	font-weight: 600;
	color: var(--text-tertiary);
	letter-spacing: 0.05em;
}

.me-side-refresh {
	display: flex;
	align-items: center;
	justify-content: center;
	width: 22px;
	height: 22px;
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
	padding: 4px 6px 10px;
}

.me-tree-loading,
.me-tree-empty,
.me-leaf-empty {
	padding: 8px 12px;
	font-size: 11px;
	color: var(--text-tertiary);
}

.me-tree-err {
	padding: 8px 12px;
	font-size: 11px;
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
	font-size: 11px;
	padding: 3px 10px;
	cursor: pointer;
}

.me-tree-retry:hover { background-color: var(--surface-hover); }

.me-node {
	display: flex;
	align-items: center;
	gap: 5px;
	width: 100%;
	border: none;
	background: transparent;
	color: inherit;
	text-align: left;
	cursor: pointer;
	border-radius: 5px;
	transition: background 0.1s;
	user-select: none;
}

.me-node:hover { background-color: var(--surface-hover); }

.me-dir { padding: 5px 6px; font-size: 12px; font-weight: 600; }
.me-ver { padding: 4px 6px 4px 20px; font-size: 12px; }
.me-world { padding: 5px 6px 5px 34px; font-size: 12px; }

.me-node.active { background-color: var(--color-brand); color: #fff; }
.me-node.active .me-world-meta { color: rgba(255, 255, 255, 0.75); }

.me-caret { flex-shrink: 0; color: var(--text-tertiary); }
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
	font-size: 10px;
	padding: 1px 5px;
	border-radius: 3px;
	background-color: var(--surface-hover);
	color: var(--text-tertiary);
	flex-shrink: 0;
}

.me-world-thumb {
	width: 16px;
	height: 16px;
	border-radius: 3px;
	flex-shrink: 0;
	object-fit: cover;
}

/* ── 阶段 A：画布工具栏 ── */
.me-toolbar {
	display: flex;
	align-items: center;
	gap: 12px;
	padding: 6px 10px;
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.2));
	background-color: var(--surface-card);
	flex-shrink: 0;
	flex-wrap: wrap;
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
	font-size: 11px;
	padding: 3px 10px;
	border-radius: 4px;
	cursor: pointer;
	transition: background 0.1s, color 0.1s;
}

.me-dim-btn:hover { background-color: var(--surface-hover); }
.me-dim-btn.active { background-color: var(--color-brand); color: #fff; }

.me-y-slider {
	width: 140px;
	accent-color: var(--color-brand);
}

.me-y-label {
	font-size: 11px;
	color: var(--text-secondary);
	min-width: 52px;
	font-variant-numeric: tabular-nums;
}

.me-canvas-holder {
	flex: 1;
	min-height: 0;
	position: relative;
}

.me-world-meta {
	font-size: 10px;
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

.me-canvas-empty p { font-size: 13px; margin: 0; }

.me-ph-title { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0; }
.me-ph-meta { font-size: 12px; margin: 0; }
.me-ph-hint { font-size: 11px; opacity: 0.7; margin: 0; }

.me-canvas-statusbar {
	display: flex;
	align-items: center;
	gap: 10px;
	padding: 6px 12px;
	border-top: 1px solid var(--surface-border, rgba(128, 128, 128, 0.2));
	background-color: var(--surface-card);
	flex-shrink: 0;
}

.me-status-path {
	flex: 1;
	min-width: 0;
	font-size: 11px;
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
	font-size: 11px;
	cursor: pointer;
	padding: 3px 8px;
	border-radius: 4px;
	transition: background 0.1s;
}

.me-status-btn:hover { background-color: var(--surface-hover); color: var(--text-primary); }
</style>
