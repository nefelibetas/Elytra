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

/** 轮询刷新（编辑器窗口打开期间定时调用）：游戏运行中新建/删除的版本与存档
 *  实时反映到树上。选中存档已被删除时清空选中（右侧画布随 hasSelection 收起）；
 *  展开状态与仍存在的选中保持不变。 */
export async function pollTree() {
	await loadTree();
	if (selected.world) {
		const exists = mapTree.value.some(d =>
			d.versions.some(v => v.name === selected.version && v.worlds.some(w => w.name === selected.world)),
		);
		if (!exists) {
			selected.dirPath = "";
			selected.version = "";
			selected.world = "";
			syncWindowTitle();
		}
	}
	applySelection();
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

let closeCascadeRegistered = false;

/** ms 后 resolve 的空承诺（Promise.withResolvers 需 ES2024 且 WebKitGTK 2.40
 *  运行时缺失，故本地封装，语义等价且无环境依赖）。 */
function delay(ms: number): Promise<void> {
	let resolveFn: (() => void) | undefined;
	const promise = new Promise<void>(resolve => {
		resolveFn = resolve;
	});
	setTimeout(() => resolveFn?.(), ms);
	return promise;
}

/** 主窗口关闭时连带关闭地图编辑器（仅主窗口注册一次；编辑器窗口自身关闭不受影响）。
 *  先关编辑器再放行主窗口销毁（否则 webview 立即拆除，后台动作来不及执行）；
 *  编辑器关闭带 1.5s 超时兜底——不因编辑器卡顿而阻塞主窗口退出。
 *  曾踩坑：a) 无 allow-destroy 时主窗卡死（已补权限）；b) 纯后台 fire-and-forget
 *  导致编辑器没被关掉（本版为等待 + 超时）。 */
export function mainCloseCascadesToMapEditor() {
	const win = getCurrentWindow();
	if (win.label !== "main" || closeCascadeRegistered) return;
	closeCascadeRegistered = true;
	void win.onCloseRequested(async () => {
		try {
			const editor = await WebviewWindow.getByLabel("map-editor");
			if (editor) {
				await Promise.race([editor.close(), delay(1500)]);
			}
		} catch {
			// 忽略：主窗口关闭不受编辑器影响
		}
	});
}

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
