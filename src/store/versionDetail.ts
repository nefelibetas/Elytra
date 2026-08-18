// 主页版本卡片 store：存档/Mod 列表加载、Mod 更新检查与更新执行。
// 卡片组件 import 本模块单例；版本切换时 reset 后重新加载。
import { ref, computed } from "vue";
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

// ── Mod 列表 ──

export const mods = ref<ModFileDetail[]>([]);
export const modsLoading = ref(false);
export const modsError = ref("");
/** 元数据网络查询降级标记（网络失败时列表仍显示本地信息）。 */
export const modsNetworkError = ref(false);

export async function loadMods(versionName: string) {
  modsLoading.value = true;
  modsError.value = "";
  try {
    const r = await invoke<ModListResult>("list_mods_detailed", { versionName });
    mods.value = r.mods;
    modsNetworkError.value = r.network_error;
  } catch (e) {
    modsError.value = errMsg(e);
  } finally {
    modsLoading.value = false;
  }
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
  mods.value = [];
  modsError.value = "";
  modsNetworkError.value = false;
  updates.value = [];
  updatesError.value = "";
  updatesChecked.value = false;
  versionLoader.value = "";
  updating.value = [];
}
