import { useConfirmDelete } from "../utils/useConfirmDelete";
// 版本选择页 store：游戏目录管理（增删改/激活/右键菜单）+ 目录内版本列表。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { LOADER_TITLES, LOADER_ICONS } from "../utils/versionMeta";
import { classifyVersion } from "../utils/versionClassify";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { openFolder } from "../utils/fs";
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
    // 记住游戏目录选择状态：优先恢复后端持久化的激活目录（跨重启保留）
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
    dirsError.value = errMsg(e);
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
  // toggle：同一目录菜单已展开则关闭
  if (itemMenu.value.visible && itemMenu.value.dir?.path === d.path) {
    closeItemMenu();
    return;
  }
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
    await openFolder(d.path);
  } catch (e) {
    dirsError.value = errMsg(e);
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
    dirsError.value = errMsg(e);
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
    renameError.value = errMsg(e);
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
  // 默认名称 = 目录名
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
    // 添加后自动激活新目录（刚加入的目录通常就是要用的），并加载其版本列表
    const added = dirs.value.find(d => d.path === addPath.value);
    if (added && added.path !== activePath.value) {
      await selectDir(added);
    } else if (added) {
      await loadVersions();
    }
  } catch (e) {
    addError.value = errMsg(e);
  }
}

// ── 删除版本 dialog（与版本设置页删除功能一致；复用共享 composable）──
const {
  deleteDialog, deleteTarget, openDelete, confirmDelete: runConfirmDelete,
} = useConfirmDelete(async (versionName) => {
  // 先关 dialog 并提示删除中（delete_version_dir 递归删除可能耗时数秒）
  showToast("正在删除版本…");
  try {
    await invoke("delete_version_dir", { versionName });
    showToast(`版本「${versionName}」已经删除`);
    // 删除的是当前选中版本 → 清理持久化选择
    if (selectedVersion.value === versionName) {
      clearSelected();
    }
    await loadVersions();
  } catch (e) {
    showToast(errMsg(e));
  }
});
export { deleteDialog, deleteTarget, openDelete };

export async function confirmDeleteVersion() {
  await runConfirmDelete();
}

// ── 版本列表（右侧，单卡片平铺全部版本）──

/** 重命名版本目录（versions/{old} → versions/{new}）。 */
export async function renameVersionDir(versionName: string, newName: string) {
  await invoke("rename_version_dir", { versionName, newName });
}

/** 确保版本子目录存在并返回其绝对路径（后端 ensure_game_folder：不存在则创建）。 */
export async function ensureGameFolder(
  kind: "saves" | "version" | "mods" | "resourcepacks" | "datapacks" | "shaderpacks",
  versionName: string,
): Promise<string> {
  return await invoke<string>("ensure_game_folder", { kind, versionName });
}

/** 已安装版本信息（镜像 Rust get_installed_versions；loader 为 json 推断）。 */
export interface InstalledVersionInfo {
  name: string;
  loader: string;
}

export const versions = ref<InstalledVersionInfo[]>([]);
export const versionsLoading = ref(false);
export const versionsError = ref("");

/** 版本列表展开状态（单卡片：默认展开）。 */
export const expanded = ref<Record<string, boolean>>({ all: true });

/** 版本过滤关键字（右侧列表顶部搜索框；按名称模糊匹配，大小写不敏感）。 */
export const searchQuery = ref("");

/** 全部已安装版本（单卡片平铺）：loader 优先取后端 json 推断（PCL2 实例目录名
 *  无 loader 标记），推断失败回退目录名分类；副标题为 loader 版本号或 loader 名。 */
export const allVersions = computed(() =>
  versions.value
    .map(info => {
      const cls = classifyVersion(info.name);
      const category = info.loader || cls.category;
      const title = LOADER_TITLES[category] ?? LOADER_TITLES.vanilla;
      const loaderVersion = cls.category === category ? cls.loaderVersion : "";
      return {
        name: info.name,
        category,
        title,
        loaderVersion,
        subtitle: category === "vanilla" ? "原版" : loaderVersion || title,
        icon: LOADER_ICONS[category] ?? LOADER_ICONS.vanilla,
      };
    })
    .filter(e => {
      const q = searchQuery.value.trim().toLowerCase();
      return !q || e.name.toLowerCase().includes(q);
    }),
);

export async function loadVersions() {
  versionsLoading.value = true;
  versionsError.value = "";
  try {
    versions.value = await invoke<InstalledVersionInfo[]>("get_installed_versions");
    if (!versions.value.some(v => v.name === selectedVersion.value)) selectedVersion.value = "";
  } catch (e) {
    versionsError.value = errMsg(e);
  } finally {
    versionsLoading.value = false;
  }
}

/** 选择版本 → 记录（含所属目录）；跳转由组件执行。 */
export function pickVersion(v: string) {
  setSelected(v, activePath.value || undefined);
}
