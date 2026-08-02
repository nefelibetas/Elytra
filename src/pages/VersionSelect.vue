<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import AlertDialog from "../component/AlertDialog.vue";
import VersionCard from "../component/VersionCard.vue";
import VersionItem from "../component/VersionItem.vue";
import { CATEGORY_ICONS } from "../utils/versionIcons";
import { classifyVersion } from "../utils/versionClassify";

type GameDir = { name: string; path: string };

const router = useRouter();

// ── 目录列表 ──
const dirs = ref<GameDir[]>([]);
const activePath = ref("");
const dirsError = ref("");

async function loadDirs(keepActive = true) {
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

async function selectDir(d: GameDir) {
  if (d.path === activePath.value) return;
  activePath.value = d.path;
  await invoke("set_active_game_dir", { path: d.path });
  await loadVersions();
}

// ── 版本列表（右侧，按 ModLoader/原版分类卡片）──
const versions = ref<string[]>([]);
const selectedVersion = ref("");
const versionsLoading = ref(false);
const versionsError = ref("");

const CATEGORIES = [
  { key: "vanilla", title: "原版" },
  { key: "forge", title: "Forge" },
  { key: "neoforge", title: "NeoForge" },
  { key: "fabric", title: "Fabric" },
  { key: "quilt", title: "Quilt" },
  { key: "optifine", title: "OptiFine" },
  { key: "liteloader", title: "LiteLoader" },
];

/** 分类卡片展开状态：默认全部展开。 */
const expanded = ref<Record<string, boolean>>(
  Object.fromEntries(CATEGORIES.map(c => [c.key, true])),
);

const categories = computed(() =>
  CATEGORIES.map(c => ({
    ...c,
    items: versions.value
      .map(name => ({ name, ...classifyVersion(name) }))
      .filter(e => e.category === c.key)
      .map(e => ({ ...e, icon: CATEGORY_ICONS[e.category] ?? CATEGORY_ICONS.vanilla })),
  })).filter(c => c.items.length > 0),
);

async function loadVersions() {
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

/** 选择版本 → 记录（含所属目录）并返回启动页。 */
function pickVersion(v: string) {
  selectedVersion.value = v;
  localStorage.setItem("elytra.selectedVersion", v);
  if (activePath.value) {
    localStorage.setItem("elytra.selectedVersionDir", activePath.value);
  }
  router.push("/");
}

/** 进入该版本的设置界面。 */
function openSetup(v: string) {
  router.push({ path: "/version-setup", query: { name: v } });
}

// ── 删除版本 dialog（与版本设置页删除功能一致）──
const deleteDialog = ref(false);
const deleteTarget = ref("");
const deleteError = ref("");

function openDelete(v: string) {
  deleteTarget.value = v;
  deleteError.value = "";
  deleteDialog.value = true;
}

async function confirmDeleteVersion() {
  try {
    await invoke("delete_version_dir", { versionName: deleteTarget.value });
    deleteDialog.value = false;
    // 删除的是当前选中版本 → 清理持久化选择
    if (localStorage.getItem("elytra.selectedVersion") === deleteTarget.value) {
      localStorage.removeItem("elytra.selectedVersion");
      localStorage.removeItem("elytra.selectedVersionDir");
    }
    selectedVersion.value = "";
    await loadVersions();
  } catch (e) {
    deleteError.value = String(e);
  }
}

// ── 目录项菜单（齿轮）──
const itemMenu = ref<{ visible: boolean; x: number; y: number; dir: GameDir | null }>({
  visible: false, x: 0, y: 0, dir: null,
});

function openItemMenu(e: MouseEvent, d: GameDir) {
  e.stopPropagation();
  itemMenu.value = {
    visible: true,
    x: Math.min(e.clientX + 4, window.innerWidth - 140),
    y: Math.min(e.clientY + 4, window.innerHeight - 180),
    dir: d,
  };
}

function closeItemMenu() {
  itemMenu.value.visible = false;
}

function onWindowClick() {
  closeItemMenu();
}

onMounted(() => window.addEventListener("click", onWindowClick));
onUnmounted(() => window.removeEventListener("click", onWindowClick));

/** 打开：文件管理器打开对应路径。 */
async function menuOpen() {
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
async function menuRefresh() {
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
async function menuRemove() {
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
const renameDialog = ref(false);
const renameTarget = ref<GameDir | null>(null);
const renameInput = ref("");
const renameError = ref("");

function menuRename() {
  const d = itemMenu.value.dir;
  closeItemMenu();
  if (!d) return;
  renameTarget.value = d;
  renameInput.value = d.name;
  renameError.value = "";
  renameDialog.value = true;
}

async function confirmRename() {
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
const addDialog = ref(false);
const addInput = ref("");
const addError = ref("");
const addPath = ref("");

async function addFolder() {
  const path = await open({ directory: true, multiple: false });
  if (!path) return;
  addPath.value = path;
  // 默认名称 = 文件夹名
  const segs = path.replace(/\\/g, "/").split("/").filter(Boolean);
  addInput.value = segs[segs.length - 1] || "";
  addError.value = "";
  addDialog.value = true;
}

async function confirmAdd() {
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

/** 导入整合包：暂未实现。 */
function importModpack() {
  // TODO: 暂留
}

onMounted(async () => {
  await loadDirs();
  await loadVersions();
});
</script>

<template>
  <div class="vs">
    <!-- 左侧（标题栏由窗口 TitleBar 提供，与下载管理一致） -->
    <div class="vs-left">
      <!-- 文件夹列表 -->
      <div class="vs-group-label">文件夹列表</div>
      <div class="vs-dir-list">
        <div
          v-for="d in dirs"
          :key="d.path"
          class="vs-dir-item"
          :class="{ active: d.path === activePath, 'menu-open': itemMenu.visible && itemMenu.dir?.path === d.path }"
          @click="selectDir(d)"
          @contextmenu.prevent="openItemMenu($event, d)"
        >
          <div class="vs-dir-main">
            <div class="vs-dir-title">{{ d.name }}</div>
            <div class="vs-dir-sub" :title="d.path">{{ d.path }}</div>
          </div>
          <button
            class="vs-dir-more"
            title="更多操作"
            @click.stop="openItemMenu($event, d)"
          >
            <svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor">
              <circle cx="8" cy="3" r="1.6" />
              <circle cx="8" cy="8" r="1.6" />
              <circle cx="8" cy="13" r="1.6" />
            </svg>
          </button>
        </div>
      </div>

      <!-- 添加或导入 -->
      <div class="vs-group-label">添加或导入</div>
      <div class="vs-add-list">
        <div class="vs-add-item" @click="addFolder">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
            <path d="M2 3a1 1 0 011-1h3l2 2h6a1 1 0 011 1v8a1 1 0 01-1 1H3a1 1 0 01-1-1V3z" />
          </svg>
          <span>添加已有文件夹</span>
        </div>
        <div class="vs-add-item" @click="importModpack">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
            <path d="M3 1h10a1 1 0 011 1v10a1 1 0 01-1 1H3a1 1 0 01-1-1V2a1 1 0 011-1zm1 2v8h8V3H4zm1 1h6v1H5V4zm0 2h6v1H5V6zm0 2h4v1H5V8z" />
          </svg>
          <span>导入整合包</span>
        </div>
      </div>
      <div v-if="dirsError" class="vs-err">{{ dirsError }}</div>
    </div>

    <!-- 右侧：当前目录版本列表（按 ModLoader/原版分类卡片） -->
    <div class="vs-right">
      <div v-if="versionsLoading" class="vs-hint">加载中…</div>
      <div v-else-if="versionsError" class="vs-err">{{ versionsError }}</div>
      <div v-else-if="versions.length === 0" class="vs-hint">该目录下没有已安装的游戏版本</div>
      <div v-else class="vs-cards">
        <VersionCard
          v-for="cat in categories"
          :key="cat.key"
          :title="cat.title"
          :count="cat.items.length"
          :expanded="expanded[cat.key]"
          @toggle="expanded[cat.key] = !expanded[cat.key]"
        >
          <VersionItem
            v-for="v in cat.items"
            :key="v.name"
            :icon-src="v.icon"
            :version-id="v.name"
            :subtitle="v.loaderVersion || '原版'"
            @select="pickVersion(v.name)"
          >
            <template #action>
              <button
                class="vs-item-setup"
                title="版本设置"
                @click.stop="openSetup(v.name)"
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                  <line x1="21" y1="4" x2="14" y2="4"/>
                  <line x1="10" y1="4" x2="3" y2="4"/>
                  <line x1="21" y1="12" x2="12" y2="12"/>
                  <line x1="8" y1="12" x2="3" y2="12"/>
                  <line x1="21" y1="20" x2="16" y2="20"/>
                  <line x1="12" y1="20" x2="3" y2="20"/>
                  <line x1="14" y1="2" x2="14" y2="6"/>
                  <line x1="8" y1="10" x2="8" y2="14"/>
                  <line x1="16" y1="18" x2="16" y2="22"/>
                </svg>
              </button>
              <button
                class="vs-item-del"
                title="删除版本"
                @click.stop="openDelete(v.name)"
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="3 6 5 6 21 6" />
                  <path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6" />
                  <path d="M10 11v6M14 11v6" />
                  <path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2" />
                </svg>
              </button>
            </template>
          </VersionItem>
        </VersionCard>
      </div>
    </div>

    <!-- 目录项菜单 -->
    <Teleport to="body">
      <div
        v-if="itemMenu.visible"
        class="vs-item-menu"
        :style="{ left: itemMenu.x + 'px', top: itemMenu.y + 'px' }"
        @click.stop
      >
        <div class="menu-item" @click="menuRename">重命名</div>
        <div class="menu-item" @click="menuOpen">打开</div>
        <div class="menu-item" @click="menuRemove">删除</div>
        <div class="menu-item" @click="menuRefresh">刷新</div>
      </div>
    </Teleport>

    <!-- 重命名 dialog -->
    <AlertDialog v-model="renameDialog" title="重命名文件夹" :divider="false">
      <div class="vs-dialog-body">
        <div class="vs-dialog-label">输入该文件夹在左侧列表中显示的名称</div>
        <input
          v-model="renameInput"
          class="vs-input"
          type="text"
          maxlength="32"
          @keydown.enter="confirmRename"
        />
        <div v-if="renameError" class="vs-err">{{ renameError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmRename">确定</button>
        <button class="dialog-btn" @click="renameDialog = false">取消</button>
      </template>
    </AlertDialog>

    <!-- 删除版本确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除版本" :divider="false">
      <div class="vs-dialog-body">
        <div class="vs-dialog-text">
          确定要删除版本「{{ deleteTarget }}」吗？该操作会删除版本文件夹及其全部文件，且无法恢复。
        </div>
        <div v-if="deleteError" class="vs-err">{{ deleteError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn danger" @click="confirmDeleteVersion">删除</button>
        <button class="dialog-btn" @click="deleteDialog = false">取消</button>
      </template>
    </AlertDialog>

    <!-- 添加已有文件夹 dialog -->
    <AlertDialog v-model="addDialog" title="输入显示名称" :divider="false">
      <div class="vs-dialog-body">
        <div class="vs-dialog-label">输入该文件夹在左侧列表中显示的名称</div>
        <input
          v-model="addInput"
          class="vs-input"
          type="text"
          maxlength="32"
          @keydown.enter="confirmAdd"
        />
        <div v-if="addError" class="vs-err">{{ addError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmAdd">确定</button>
        <button class="dialog-btn" @click="addDialog = false">取消</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vs {
  display: flex;
  height: 100%;
}

/* ===== 左侧 ===== */
.vs-left {
  flex: 0 0 240px;
  min-width: 220px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
}

.vs-group-label {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.8px;
  padding: 8px 8px 6px;
}

.vs-dir-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.vs-dir-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background 0.12s;
}

.vs-dir-item:hover,
.vs-dir-item.menu-open {
  background: var(--surface-hover);
}

.vs-dir-item.active {
  background: var(--color-brand-bg);
}

.vs-dir-more {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  transition: all 0.12s;
}

.vs-dir-item:hover .vs-dir-more,
.vs-dir-item.menu-open .vs-dir-more {
  opacity: 1;
}

.vs-dir-more:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.vs-dir-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vs-dir-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.vs-dir-sub {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 添加或导入 */
.vs-add-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vs-add-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.12s;
}

.vs-add-item:hover {
  background: var(--surface-hover);
  color: var(--color-brand);
}

/* ===== 右侧 ===== */
.vs-right {
  flex: 1;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface-page);
}

.vs-cards {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* 版本选择场景：item 之间不显示分割线（下载页保持原样） */
.vs-cards :deep(.ver-item + .ver-item) {
  border-top: none;
}

.vs-item-setup {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.vs-item-setup:hover {
  background-color: var(--color-brand-bg);
  color: var(--color-brand);
}

.vs-item-del {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.vs-item-del:hover {
  background-color: rgba(231, 76, 60, 0.1);
  color: #e74c3c;
}

.vs-hint {
  padding: 24px;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}

.vs-err {
  font-size: 12px;
  color: #e74c3c;
  padding: 6px 8px;
}

/* 目录项菜单 */
.vs-item-menu {
  position: fixed;
  z-index: 1000;
  min-width: 120px;
  padding: 6px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.menu-item {
  padding: 8px 12px;
  font-size: 13px;
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s;
}

.menu-item:hover {
  background: var(--color-brand);
  color: #fff;
}

/* dialog 内容 */
.vs-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.vs-dialog-label {
  font-size: 13px;
  color: var(--text-secondary);
}

.vs-dialog-text {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
}

.dialog-btn.danger {
  background: #e74c3c;
  border-color: #e74c3c;
  color: #fff;
}

.dialog-btn.danger:hover {
  opacity: 0.88;
}

.vs-input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}

.vs-input:focus {
  border-color: var(--color-brand);
}

.dialog-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.dialog-btn.primary {
  background: var(--color-brand);
  border-color: var(--color-brand);
  color: #fff;
}

.dialog-btn.primary:hover {
  opacity: 0.88;
}
</style>
