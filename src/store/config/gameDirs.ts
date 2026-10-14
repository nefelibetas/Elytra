import type { GameDir } from "../../types/config/GameDir";
import type { InstalledVersionInfo } from "../../types/game/InstalledVersionInfo";
import { useConfirmDelete } from "../../utils/useConfirmDelete";
// 版本选择页 store：游戏目录管理（增删改/激活/右键菜单）+ 版本列表。
import { ref, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { LOADER_TITLES, LOADER_ICONS } from "../../utils/versionMeta";
import { classifyVersion } from "../../utils/versionClassify";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { openFolder } from "../../utils/fs";
import { selectedVersion, setSelected, clearSelected } from "../app/selectedVersion";
import { VERSIONS_CHANGED, type VersionsChanged } from "../app/events";

// ── 目录列表 ──
export const dirs = ref<GameDir[]>([]);
export const activePath = ref("");
export const dirsError = ref("");

export async function loadDirs(keepActive = true) {
  try {
    dirs.value = await invoke<GameDir[]>("get_game_dirs");
    // 优先恢复后端持久化的激活目录（跨重启保留）
    let desired = "";
    if (keepActive) {
      try {
        const active = await invoke<GameDir>("get_active_game_dir");
        desired = active.path;
      } catch {
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
  if (itemMenu.value.visible && itemMenu.value.dir?.path === d.path) {
    closeItemMenu();
    return;
  }
  itemMenu.value = {
    visible: true,
    x: Math.min(e.clientX + 4, window.innerWidth - 140),
    // 5 项菜单（重命名/打开/删除/刷新/导入整合包）约 170px 高，留 200px 余量
    y: Math.min(e.clientY + 4, window.innerHeight - 200),
    dir: d,
  };
}

export function closeItemMenu() {
  itemMenu.value.visible = false;
}

export function onWindowClick() {
  closeItemMenu();
}

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

// ── 删除版本 dialog（复用版本设置页共享 composable）──
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

export async function renameVersionDir(versionName: string, newName: string) {
  await invoke("rename_version_dir", { versionName, newName });
}

/** 确保版本子目录存在（后端 ensure_game_folder 不存在则创建）。 */
export async function ensureGameFolder(
  kind: "saves" | "version" | "mods" | "resourcepacks" | "datapacks" | "shaderpacks",
  versionName: string,
): Promise<string> {
  return await invoke<string>("ensure_game_folder", { kind, versionName });
}

/** 已安装版本信息（Rust `InstalledVersionInfo`：`loader` 由版本 json 推断，读不到时为空串）。 */
export type { InstalledVersionInfo };

export const versions = ref<InstalledVersionInfo[]>([]);
export const versionsLoading = ref(false);
export const versionsError = ref("");

export const expanded = ref<Record<string, boolean>>({ all: true });

export const searchQuery = ref("");

/** 全部已安装版本：loader 优先取后端 json 推断，失败回退目录名分类；副标题为 loader 版本号或名。 */
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

/** 应用后端版本列表：选中版本已不在列表时清空（与命令拉取语义一致）。 */
function applyVersions(list: InstalledVersionInfo[]) {
  versions.value = list;
  if (!list.some(v => v.name === selectedVersion.value)) selectedVersion.value = "";
}

export async function loadVersions() {
  versionsLoading.value = true;
  versionsError.value = "";
  try {
    applyVersions(await invoke<InstalledVersionInfo[]>("get_installed_versions"));
  } catch (e) {
    versionsError.value = errMsg(e);
  } finally {
    versionsLoading.value = false;
  }
}

// ── 游戏目录变化事件（后端非递归监听激活目录的 versions/ 与聚焦版本子目录）──

let versionsWatchReady = false;
/** 窗口不可见期间到达的变化：只置脏，切回窗口时补一次拉取（后台不做无谓 IO）。 */
let versionsStale = false;

/** 订阅版本列表变化（幂等；App 启动注册一次）：可见时直接用 payload 刷新，不可见只置脏。 */
export async function ensureVersionsWatch() {
  if (versionsWatchReady) return;
  versionsWatchReady = true;
  await listen<VersionsChanged>(VERSIONS_CHANGED, (ev) => {
    // 切目录前发出的迟到事件：来源目录与激活目录不一致即丢弃
    // （两边一个来自配置值、一个是解析后的绝对路径，故按归一化后缀匹配）
    const norm = (s: string) => s.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
    const active = norm(activePath.value);
    const from = norm(ev.payload.dir);
    const sameDir =
      !!active && (active === from || from.endsWith(`/${active}`) || active.endsWith(`/${from}`));
    if (!sameDir) return;
    if (document.visibilityState !== "visible") {
      versionsStale = true;
      return;
    }
    versionsStale = false;
    applyVersions(ev.payload.versions);
  });
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && versionsStale) {
      versionsStale = false;
      void loadVersions();
    }
  });
  // 聚焦版本决定后端额外监听哪些版本子目录（变化即重定向）
  watch(
    selectedVersion,
    (v) => {
      void invoke("set_watched_version", { version: v || null });
    },
    { immediate: true },
  );
}

export function pickVersion(v: string) {
  setSelected(v, activePath.value || undefined);
}
