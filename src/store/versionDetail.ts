// 主页版本卡片 store：存档/Mod 列表加载、Mod 更新检查与更新执行。
// 卡片组件 import 本模块单例；版本切换时 reset 后重新加载。
import { ref, computed, reactive } from "vue";
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

/** list_mods_detailed 返回（网络失败时 mods 仍含本地信息）。 */
export interface ModListResult {
  mods: ModFileDetail[];
  network_error: boolean;
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

export async function loadWorlds(versionName: string) {
  worldsLoading.value = true;
  worldsError.value = "";
  try {
    worlds.value = await invoke<WorldInfo[]>("list_worlds", { versionName });
  } catch (e) {
    worldsError.value = errMsg(e);
  } finally {
    worldsLoading.value = false;
  }
}

// ── 世界缩略图（saves/<world>/icon.png 懒加载缓存）──

/** world name → data URL（版本切换时清空）。 */
export const worldIcons = reactive<Record<string, string>>({});
/** 加载中标记（幂等，防并发重复请求）。 */
const worldIconsLoading = reactive<Record<string, boolean>>({});

/** number[] → base64 data URL（分块防大数组展开栈溢出）。 */
function bytesToDataUrl(bytes: number[]): string {
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.slice(i, i + CHUNK));
  }
  return `data:image/png;base64,${btoa(bin)}`;
}

/** 懒加载单个世界缩略图（本地读取，缺失/失败保持 Map 占位）。 */
export async function loadWorldIcon(versionName: string, worldName: string) {
  if (worldIcons[worldName] || worldIconsLoading[worldName]) return;
  worldIconsLoading[worldName] = true;
  try {
    const bytes = await invoke<number[] | null>("get_world_icon", {
      versionName,
      worldName,
    });
    if (bytes && bytes.length > 0) {
      worldIcons[worldName] = bytesToDataUrl(bytes);
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

export async function loadMods(versionName: string) {
  modsLoading.value = true;
  modsError.value = "";
  try {
    const r = await invoke<ModListResult>("list_mods_detailed", { versionName });
    mods.value = r.mods;
    modsNetworkError.value = r.network_error;
    // 与详细列表保持一致（删除/更新后图标即时正确）
    modsQuick.value = r.mods.map(m => ({ name: m.file_name, size: m.size, last_modified: 0 }));
  } catch (e) {
    modsError.value = errMsg(e);
  } finally {
    modsLoading.value = false;
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
export const datapacks = ref<ModFileDetail[]>([]);
export const datapacksLoading = ref(false);
export const datapacksError = ref("");
export const datapacksNetworkError = ref(false);
export const shaderpacks = ref<ModFileDetail[]>([]);
export const shaderpacksLoading = ref(false);
export const shaderpacksError = ref("");
export const shaderpacksNetworkError = ref(false);

/** 本地快速扫描（.zip 文件，纯 read_dir 无网络）——侧边图标显示判定用。 */
export const resourcepacksQuick = ref<ResourceFile[]>([]);
export const datapacksQuick = ref<ResourceFile[]>([]);
export const shaderpacksQuick = ref<ResourceFile[]>([]);

function resourceQuickRef(kind: ResourceKind) {
  if (kind === "resourcepacks") return resourcepacksQuick;
  if (kind === "datapacks") return datapacksQuick;
  return shaderpacksQuick;
}

function resourceDetailRefs(kind: ResourceKind) {
  if (kind === "resourcepacks") {
    return {
      list: resourcepacks,
      loading: resourcepacksLoading,
      error: resourcepacksError,
      networkError: resourcepacksNetworkError,
    };
  }
  if (kind === "datapacks") {
    return {
      list: datapacks,
      loading: datapacksLoading,
      error: datapacksError,
      networkError: datapacksNetworkError,
    };
  }
  return {
    list: shaderpacks,
    loading: shaderpacksLoading,
    error: shaderpacksError,
    networkError: shaderpacksNetworkError,
  };
}

/** 本地快速扫描资源目录 .zip 文件（图标判定用，无网络；失败置空）。 */
export async function loadResourceQuick(versionName: string, kind: ResourceKind) {
  try {
    resourceQuickRef(kind).value = await invoke<ResourceFile[]>("list_resource_files", {
      versionName,
      kind,
    });
  } catch {
    resourceQuickRef(kind).value = [];
  }
}

/** 加载资源目录详细列表（Modrinth 识别；网络失败时 files 仍含本地信息并置 network_error）。 */
export async function loadResourceDetailed(versionName: string, kind: ResourceKind) {
  const { list, loading, error, networkError } = resourceDetailRefs(kind);
  loading.value = true;
  error.value = "";
  networkError.value = false;
  try {
    const r = await invoke<ResourceListResult>("list_resource_detailed", {
      versionName,
      kind,
    });
    list.value = r.files;
    networkError.value = r.network_error;
  } catch (e) {
    error.value = errMsg(e);
  } finally {
    loading.value = false;
  }
}

/** 并行加载三类资源目录（快速扫描 + 详细列表）。 */
export function loadResourceLists(versionName: string) {
  void loadResourceQuick(versionName, "resourcepacks");
  void loadResourceQuick(versionName, "datapacks");
  void loadResourceQuick(versionName, "shaderpacks");
  void loadResourceDetailed(versionName, "resourcepacks");
  void loadResourceDetailed(versionName, "datapacks");
  void loadResourceDetailed(versionName, "shaderpacks");
}

// ── Mod 更新检查 ──

export const updates = ref<ModUpdate[]>([]);
export const checkingUpdates = ref(false);
export const updatesError = ref("");
/** 是否已执行过检查（可更新 tab 空态区分"未检查"与"已是最新"）。 */
export const updatesChecked = ref(false);

/** 检查更新（仅 ModLoader 版本调用）：结果覆盖 updates；网络失败仅报错不破坏既有列表。 */
export async function checkModUpdates(versionName: string, mcVersion: string, loader: string) {
  checkingUpdates.value = true;
  updatesError.value = "";
  try {
    updates.value = await invoke<ModUpdate[]>("check_mod_updates", {
      versionName,
      mcVersion,
      loader,
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

/** 删除单个 mod 文件：删除后刷新本地列表 + 从更新列表移除该项。 */
export async function deleteMod(versionName: string, fileName: string) {
  await invoke("delete_mod_file", { versionName, fileName });
  updates.value = updates.value.filter(u => u.file_name !== fileName);
  await loadMods(versionName);
}

// ── 版本切换重置 ──

export function resetVersionDetail() {
  worlds.value = [];
  worldsError.value = "";
  Object.keys(worldIcons).forEach(k => delete worldIcons[k]);
  Object.keys(worldIconsLoading).forEach(k => delete worldIconsLoading[k]);
  mods.value = [];
  modsError.value = "";
  modsNetworkError.value = false;
  modsQuick.value = [];
  resourcepacks.value = [];
  resourcepacksError.value = "";
  resourcepacksNetworkError.value = false;
  resourcepacksQuick.value = [];
  datapacks.value = [];
  datapacksError.value = "";
  datapacksNetworkError.value = false;
  datapacksQuick.value = [];
  shaderpacks.value = [];
  shaderpacksError.value = "";
  shaderpacksNetworkError.value = false;
  shaderpacksQuick.value = [];
  updates.value = [];
  updatesError.value = "";
  updatesChecked.value = false;
  versionLoader.value = "";
  updating.value = [];
}
