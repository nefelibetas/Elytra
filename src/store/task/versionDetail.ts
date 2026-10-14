// 主页版本卡片 store：存档/Mod 列表加载、Mod 更新检查与执行（模块级单例；版本切换时 reset 重载）。
import { ref, computed, watch, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { VERSION_DIR_CHANGED, type VersionDirChanged } from "../app/events";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { createBoundedCache } from "../../utils/boundedCache";
import { classifyVersion } from "../../utils/versionClassify";
import type { InstalledVersionInfo } from "../config/gameDirs";

// ── 类型镜像 Rust（commands/version.rs + downloader/mod_updates.rs）──

export interface WorldInfo {
  name: string;
  last_modified: number;
}

export interface ResourceFile {
  name: string;
  size: number;
  last_modified: number;
}

export type ResourceKind = "resourcepacks" | "datapacks" | "shaderpacks";

/** 本地 mod 详细项（镜像 Rust ModFileDetail：Modrinth 命中时带项目元数据）。 */
export interface ModFileDetail {
  file_name: string;
  size: number;
  last_modified: number;
  project_id: string | null;
  title: string | null;
  title_zh: string | null;
  description: string | null;
  description_zh: string | null;
  icon_url: string | null;
  downloads: number | null;
  current_version: string | null;
  /** 前端本地识别状态：pending 查询中 / done 已查询 / failed 本次查询失败（不自动重试）。 */
  identify?: ModIdentifyState;
}

/** Mod 元数据查询状态（仅前端使用）。 */
export type ModIdentifyState = "pending" | "done" | "failed";

export interface ModUpdate {
  file_name: string;
  project_id: string;
  current_version: string;
  latest_version: string;
  file_url: string;
  file_sha1: string;
}

// ── 版本 loader 推断（后端 json 特征优先，目录名分类兜底）──

export const versionLoader = ref("");

/** 仅真 ModLoader 版本可更新 Mod（排除原版与纯 OptiFine）。 */
export const isModLoaderVersion = computed(() =>
  ["fabric", "forge", "neoforge", "quilt"].includes(versionLoader.value),
);

/** 加载当前版本 loader：优先后端 json 特征推断（无 loader 标记的实例也能识别），失败回退目录名分类。 */
export async function loadVersionLoader(versionName: string) {
  try {
    const list = await invoke<InstalledVersionInfo[]>("get_installed_versions");
    const hit = list.find(v => v.name === versionName);
    versionLoader.value = hit?.loader || classifyVersion(versionName).category;
  } catch {
    versionLoader.value = classifyVersion(versionName).category;
  }
}

// ── 存档列表 ──

export const worlds = ref<WorldInfo[]>([]);
export const worldsLoading = ref(false);
export const worldsError = ref("");
/** 已加载缓存的版本名（设置页切 Tab 缓存命中跳过重载；空串 = 未加载）。 */
export const worldsLoadedFor = ref("");

export async function loadWorlds(versionName: string) {
  worldsLoading.value = true;
  worldsError.value = "";
  try {
    worlds.value = await invoke<WorldInfo[]>("list_worlds", { versionName });
    worldsLoadedFor.value = versionName;
  } catch (e) {
    worldsError.value = errMsg(e);
  } finally {
    worldsLoading.value = false;
  }
}

// ── 存档列表实时性：轮询自动反映游戏运行中新建/删除的存档 ──

let worldsPollTimer: number | null = null;
let worldsPollVersion = "";

/** 静默刷新：不置 loading（列表不闪烁）；内容一致时不更新；失败保留旧列表，下轮轮询自愈。 */
async function refreshWorldsSilent(versionName: string) {
	try {
		const list = await invoke<WorldInfo[]>("list_worlds", { versionName });
		if (JSON.stringify(list) === JSON.stringify(worlds.value)) return;
		worlds.value = list;
		worldsLoadedFor.value = versionName;
	} catch {
		// 静默失败：保留旧列表
	}
}

export function startWorldsLivePolling(versionName: string) {
	if (worldsPollTimer !== null && worldsPollVersion === versionName) return;
	stopWorldsLivePolling();
	worldsPollVersion = versionName;
	worldsPollTimer = window.setInterval(() => {
		if (document.visibilityState !== "visible" || worldsLoading.value) return;
		void refreshWorldsSilent(versionName);
	}, 3000);
}

export function stopWorldsLivePolling() {
	if (worldsPollTimer !== null) {
		clearInterval(worldsPollTimer);
		worldsPollTimer = null;
	}
	worldsPollVersion = "";
}

// ── 版本目录变化事件（后端非递归监听聚焦版本的一级资源子目录）──

let dirWatchReady = false;
/** 当前关注的版本（页面挂载时设置；事件按此过滤，避免刷新非当前版本）。 */
let watchedVersion = "";

/** 聚焦版本目录：切换后端监听目标 + 订阅资源子目录变化（幂等；页面 onMounted 调用）。 */
export function watchVersionDir(version: string) {
	watchedVersion = version;
	void invoke("set_watched_version", { version: version || null });
	if (dirWatchReady || !version) return;
	dirWatchReady = true;
	void listen<VersionDirChanged>(VERSION_DIR_CHANGED, (ev) => {
		if (ev.payload.version !== watchedVersion || document.visibilityState !== "visible") return;
		if (ev.payload.kinds.includes("saves")) void refreshWorldsSilent(watchedVersion);
		if (ev.payload.kinds.includes("mods")) void loadMods(watchedVersion);
	});
}

watch(worlds, list => {
	const v = worldsLoadedFor.value;
	if (!v) return;
	for (const w of list) void loadWorldIcon(v, w.name);
});

// ── 世界缩略图（saves/<world>/icon.png 懒加载缓存）──

/** world name → data URL（版本切换时清空）。单图上限见 Rust get_world_icon；上限 128 防无界增长。 */
const worldIconsCache = createBoundedCache<string>(128);
export const worldIcons = worldIconsCache.record;
const worldIconsLoadingCache = createBoundedCache<boolean>(256);
const worldIconsLoading = worldIconsLoadingCache.record;

export async function loadWorldIcon(versionName: string, worldName: string) {
  if (worldIcons[worldName] || worldIconsLoading[worldName]) return;
  worldIconsLoadingCache.put(worldName, true);
  try {
    // 后端返回 { mime, base64 } 载荷（魔数嗅探 + 服务端编码，替代 JSON 数字数组）
    const icon = await invoke<{ mime: string; data: string } | null>("get_world_icon", {
      versionName,
      worldName,
    });
    if (icon) {
      worldIconsCache.put(worldName, `data:${icon.mime};base64,${icon.data}`);
    }
  } catch {
    // 缺失/读取失败：保持 Map 占位
  } finally {
    worldIconsLoadingCache.put(worldName, false);
  }
}

// ── Mod 列表 ──

export const mods = ref<ModFileDetail[]>([]);
export const modsLoading = ref(false);
export const modsError = ref("");
export const modsNetworkError = ref(false);
export const modsLoadedFor = ref("");
/** 正在加载的版本名（防并发重入：onMounted 与手动刷新可能同时触发）。 */
const modsLoadingFor = ref("");
/** 重入期间到来的强制刷新（deleteMod/toggleModsBulk 的 reset）待补跑标记：见 loadMods。 */
let modsPendingReset = false;
/** mods 目录本地快速扫描（.jar / .jar.disabled，纯 read_dir 无网络）——列表骨架与图标判定用，免等 Modrinth 元数据。 */
export const modsQuick = ref<ResourceFile[]>([]);
/** 已禁用项的文件名（磁盘上为 `{name}.disabled`）：与识别结果解耦，识别写入不会覆盖它。 */
export const disabledMods = ref<Set<string>>(new Set());

export async function loadModsQuick(versionName: string) {
  try {
    const files = await invoke<ResourceFile[]>("list_resource_files", {
      versionName,
      kind: "mods",
    });
    // 禁用项在磁盘上是 `.jar.disabled`，同样列出（否则用户没有重新启用的入口）
    const jars = files.filter(f => /\.jar(\.disabled)?$/i.test(f.name));
    modsQuick.value = jars;
    disabledMods.value = new Set(
      jars.filter(f => /\.disabled$/i.test(f.name)).map(f => f.name.replace(/\.disabled$/i, "")),
    );
  } catch {
    modsQuick.value = [];
    disabledMods.value = new Set();
  }
}

function quickToDetail(files: ResourceFile[]): ModFileDetail[] {
  return files.map(f => ({
    // 对外统一用原名（命令与选择集都以它为准）；禁用态由 disabledMods 单独承载
    file_name: f.name.replace(/\.disabled$/i, ""),
    size: f.size,
    last_modified: f.last_modified,
    project_id: null,
    title: null,
    title_zh: null,
    description: null,
    description_zh: null,
    icon_url: null,
    downloads: null,
    current_version: null,
  }));
}

/** 单项元数据查询超时：到点即判失败并继续后续项（后端请求无法取消，但不再阻塞队列）。 */
const IDENTIFY_TIMEOUT_MS = 8000;
/** 本版本内查询失败的项：重扫时不再自动排队（手动刷新会清空重试）。 */
const identifyFailed = ref<Set<string>>(new Set());

function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("查询超时")), ms);
    p.then(
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}

/** 标记单项识别状态（替换对象以触发响应式更新）。 */
function setIdentifyState(fileName: string, identify: ModIdentifyState) {
  const idx = mods.value.findIndex(m => m.file_name === fileName);
  if (idx >= 0) mods.value[idx] = { ...mods.value[idx], identify };
}

export async function identifyModFile(versionName: string, fileName: string) {
  try {
    const r = await withTimeout(
      invoke<ModFileDetail | null>("identify_mod_file", { versionName, fileName }),
      IDENTIFY_TIMEOUT_MS,
    );
    if (!r) {
      // 未命中 Modrinth：保留本地信息，标记已查询
      setIdentifyState(fileName, "done");
      return;
    }
    const idx = mods.value.findIndex(m => m.file_name === fileName);
    if (idx >= 0) mods.value[idx] = { ...r, identify: "done" };
  } catch {
    identifyFailed.value = new Set([...identifyFailed.value, fileName]);
    setIdentifyState(fileName, "failed");
  }
}

/** 本地重扫并重建列表骨架（复用已识别结果）：禁用态与外部增删立即可见，不受识别在途影响。 */
async function refreshModsLocal(versionName: string) {
  await loadModsQuick(versionName);
  const prev = new Map(mods.value.map(m => [m.file_name, m]));
  mods.value = quickToDetail(modsQuick.value).map(m => {
    const hit = prev.get(m.file_name);
    // 同一文件（大小/修改时间一致）沿用识别结果，避免重复联网
    const reusable = !!hit && hit.size === m.size && hit.last_modified === m.last_modified;
    if (reusable) return hit;
    // 禁用项不参与查询（不显示查询状态）；已知失败的项不自动重试（手动刷新会清空失败集）
    if (disabledMods.value.has(m.file_name)) return m;
    return { ...m, identify: identifyFailed.value.has(m.file_name) ? "failed" : "pending" };
  });
  modsNetworkError.value = false;
}

/** 加载 mods 列表：先本地重扫（立即反映磁盘事实），再对尚无元数据且未禁用的项增量识别；`reset` 会重试此前失败的项。 */
export async function loadMods(versionName: string, reset = false) {
  modsError.value = "";
  if (reset) identifyFailed.value = new Set();
  await refreshModsLocal(versionName);
  // 同版本识别在途：跳过识别（记待补跑标记，由在途流程 finally 补一次）
  if (modsLoadingFor.value === versionName) {
    if (reset) modsPendingReset = true;
    return;
  }
  modsLoadingFor.value = versionName;
  modsLoading.value = true;
  try {
    // 逐文件增量识别：只处理尚无元数据且未失败的项，分批并发（每批 8）；单项自带超时，
    // 批次用 allSettled——个别失败/超时不中断其余项
    const pending = mods.value
      .filter(
        m => !disabledMods.value.has(m.file_name) && m.project_id === null && m.identify !== "failed",
      )
      .map(m => m.file_name);
    const BATCH = 8;
    for (let i = 0; i < pending.length; i += BATCH) {
      await Promise.allSettled(pending.slice(i, i + BATCH).map(f => identifyModFile(versionName, f)));
    }
    modsLoadedFor.value = versionName;
  } catch (e) {
    modsError.value = errMsg(e);
  } finally {
    modsLoading.value = false;
    modsLoadingFor.value = "";
    // 在途期间到来的强制刷新：补跑一次
    if (modsPendingReset) {
      modsPendingReset = false;
      void loadMods(versionName, true);
    }
  }
}

// ── 资源目录（材质包 / 数据包 / 光影包）──

/** 详细列表结果（镜像 Rust ResourceListResult）。 */
export interface ResourceListResult {
  files: ModFileDetail[];
  network_error: boolean;
}

export const resourcepacks = ref<ModFileDetail[]>([]);
export const resourcepacksLoading = ref(false);
export const resourcepacksError = ref("");
export const resourcepacksNetworkError = ref(false);
export const resourcepacksLoadedFor = ref("");
export const datapacks = ref<ModFileDetail[]>([]);
export const datapacksLoading = ref(false);
export const datapacksError = ref("");
export const datapacksNetworkError = ref(false);
export const datapacksLoadedFor = ref("");
export const shaderpacks = ref<ModFileDetail[]>([]);
export const shaderpacksLoading = ref(false);
export const shaderpacksError = ref("");
export const shaderpacksNetworkError = ref(false);
export const shaderpacksLoadedFor = ref("");

export const resourcepacksQuick = ref<ResourceFile[]>([]);
export const datapacksQuick = ref<ResourceFile[]>([]);
export const shaderpacksQuick = ref<ResourceFile[]>([]);

export interface ResourceRefs {
  list: Ref<ModFileDetail[]>;
  loading: Ref<boolean>;
  error: Ref<string>;
  networkError: Ref<boolean>;
  quick: Ref<ResourceFile[]>;
  loadedFor: Ref<string>;
}

const RESOURCE_KINDS = ["resourcepacks", "datapacks", "shaderpacks"] as const;

export function resourceRefs(kind: ResourceKind): ResourceRefs {
  const map: Record<ResourceKind, ResourceRefs> = {
    resourcepacks: {
      list: resourcepacks, loading: resourcepacksLoading, error: resourcepacksError,
      networkError: resourcepacksNetworkError, quick: resourcepacksQuick, loadedFor: resourcepacksLoadedFor,
    },
    datapacks: {
      list: datapacks, loading: datapacksLoading, error: datapacksError,
      networkError: datapacksNetworkError, quick: datapacksQuick, loadedFor: datapacksLoadedFor,
    },
    shaderpacks: {
      list: shaderpacks, loading: shaderpacksLoading, error: shaderpacksError,
      networkError: shaderpacksNetworkError, quick: shaderpacksQuick, loadedFor: shaderpacksLoadedFor,
    },
  };
  return map[kind];
}

export async function loadResourceQuick(versionName: string, kind: ResourceKind) {
  const { quick } = resourceRefs(kind);
  try {
    quick.value = await invoke<ResourceFile[]>("list_resource_files", {
      versionName,
      kind,
    });
  } catch {
    quick.value = [];
  }
}

/** 加载资源目录详细列表（Modrinth 识别；网络失败时 files 仍含本地信息并置 network_error）。 */
export async function loadResourceDetailed(versionName: string, kind: ResourceKind) {
  const { list, loading, error, networkError, loadedFor, quick } = resourceRefs(kind);
  loading.value = true;
  error.value = "";
  networkError.value = false;
  if (loadedFor.value !== versionName) {
    if (quick.value.length === 0) await loadResourceQuick(versionName, kind);
    list.value = quickToDetail(quick.value);
  }
  try {
    const r = await invoke<ResourceListResult>("list_resource_detailed", {
      versionName,
      kind,
    });
    list.value = r.files;
    networkError.value = r.network_error;
    loadedFor.value = versionName;
  } catch (e) {
    error.value = errMsg(e);
  } finally {
    loading.value = false;
  }
}

export function loadResourceLists(versionName: string) {
  for (const k of RESOURCE_KINDS) {
    void loadResourceQuick(versionName, k);
    void loadResourceDetailed(versionName, k);
  }
}

// ── Mod 更新检查 ──

export const updates = ref<ModUpdate[]>([]);
export const checkingUpdates = ref(false);
export const updatesError = ref("");
/** 是否已执行过检查（可更新 tab 空态区分"未检查"与"已是最新"）。 */
export const updatesChecked = ref(false);

/** 检查更新（仅 ModLoader 版本调用）：mc/loader 由后端从版本 json 权威解析；结果覆盖 updates；网络失败仅报错不破坏既有列表。 */
export async function checkModUpdates(versionName: string) {
  checkingUpdates.value = true;
  updatesError.value = "";
  try {
    updates.value = await invoke<ModUpdate[]>("check_mod_updates", {
      versionName,
    });
    updatesChecked.value = true;
  } catch (e) {
    updatesError.value = errMsg(e);
  } finally {
    checkingUpdates.value = false;
  }
}

/** 正在更新中的 file_name 集合（ref<Set> 需整体替换触发响应）。 */
const updating = ref<string[]>([]);

export const updatingFiles = computed(() => new Set(updating.value));

export async function updateMod(versionName: string, upd: ModUpdate) {
  if (updatingFiles.value.has(upd.file_name)) return;
  updating.value = [...updating.value, upd.file_name];
  try {
    await invoke("update_mod_file", {
      versionName,
      fileName: upd.file_name,
      fileUrl: upd.file_url,
      fileSha1: upd.file_sha1,
    });
    showToast(`已更新 ${upd.file_name}`);
    updates.value = updates.value.filter(u => u.file_name !== upd.file_name);
    await loadMods(versionName);
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    updating.value = updating.value.filter(f => f !== upd.file_name);
  }
}

export async function deleteWorld(versionName: string, worldName: string) {
  await invoke("delete_world", { versionName, worldName });
}

export async function deleteResourceFile(
  versionName: string,
  kind: ResourceKind,
  fileName: string,
) {
  await invoke("delete_resource_file", { versionName, kind, fileName });
}

export async function deleteMod(versionName: string, fileName: string) {
  await invoke("delete_mod_file", { versionName, fileName });
  updates.value = updates.value.filter(u => u.file_name !== fileName);
  await loadMods(versionName, true);
}

/** 批量切换启用/禁用（逐个重命名 .jar ↔ .jar.disabled），最后统一重扫一次列表；返回成功项数与失败原因（错误首行）。 */
export async function toggleModsBulk(versionName: string, fileNames: string[]) {
  const failed: string[] = [];
  for (const name of fileNames) {
    try {
      await invoke<boolean>("toggle_mod_file", { versionName, fileName: name });
    } catch (e) {
      failed.push(errMsg(e));
    }
  }
  updates.value = updates.value.filter(u => !fileNames.includes(u.file_name));
  await loadMods(versionName, true);
  return { ok: fileNames.length - failed.length, failed };
}

// ── 版本切换重置 ──

export function resetVersionDetail() {
  worlds.value = [];
  worldsError.value = "";
  worldsLoadedFor.value = "";
  worldIconsCache.clear();
  worldIconsLoadingCache.clear();
  mods.value = [];
  modsError.value = "";
  modsNetworkError.value = false;
  modsLoadedFor.value = "";
  modsLoadingFor.value = "";
  modsPendingReset = false;
  modsQuick.value = [];
  // 三类资源目录统一重置（loading 保持原样不清）
  for (const k of RESOURCE_KINDS) {
    const r = resourceRefs(k);
    r.list.value = [];
    r.error.value = "";
    r.networkError.value = false;
    r.loadedFor.value = "";
    r.quick.value = [];
  }
  updates.value = [];
  updatesError.value = "";
  updatesChecked.value = false;
  versionLoader.value = "";
  updating.value = [];
}
