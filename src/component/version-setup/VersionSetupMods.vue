<script setup lang="ts">
// 版本设置-Mod 管理 tab：两层卡片竖直布局。
// 上层：搜索卡片（本地过滤）；下层：Tab（全部/可更新）+ 打开文件夹 + Mod items。
// 识别得到 → 详情样式；识别不到 → 本地文件样式；可更新 item 行最右侧更新按钮。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { Search, RefreshCw, Upload, Puzzle, Folder, Trash2 } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { fmtCount, fmtSize } from "../../utils/format";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { dlTranslateMode } from "../../store/downloaderConfig";
import IconImage from "../common/IconImage.vue";
import AlertDialog from "../common/AlertDialog.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";
import { useSelectable } from "../../utils/useSelectable";
import {
  mods, modsLoading, modsError, modsNetworkError, modsLoadedFor,
  updates, updatesChecked, checkingUpdates, updatesError, updatingFiles,
  loadVersionLoader, loadMods, checkModUpdates, updateMod, deleteMod, toggleMod,
} from "../../store/versionDetail";

const route = useRoute();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

/** 当前 tab：全部 | 可更新。 */
const tab = ref<"all" | "updatable">("all");

/** 搜索关键字（按标题/文件名模糊过滤）。 */
const searchQuery = ref("");

/** 更新信息按 file_name 索引（行最右侧更新按钮用）。 */
const updMap = computed<Record<string, (typeof updates.value)[number]>>(() =>
  Object.fromEntries(updates.value.map(u => [u.file_name, u])),
);

/** 搜索过滤后的 mods（对全部与可更新两个 tab 同时生效）。 */
const filteredMods = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return mods.value;
  return mods.value.filter(m =>
    (m.title_zh ?? m.title ?? m.file_name).toLowerCase().includes(q) ||
    m.file_name.toLowerCase().includes(q),
  );
});

/** 当前 tab 的 items。 */
const items = computed(() =>
  tab.value === "all" ? filteredMods.value : filteredMods.value.filter(m => updMap.value[m.file_name]),
);

/** 多选状态：点击 item 切换选中，底部 action 栏批量操作。 */
const { selected, selectedSet, selecting, allVisibleSelected, toggle, toggleAll, clear } =
  useSelectable(computed(() => items.value.map(m => m.file_name)));

/** 可更新的选中项（file_name → ModUpdate）。 */
const updatableSelected = computed(() =>
  selected.value.filter(name => updMap.value[name]).map(name => updMap.value[name]),
);

/** 批量更新选中项：逐个执行，跳过更新中；完成后清空选中。 */
async function doBatchUpdate() {
  if (updatingFiles.value.size > 0) return;
  const targets = updatableSelected.value;
  if (targets.length === 0) return;
  for (const upd of targets) {
    await updateMod(versionName.value, upd);
  }
  clear();
}

/** 删除确认弹窗：目标名（选中单项时）或"N 项"批量文案。 */
const deleteDialog = ref(false);
const deleteLabel = ref("");
/** 待删除列表（确认后逐个消费）。 */
const deletePending = ref<string[]>([]);

/** 批量删除选中项：一次确认后逐个删除。 */
function doBatchDelete() {
  const names = [...selected.value];
  if (names.length === 0) return;
  deleteLabel.value = names.length === 1 ? `「${names[0]}」` : `${names.length} 个 Mod`;
  deletePending.value = names;
  deleteDialog.value = true;
}

/** 确认删除：逐个执行，完成后刷新列表并清空选中。 */
async function confirmDelete() {
  const names = [...deletePending.value];
  deleteDialog.value = false;
  deletePending.value = [];
  for (const name of names) {
    try {
      await deleteMod(versionName.value, name);
      showToast(`已删除 ${name}`);
    } catch (e) {
      showToast(errMsg(e));
    }
  }
  if (names.length > 0) clear();
}

/** 禁用单个 mod：调用后端重命名 .jar → .jar.disabled，成功后刷新列表（禁用项被扫描过滤隐藏）。 */
async function toggleOne(fileName: string) {
  try {
    await toggleMod(versionName.value, fileName);
    showToast(`已禁用 ${fileName}`);
  } catch (e) {
    showToast(errMsg(e));
  }
}

onMounted(async () => {
  if (!versionName.value) return;
  await loadVersionLoader(versionName.value);
  // 缓存命中（切 Tab 回来/刚从主页进入）不重新加载，避免 loading 闪烁
  if (modsLoadedFor.value !== versionName.value) {
    await loadMods(versionName.value);
  }
});

async function doCheckUpdates() {
  await checkModUpdates(versionName.value);
  if (updatesError.value) {
    showToast(errMsg(updatesError.value));
  } else if (!updates.value.length) {
    showToast("所有 Mod 已是最新");
  }
}

/** 打开 mods 文件夹（目录不存在先创建）。 */
async function openModsFolder() {
  try {
    const target = await invoke<string>("ensure_game_folder", {
      kind: "mods",
      versionName: versionName.value,
    });
    await invoke("open_folder", { path: target });
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vsm">
    <!-- 上层：搜索卡片 -->
    <div class="vsm-card vsm-search-card">
      <Search :size="14" class="vsm-search-icon" />
      <input
        v-model="searchQuery"
        class="vsm-search-input"
        type="text"
        placeholder="搜索 Mod…"
      />
    </div>

    <!-- 下层：列表卡片 -->
    <div class="vsm-card vsm-list-card">
      <!-- Tab 行：全部 / 可更新 + 右侧操作 -->
      <div class="vsm-tabbar">
        <div class="vsm-tabs">
          <div
            class="vsm-tab"
            :class="{ active: tab === 'all' }"
            @click="tab = 'all'"
          >全部（{{ mods.length }}）</div>
          <div
            class="vsm-tab"
            :class="{ active: tab === 'updatable' }"
            @click="tab = 'updatable'"
          >可更新（{{ updates.length }}）</div>
        </div>
        <div class="vsm-actions">
          <button
            class="vsm-act-btn"
            :disabled="checkingUpdates || mods.length === 0"
            @click="doCheckUpdates"
          >
            <RefreshCw :size="13" :class="{ spin: checkingUpdates }" />
            <span>{{ checkingUpdates ? "检查中…" : "检查更新" }}</span>
          </button>
          <button class="vsm-act-btn" @click="openModsFolder">
            <Folder :size="13" />
            <span>打开文件夹</span>
          </button>
        </div>
      </div>

      <!-- 内容：Mod items（状态切换淡入淡出） -->
      <div class="vsm-content">
        <Transition name="fade" mode="out-in">
          <LoadingSpinner v-if="modsLoading && mods.length === 0" key="loading" text="加载 Mod 信息…" />
          <ErrorState v-else-if="modsError" key="err" :text="modsError" />
          <EmptyState v-else-if="mods.length === 0" key="empty">
            <Puzzle :size="40" :stroke-width="1.5" opacity="0.5" />
            <p>mods 目录为空，可从资源中心下载 Mod</p>
          </EmptyState>
          <EmptyState v-else-if="items.length === 0" key="filtered">
            <Puzzle :size="40" :stroke-width="1.5" opacity="0.5" />
            <p>
              {{ tab === "updatable"
                ? (updatesChecked ? "没有可更新的 Mod" : "点击检查更新查看可更新的 Mod")
                : `没有匹配「${searchQuery.trim()}」的 Mod` }}
            </p>
          </EmptyState>
          <div v-else key="list">
            <div v-if="modsNetworkError" class="vsm-net-warn">部分 Mod 信息获取失败，已显示本地信息</div>
            <div class="vsm-list">
              <div v-for="m in items" :key="m.file_name" class="vsm-item">
                <div
                  class="vsm-item-body"
                  :class="{ selected: selectedSet.has(m.file_name) }"
                  @click="toggle(m.file_name)"
                >
                  <!-- 识别得到：Modrinth 元数据详情 -->
                  <template v-if="m.project_id">
                    <IconImage :src="m.icon_url" :size="32" />
                    <span class="vsm-info">
                      <span class="vsm-title-row">
                        <span class="vsm-title">{{ m.title_zh ?? m.title }}</span>
                        <span v-if="m.title_zh" class="vsm-file">{{ m.file_name }}</span>
                        <span v-if="m.downloads" class="vsm-sub">{{ fmtCount(m.downloads) }} 次下载</span>
                      </span>
                      <span v-if="m.description" class="vsm-desc">
                        {{ dlTranslateMode === "en" ? m.description : (m.description_zh ?? m.description) }}
                      </span>
                      <span class="vsm-support">
                        <span v-if="m.current_version" class="vsm-ver">v{{ m.current_version }}</span>
                        <span v-if="updMap[m.file_name]" class="vsm-new">→ v{{ updMap[m.file_name].latest_version }}</span>
                      </span>
                    </span>
                  </template>
                  <!-- 识别不到：本地文件样式 -->
                  <template v-else>
                    <Puzzle :size="20" class="vsm-local-icon" />
                    <span class="vsm-info">
                      <span class="vsm-local-name">{{ m.file_name }}</span>
                      <span class="vsm-local-meta">{{ fmtSize(m.size) }}</span>
                    </span>
                  </template>
                </div>
                <button class="vsm-toggle-btn" @click="toggleOne(m.file_name)">禁用</button>
              </div>
            </div>
          </div>
        </Transition>
      </div>

      <!-- 底部 action 栏：选中 ≥1 时滑出，全选/取消 + 批量更新/删除 -->
      <Transition name="fade">
        <div v-if="selecting" class="vsm-actions-bar">
          <button class="vsm-ab-btn" @click="toggleAll">
            {{ allVisibleSelected ? "取消全选" : "全选" }}
          </button>
          <button class="vsm-ab-btn" @click="clear">取消</button>
          <span class="vsm-ab-count">已选 {{ selected.length }} 项</span>
          <button
            class="vsm-ab-btn update"
            :disabled="updatableSelected.length === 0 || updatingFiles.size > 0"
            @click="doBatchUpdate"
          >
            <Upload :size="13" />
            <span>更新（{{ updatableSelected.length }}）</span>
          </button>
          <button class="vsm-ab-btn danger" @click="doBatchDelete">
            <Trash2 :size="13" />
            <span>删除（{{ selected.length }}）</span>
          </button>
        </div>
      </Transition>
    </div>

    <!-- 删除 Mod 确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除 Mod">
      <div class="vsm-dialog-body">
        <div class="vsm-dialog-text">
          确定要删除 {{ deleteLabel }} 吗？该操作会删除文件且无法恢复。
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn danger" @click="confirmDelete">删除</button>
        <button class="dialog-btn" @click="deleteDialog = false">取消</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vsm {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
}

/* 卡片通用 */
.vsm-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

/* 上层：搜索卡片 */
.vsm-search-card {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
}

.vsm-search-icon {
  flex: none;
  color: var(--text-tertiary);
}

.vsm-search-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
}

.vsm-search-input::placeholder {
  color: var(--text-tertiary);
}

/* 下层：列表卡片（占满剩余高度，内容区内滚动） */
.vsm-list-card {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* Tab 行 */
.vsm-tabbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 0 14px;
  border-bottom: 1px solid var(--border-default);
  flex-wrap: wrap;
}

.vsm-tabs {
  display: flex;
  gap: 2px;
}

.vsm-tab {
  padding: 10px 12px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
  position: relative;
  transition: color 0.15s;
}

.vsm-tab:hover {
  color: var(--text-primary);
}

.vsm-tab.active {
  color: var(--color-brand);
  font-weight: 600;
}

.vsm-tab.active::after {
  content: "";
  position: absolute;
  left: 12px;
  right: 12px;
  bottom: 0;
  height: 2px;
  border-radius: 1px;
  background: var(--color-brand);
}

.vsm-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.vsm-act-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.vsm-act-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vsm-act-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.vsm-act-btn .spin {
  animation: vsm-spin 0.9s linear infinite;
}

@keyframes vsm-spin {
  to {
    transform: rotate(360deg);
  }
}

/* 内容区（卡片内滚动） */
.vsm-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 10px 12px 14px;
}

.vsm-net-warn {
  font-size: 11px;
  color: var(--color-danger);
  padding: 0 0 8px;
}

/* 列表 */
.vsm-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.vsm-item {
  display: flex;
  align-items: center;
  padding: 4px 0;
}

.vsm-item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  background: var(--surface-page);
  border: 2px solid transparent;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}

.vsm-item-body:hover {
  border-color: var(--border-strong);
  background: var(--surface-hover);
}

.vsm-item-body.selected {
  border-color: var(--color-brand);
}

/* 让位过渡移除：info 不再为 hover 按钮留空 */
.vsm-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vsm-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.vsm-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vsm-file {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 1;
}

.vsm-sub {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.vsm-desc {
  font-size: 11px;
  color: var(--text-tertiary);
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.vsm-support {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.vsm-ver {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

.vsm-new {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-danger-bg);
  color: var(--color-danger);
}

/* 本地文件样式 */
.vsm-local-icon {
  flex: none;
  color: var(--text-tertiary);
}

.vsm-local-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vsm-local-meta {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* 底部 action 栏：选中 ≥1 时滑出，固定列表卡底部 */
.vsm-actions-bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 12px 12px;
  padding: 10px 14px;
  background: var(--surface-elevated);
  border-top: 1px solid var(--border-default);
  box-shadow: var(--shadow-md);
}

.vsm-ab-count {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.vsm-ab-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.vsm-ab-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vsm-ab-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.vsm-ab-btn.update {
  border-color: var(--color-brand);
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

.vsm-ab-btn.update:hover:not(:disabled) {
  background: var(--color-brand);
  color: var(--text-inverse);
}

.vsm-ab-btn.danger {
  border: 1px solid color-mix(in srgb, var(--color-danger) 40%, transparent);
  background: transparent;
  color: var(--color-danger);
}

.vsm-ab-btn.danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--color-danger) 8%, transparent);
  border-color: var(--color-danger);
}

/* 行内禁用按钮（Mod 行最右侧；与 act 按钮同款紧凑样式） */
.vsm-toggle-btn {
  flex: none;
  margin-left: 8px;
  padding: 5px 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.vsm-toggle-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vsm-toggle-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

/* dialog */
.vsm-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.vsm-dialog-text {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
}

</style>
