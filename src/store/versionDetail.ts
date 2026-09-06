// 主页版本卡片 store：存档/Mod 列表加载、Mod 更新检查与更新执行。
// 卡片组件 import 本模块单例；版本切换时 reset 后重新加载。
import { ref, computed, reactive, watch, type Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { classifyVersion } from "../utils/versionClassify";
import type { InstalledVersionInfo } from "./gameDirs";

// ── 类型镜像 Rust（commands/version.rs + downloader/mod_updates.rs）──

export interface WorldInfo {
  name: string;
  last_modified: number;
}

/** 版本下资源目录中的单个文件（镜像 Rust ResourceFile：resourcepacks/datapacks/shaderpacks）。 */
export interface ResourceFile {
  name: string;
  size: number;
  last_modified: number;
}

/** 资源目录类型（后端 list_resource_files 白名单）。 */
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
}

export interface ModUpdate {
  file_name: string;
  project_id: string;
  current_version: string;
  latest_version: string;
  file_url: string;
  file_sha1: string;
}

// ── 版本 loader 推断（后端 json 特征优先，目录名分类兜底）──

/** 当前版本的后端推断 loader（fabric/forge/neoforge/quilt/optifine/vanilla）。 */
export const versionLoader = ref("");

/** 仅真 ModLoader 版本可更新 Mod（排除原版与纯 OptiFine）。 */
export const isModLoaderVersion = computed(() =>
  ["fabric", "forge", "neoforge", "quilt"].includes(versionLoader.value),
);

/** 加载当前版本的 loader：优先后端 get_installed_versions 的 json 特征推断（PCL2 等
 *  目录名无 loader 标记的实例也能识别），失败回退目录名分类。 */
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

// ── 存档列表实时性（游戏运行中新建/删除存档自动反映，无需切版本或手动刷新）──

let worldsPollTimer: number | null = null;
let worldsPollVersion = "";

/** 静默刷新：不置 loading（列表不闪烁）；内容与当前一致时不更新（避免无谓重渲染）。
 *  失败静默——保留旧列表，下轮轮询自愈。 */
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

/** 开始存档轮询（可见页面组件挂载期间调用；切版本/重复调用幂等）。 */
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

/** 存档列表变化后懒加载新增项缩略图（store 级单次注册；轮询新增的存档同样生效）。 */
watch(worlds, list => {
	const v = worldsLoadedFor.value;
	if (!v) return;
	for (const w of list) void loadWorldIcon(v, w.name);
});

// ── 世界缩略图（saves/<world>/icon.png 懒加载缓存）──

/** world name → data URL（版本切换时清空）。 */
export const worldIcons = reactive<Record<string, string>>({});
/** 加载中标记（幂等，防并发重复请求）。 */
const worldIconsLoading = reactive<Record<string, boolean>>({});

/** 懒加载单个世界缩略图（本地读取，缺失/失败保持 Map 占位）。 */
export async function loadWorldIcon(versionName: string, worldName: string) {
  if (worldIcons[worldName] || worldIconsLoading[worldName]) return;
  worldIconsLoading[worldName] = true;
  try {
    // 后端返回 { mime, base64 } 载荷（魔数嗅探 + 服务端编码，替代 JSON 数字数组）
    const icon = await invoke<{ mime: string; data: string } | null>("get_world_icon", {
      versionName,
      worldName,
    });
    if (icon) {
      worldIcons[worldName] = `data:${icon.mime};base64,${icon.data}`;
    }
  } catch {
    // 缺失/读取失败：保持 Map 占位
  } finally {
    worldIconsLoading[worldName] = false;
  }
}

// ── Mod 列表 ──

export const mods = ref<ModFileDetail[]>([]);
export const modsLoading = ref(false);
export const modsError = ref("");
/** 元数据网络查询降级标记（网络失败时列表仍显示本地信息）。 */
export const modsNetworkError = ref(false);
/** 已加载缓存的版本名（设置页切 Tab 缓存命中跳过重载）。 */
export const modsLoadedFor = ref("");
/** 正在加载的版本名（防并发重入：onMounted 与手动刷新可能同时触发）。 */
const modsLoadingFor = ref("");
/**
 * mods 目录本地快速扫描（.jar 文件，纯 read_dir 无网络）——侧边 Mod 图标显示判定用，
 * 避免等待 list_mods_detailed 的 Modrinth 元数据网络查询（1s+）导致图标延迟。
 */
export const modsQuick = ref<ResourceFile[]>([]);

/** 本地快速扫描 mods 目录 .jar 文件（与后端 scan_mod_files 过滤语义一致）。 */
export async function loadModsQuick(versionName: string) {
  try {
    const files = await invoke<ResourceFile[]>("list_resource_files", {
      versionName,
      kind: "mods",
    });
    modsQuick.value = files.filter(f => f.name.toLowerCase().endsWith(".jar"));
  } catch {
    modsQuick.value = [];
  }
}

/** 本地快速扫描结果 → 无元数据的 ModFileDetail。detailed 加载期间先渲染本地预览
 *  （避免等待 Modrinth 元数据网络查询 1s+），元数据到达后平滑替换为社区详情样式；
 *  结构与 detailed 同构，key 一致（file_name）避免列表重建。 */
function quickToDetail(files: ResourceFile[]): ModFileDetail[] {
  return files.map(f => ({
    file_name: f.name,
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

/** 单文件增量识别（一个好了就刷一个）：sha1 → Modrinth 元数据。
 *  返回项直接合并到 mods 列表对应位置（保留顺序）；None = 未命中/失败，保持本地信息。 */
export async function identifyModFile(versionName: string, fileName: string) {
  try {
    const r = await invoke<ModFileDetail | null>("identify_mod_file", {
      versionName,
      fileName,
    });
    if (!r) return;
    const idx = mods.value.findIndex(m => m.file_name === fileName);
    if (idx >= 0) mods.value[idx] = r;
  } catch {
    // 单文件识别失败：保持本地信息，不中断其余识别
  }
}

/** 加载 mods 列表：先渲染本地文件预览，再逐文件并发增量识别（每项完成立即替换，
 *  不用等全部识别完）；全部完成后标记已加载。
 *  @param reset 为 true 时强制重建本地预览并重新识别（刷新用，忽略 loadedFor 缓存）。 */
export async function loadMods(versionName: string, reset = false) {
  // 同版本已在加载（onMounted 与刷新并发）：跳过本次，由在途流程负责最终状态
  if (modsLoadingFor.value === versionName) return;
  modsLoadingFor.value = versionName;
  modsLoading.value = true;
  modsError.value = "";
  // 先用本地快速扫描立即渲染原始文件（key 与 detailed 一致，平滑升级，无 spinner 空等）
  if (reset || modsLoadedFor.value !== versionName) {
    // reset（刷新）时强制重扫本地目录，反映新增/删除的文件
    if (reset || modsQuick.value.length === 0) await loadModsQuick(versionName);
    mods.value = quickToDetail(modsQuick.value);
    modsNetworkError.value = false;
  }
  try {
    // 逐文件增量识别：分批并发（每批 8），每个完成立即 merge 到列表（不等整批）
    const files = [...mods.value.map(m => m.file_name)];
    const BATCH = 8;
    for (let i = 0; i < files.length; i += BATCH) {
      await Promise.all(files.slice(i, i + BATCH).map(f => identifyModFile(versionName, f)));
    }
    modsLoadedFor.value = versionName;
  } catch (e) {
    modsError.value = errMsg(e);
  } finally {
    modsLoading.value = false;
    modsLoadingFor.value = "";
  }
}

// ── 资源目录（材质包 / 数据包 / 光影包）──
// 双轨：quick（本地 read_dir，图标显示判定）+ detailed（Modrinth 识别，列表渲染）。

/** 详细列表结果（镜像 Rust ResourceListResult）。 */
export interface ResourceListResult {
  files: ModFileDetail[];
  network_error: boolean;
}

export const resourcepacks = ref<ModFileDetail[]>([]);
export const resourcepacksLoading = ref(false);
export const resourcepacksError = ref("");
export const resourcepacksNetworkError = ref(false);
/** 已加载缓存的版本名（设置页切 Tab 缓存命中跳过重载）。 */
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

/** 本地快速扫描（.zip 文件，纯 read_dir 无网络）——侧边图标显示判定用。 */
export const resourcepacksQuick = ref<ResourceFile[]>([]);
export const datapacksQuick = ref<ResourceFile[]>([]);
export const shaderpacksQuick = ref<ResourceFile[]>([]);

/** 单个资源目录（材质包/数据包/光影包）的全部状态 ref。 */
export interface ResourceRefs {
  /** 详细列表（Modrinth 识别，列表渲染）。 */
  list: Ref<ModFileDetail[]>;
  loading: Ref<boolean>;
  error: Ref<string>;
  networkError: Ref<boolean>;
  /** 本地快速扫描列表（.zip，图标显示判定）。 */
  quick: Ref<ResourceFile[]>;
  /** 已加载缓存的版本名（空串 = 未加载）。 */
  loadedFor: Ref<string>;
}

const RESOURCE_KINDS = ["resourcepacks", "datapacks", "shaderpacks"] as const;

/** 按 kind 聚合对应资源目录的状态 ref：原先散在 VersionDetailCard /
 *  VersionSetupResource 里的三叉 if 选择逻辑下沉于此，组件 computed 解包后直用。 */
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

/** 本地快速扫描资源目录 .zip 文件（图标判定用，无网络；失败置空）。 */
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

/** 加载资源目录详细列表（Modrinth 识别；网络失败时 files 仍含本地信息并置 network_error）。
 *  未加载过时先用本地快速扫描（quick）立即渲染原始文件，元数据到达后平滑升级。 */
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

/** 并行加载三类资源目录（快速扫描 + 详细列表）。 */
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

/** 检查更新（仅 ModLoader 版本调用）：mc/loader 由后端从版本 json 权威解析；
 * 结果覆盖 updates；网络失败仅报错不破坏既有列表。 */
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

/** 执行单个 mod 更新：下载校验替换 → 刷新本地列表 + 从更新列表移除该项。 */
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

/** 删除单个 mod 文件：删除后刷新本地列表 + 从更新列表移除该项。
 *  reset 重建列表，确保已删文件从 mods 中消失。 */
export async function deleteMod(versionName: string, fileName: string) {
  await invoke("delete_mod_file", { versionName, fileName });
  updates.value = updates.value.filter(u => u.file_name !== fileName);
  await loadMods(versionName, true);
}

/** 禁用单个 mod：后端重命名 .jar → .jar.disabled（扫描按扩展名过滤自动隐藏），
 *  成功后刷新本地列表并清出更新列表。当前列表不展示禁用项（扫描排除），无启用入口。 */
export async function toggleMod(versionName: string, fileName: string) {
  await invoke("toggle_mod_file", { versionName, fileName });
  updates.value = updates.value.filter(u => u.file_name !== fileName);
  await loadMods(versionName, true);
}

// ── 版本切换重置 ──

export function resetVersionDetail() {
  worlds.value = [];
  worldsError.value = "";
  worldsLoadedFor.value = "";
  Object.keys(worldIcons).forEach(k => delete worldIcons[k]);
  Object.keys(worldIconsLoading).forEach(k => delete worldIconsLoading[k]);
  mods.value = [];
  modsError.value = "";
  modsNetworkError.value = false;
  modsLoadedFor.value = "";
  modsLoadingFor.value = "";
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
