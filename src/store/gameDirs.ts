// 版本选择页 store：游戏目录管理（增删改/激活/右键菜单）+ 目录内版本列表。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { CATEGORIES } from "../utils/versionMeta";
import { classifyVersion } from "../utils/versionClassify";
import { CATEGORY_ICONS } from "../utils/versionIcons";
import { showToast } from "../utils/toast";
import { selectedVersion, setSelected, clearSelected } from "./selectedVersion";

export interface GameDir {
  name: string;
  path: string;
}

// ── 目录列表 ──
export const dirs = ref<GameDir[]>([]);
export const activePath = ref("");
export const dirsError = ref("");

export async function loadDirs(keepActive = true) {
  try {
    dirs.value = await invoke<GameDir[]>("get_game_dirs");
    // 记住文件夹选择状态：优先恢复后端持久化的激活目录（跨重启保留）
    let desired = "";
    if (keepActive) {
      try {
        const active = await invoke<GameDir>("get_active_game_dir");
        desired = active.path;
      } catch {
        // 读取失败则回退默认
      }
    }
    if (!desired || !dirs.value.some(d => d.path === desired)) {
      desired = dirs.value[0]?.path ?? "";
    }
    if (desired !== activePath.value) {
      activePath.value = desired;
      if (desired) await invoke("set_active_game_dir", { path: desired });
    }
  } catch (e) {
    dirsError.value = String(e);
  }
}

export async function selectDir(d: GameDir) {
  if (d.path === activePath.value) return;
  activePath.value = d.path;
  await invoke("set_active_game_dir", { path: d.path });
  await loadVersions();
}

// ── 目录项菜单（齿轮）──
export const itemMenu = ref<{ visible: boolean; x: number; y: number; dir: GameDir | null }>({
  visible: false, x: 0, y: 0, dir: null,
});

export function openItemMenu(e: MouseEvent, d: GameDir) {
  e.stopPropagation();
  itemMenu.value = {
    visible: true,
    x: Math.min(e.clientX + 4, window.innerWidth - 140),
    y: Math.min(e.clientY + 4, window.innerHeight - 180),
    dir: d,
  };
}

export function closeItemMenu() {
  itemMenu.value.visible = false;
}

/** window click 关闭菜单（页面 onMounted 注册监听）。 */
export function onWindowClick() {
  closeItemMenu();
}

/** 打开：文件管理器打开对应路径。 */
export async function menuOpen() {
  const d = itemMenu.value.dir;
  closeItemMenu();
  if (!d) return;
  try {
    await invoke("open_folder", { path: d.path });
  } catch (e) {
    dirsError.value = String(e);
  }
}

/** 刷新：重新读取该目录下的游戏。 */
export async function menuRefresh() {
  const d = itemMenu.value.dir;
  closeItemMenu();
  if (!d) return;
  if (d.path === activePath.value) {
    await loadVersions();
  } else {
    await selectDir(d);
  }
}

/** 删除：从配置中移除（不删文件）；若删除的是当前目录则自动切换。 */
export async function menuRemove() {
  const d = itemMenu.value.dir;
  closeItemMenu();
  if (!d) return;
  try {
    dirs.value = await invoke<GameDir[]>("remove_game_dir", { path: d.path });
    if (activePath.value === d.path) {
      activePath.value = dirs.value[0]?.path ?? "";
      if (activePath.value) {
        await invoke("set_active_game_dir", { path: activePath.value });
      }
    }
    await loadVersions();
  } catch (e) {
    dirsError.value = String(e);
  }
}

// ── 重命名 dialog ──
export const renameDialog = ref(false);
export const renameTarget = ref<GameDir | null>(null);
export const renameInput = ref("");
export const renameError = ref("");

export function menuRename() {
  const d = itemMenu.value.dir;
  closeItemMenu();
  if (!d) return;
  renameTarget.value = d;
  renameInput.value = d.name;
  renameError.value = "";
  renameDialog.value = true;
}

export async function confirmRename() {
  const t = renameTarget.value;
  if (!t) return;
  const name = renameInput.value.trim();
  if (!name) {
    renameError.value = "名称不能为空";
    return;
  }
  try {
    dirs.value = await invoke<GameDir[]>("rename_game_dir", {
      path: t.path,
      newName: name,
    });
    renameDialog.value = false;
  } catch (e) {
    renameError.value = String(e);
  }
}

// ── 添加已有文件夹 dialog ──
export const addDialog = ref(false);
export const addInput = ref("");
export const addError = ref("");
export const addPath = ref("");

export async function addFolder() {
  const path = await open({ directory: true, multiple: false });
  if (!path) return;
  addPath.value = path;
  // 默认名称 = 文件夹名
  const segs = path.replace(/\\/g, "/").split("/").filter(Boolean);
  addInput.value = segs[segs.length - 1] || "";
  addError.value = "";
  addDialog.value = true;
}

export async function confirmAdd() {
  const name = addInput.value.trim();
  if (!name) {
    addError.value = "名称不能为空";
    return;
  }
  try {
    dirs.value = await invoke<GameDir[]>("add_game_dir", {
      name,
      path: addPath.value,
    });
    addDialog.value = false;
  } catch (e) {
    addError.value = String(e);
  }
}

// ── 删除版本 dialog（与版本设置页删除功能一致）──
export const deleteDialog = ref(false);
export const deleteTarget = ref("");

export function openDelete(v: string) {
  deleteTarget.value = v;
  deleteDialog.value = true;
}

export async function confirmDeleteVersion() {
  // 先关 dialog 并提示删除中（delete_version_dir 递归删除可能耗时数秒）
  deleteDialog.value = false;
  showToast("正在删除版本…");
  try {
    await invoke("delete_version_dir", { versionName: deleteTarget.value });
    showToast(`版本「${deleteTarget.value}」已经删除`);
    // 删除的是当前选中版本 → 清理持久化选择
    if (selectedVersion.value === deleteTarget.value) {
      clearSelected();
    }
    await loadVersions();
  } catch (e) {
    showToast(String(e));
  }
}

// ── 版本列表（右侧，按 ModLoader/原版分类卡片）──
export const versions = ref<string[]>([]);
export const versionsLoading = ref(false);
export const versionsError = ref("");

/** 分类卡片展开状态：默认全部展开。 */
export const expanded = ref<Record<string, boolean>>(
  Object.fromEntries(CATEGORIES.map(c => [c.key, true])),
);

export const categories = computed(() =>
  CATEGORIES.map(c => ({
    ...c,
    items: versions.value
      .map(name => ({ name, ...classifyVersion(name) }))
      .filter(e => e.category === c.key)
      .map(e => ({ ...e, icon: CATEGORY_ICONS[e.category] ?? CATEGORY_ICONS.vanilla })),
  })).filter(c => c.items.length > 0),
);

export async function loadVersions() {
  versionsLoading.value = true;
  versionsError.value = "";
  try {
    versions.value = await invoke<string[]>("get_installed_versions");
    if (!versions.value.includes(selectedVersion.value)) selectedVersion.value = "";
  } catch (e) {
    versionsError.value = String(e);
  } finally {
    versionsLoading.value = false;
  }
}

/** 选择版本 → 记录（含所属目录）；跳转由组件执行。 */
export function pickVersion(v: string) {
  setSelected(v, activePath.value || undefined);
}
