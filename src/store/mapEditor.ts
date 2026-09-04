// 地图编辑器窗口 store：三级树数据 + 选中定位（数据自治，不依赖主窗口 store——
// Tauri 多窗口各自独立 WebView，模块级单例互不共享）。
import { reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

/** 树第 3 级：存档概要（镜像 Rust map::tree::MapWorldEntry）。 */
export interface MapWorldEntry {
	name: string;
	last_modified: number;
}

/** 树第 2 级：版本（镜像 Rust map::tree::MapVersionEntry）。 */
export interface MapVersionEntry {
	name: string;
	loader: string;
	worlds: MapWorldEntry[];
}

/** 树第 1 级：游戏目录（镜像 Rust map::tree::MapDirEntry）。 */
export interface MapDirEntry {
	name: string;
	path: string;
	versions: MapVersionEntry[];
}

/** 树加载状态（编辑器窗口打开即拉取）。 */
export const mapTree = ref<MapDirEntry[]>([]);
export const treeLoading = ref(false);
export const treeError = ref("");

/** 当前选中存档（右侧画布渲染目标；由树点击或 URL query 初始化）。 */
export const selected = reactive<{
	dirPath: string;
	version: string;
	world: string;
}>({ dirPath: "", version: "", world: "" });

/** 树节点展开状态（key = dirPath|version；目录级默认展开）。 */
export const expanded = reactive<Record<string, boolean>>({});

export function toggleExpand(key: string) {
	expanded[key] = !expanded[key];
}

/** 当前选中存档的绝对路径（versions/{v}/saves/{w}；画布层按此取 region）。 */
export function selectedWorldPath(): string | null {
	if (!selected.dirPath || !selected.version || !selected.world) return null;
	return `${selected.dirPath}\\versions\\${selected.version}\\saves\\${selected.world}`;
}

/** 拉取三级树；成功后按当前 selected 保持高亮（树刷新不丢选中）。 */
export async function loadTree() {
	treeLoading.value = true;
	treeError.value = "";
	try {
		mapTree.value = await invoke<MapDirEntry[]>("get_map_tree");
	} catch (e) {
		treeError.value = String(e).split("\n")[0];
	} finally {
		treeLoading.value = false;
	}
}

/** 由 URL query 初始化选中（version + world 可能跨目录重名，需 dirPath 锚定）。 */
export function initFromQuery() {
	const q = new URLSearchParams(window.location.hash.split("?")[1] ?? "");
	const version = q.get("version") ?? "";
	const world = q.get("world") ?? "";
	if (!version || !world) return;
	// 树就位后定位：优先匹配包含该版本+存档的目录（单目录场景直接命中）
	selected.version = version;
	selected.world = world;
	applySelection();
}

/** 根据 version+world 在树中补全 dirPath（树未加载完成时由 loadTree 后再调）。 */
export function applySelection() {
	if (!selected.version || !selected.world) return;
	if (selected.dirPath && mapTree.value.some(d => d.path === selected.dirPath)) return;
	const hit = mapTree.value.find(d =>
		d.versions.some(v => v.name === selected.version && v.worlds.some(w => w.name === selected.world)),
	);
	if (hit) selected.dirPath = hit.path;
	// 自动展开命中路径
	if (hit) {
		expanded[hit.path] = true;
		expanded[`${hit.path}|${selected.version}`] = true;
	}
}

/** 编辑器窗口标题：世界名 - Elytra 地图。 */
export function syncWindowTitle() {
	const label = selected.world
		? `${selected.world} - Elytra 地图`
		: "Elytra 地图";
	void getCurrentWindow().setTitle(label);
}

// ── 编辑器窗口编排（主窗口侧调用）──

/** 打开（或聚焦已存在的）地图编辑器窗口；version/world 作为初始定位 query。 */
export async function openMapEditorWindow(version: string, world: string) {
	const existing = await WebviewWindow.getByLabel("map-editor");
	if (existing) {
		// 已打开：带新定位跳转 + 聚焦（编辑器监听事件重新定位）
		await existing.emit("map-editor:navigate", { version, world });
		await existing.setFocus();
		return;
	}
	new WebviewWindow("map-editor", {
		url: `index.html#/map-editor?version=${encodeURIComponent(version)}&world=${encodeURIComponent(world)}`,
		title: `${world} - Elytra 地图`,
		width: 1100,
		height: 700,
		minWidth: 900,
		minHeight: 560,
		center: true,
		resizable: true,
		decorations: false,
		transparent: true,
		shadow: false,
	});
}
