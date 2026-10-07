<script setup lang="ts">
// 版本设置-存档管理 tab：标题卡（打开存档文件夹）+ 列表卡（世界缩略图 + 名称 + 最后游玩）。
// 多选交互：点击 item 选中，底部 action 栏全选/取消 + 批量删除。
import { computed, ref, onMounted, onUnmounted } from "vue";
import { useRoute } from "vue-router";
import { Folder, Trash2, Map } from "@lucide/vue";
import { deleteWorld } from "../../store/versionDetail";
import { ensureGameFolder } from "../../store/gameDirs";
import { openFolder } from "../../utils/fs";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { fmtUnix } from "../../utils/format";
import { useSelectable } from "../../utils/useSelectable";
import IconImage from "../common/IconImage.vue";
import AlertDialog from "../common/AlertDialog.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";
import {
  worlds, worldsLoading, worldsError, worldIcons, worldsLoadedFor,
  loadWorlds, startWorldsLivePolling, stopWorldsLivePolling,
} from "../../store/versionDetail";

const route = useRoute();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

/** 多选状态：点击 item 切换选中，底部 action 栏批量操作。 */
const { selected, selectedSet, selecting, allVisibleSelected, toggle, toggleAll, clear } =
  useSelectable(computed(() => worlds.value.map(w => w.name)));

/** 删除确认弹窗：目标名（选中单项时）或"N 项"批量文案。 */
const deleteDialog = ref(false);
const deleteLabel = ref("");
/** 待删除列表（确认后逐个消费）。 */
const deletePending = ref<string[]>([]);

/** 批量删除选中项：一次确认后逐个删除。 */
function doBatchDelete() {
  const names = [...selected.value];
  if (names.length === 0) return;
  deleteLabel.value = names.length === 1 ? `「${names[0]}」` : `${names.length} 个存档`;
  deletePending.value = names;
  deleteDialog.value = true;
}

/** 确认删除：逐个执行，完成后刷新列表并清空选中。 */
async function confirmDelete() {
  const names = [...deletePending.value];
  deleteDialog.value = false;
  deletePending.value = [];
  for (const n of names) {
    try {
      await deleteWorld(versionName.value, n);
      showToast(`已删除存档 ${n}`);
    } catch (e) {
      showToast(errMsg(e));
    }
  }
  if (names.length > 0) {
    await loadWorlds(versionName.value);
    clear();
  }
}

onMounted(async () => {
  if (!versionName.value) return;
  // 缓存命中（切 Tab 回来/刚从主页进入）不重新加载，避免 loading 闪烁
  if (worldsLoadedFor.value !== versionName.value) {
    await loadWorlds(versionName.value);
  }
  // 存档实时性轮询：游戏运行中新建/删除存档自动反映（缩略图由 store 级 watch 补载）
  startWorldsLivePolling(versionName.value);
});

onUnmounted(() => stopWorldsLivePolling());

/** 打开地图编辑器窗口（独立 WebView；初始定位到该存档）。 */

/** 打开存档文件夹（目录不存在先创建）。 */
async function openSavesFolder() {
  try {
    const target = await ensureGameFolder("saves", versionName.value);
    await openFolder(target);
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vss">
    <!-- 标题卡 -->
    <div class="vss-card vss-head-card">
      <Map :size="16" class="vss-head-icon" />
      <span class="vss-head-title">存档管理</span>
      <button class="vss-act-btn" @click="openSavesFolder">
        <Folder :size="13" />
        <span>打开存档文件夹</span>
      </button>
    </div>

    <!-- 列表卡 -->
    <div class="vss-card vss-list-card">
      <div class="vss-content">
        <Transition name="fade" mode="out-in">
          <LoadingSpinner v-if="worldsLoading" key="loading" text="加载存档…" />
          <ErrorState v-else-if="worldsError" key="err" :text="worldsError" />
          <EmptyState v-else-if="worlds.length === 0" key="empty">
            <Map :size="40" :stroke-width="1.5" opacity="0.5" />
            <p>还没有存档，启动游戏后会自动创建</p>
          </EmptyState>
          <div v-else key="list" class="vss-list">
            <div v-for="w in worlds" :key="w.name" class="vss-item">
              <div
                class="vss-item-body"
                :class="{ selected: selectedSet.has(w.name) }"
                @click="toggle(w.name)"
              >
                <IconImage
                  v-if="worldIcons[w.name]"
                  :src="worldIcons[w.name]"
                  :size="36"
                  class="vss-thumb"
                />
                <Map v-else :size="18" class="vss-local-icon" />
                <span class="vss-info">
                  <span class="vss-local-name">{{ w.name }}</span>
                  <span class="vss-local-meta">最后游玩 {{ fmtUnix(w.last_modified) }}</span>
                </span>
              </div>
            </div>
          </div>
        </Transition>
      </div>
       </div>

    <!-- 底部 action 栏：选中 ≥1 时滑出，固定列表卡底部 -->
    <Transition name="fade">
      <div v-if="selecting" class="vss-actions-bar">
        <button class="vss-ab-btn" @click="toggleAll">
          {{ allVisibleSelected ? "取消全选" : "全选" }}
        </button>
        <button class="vss-ab-btn" @click="clear">取消</button>
        <span class="vss-ab-count">已选 {{ selected.length }} 项</span>
        <button class="vss-ab-btn danger" @click="doBatchDelete">
          <Trash2 :size="13" />
          <span>删除（{{ selected.length }}）</span>
        </button>
      </div>
    </Transition>

    <!-- 删除确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除存档">
      <div class="vss-dialog-body">
        <div class="vss-dialog-text">
          确定要删除 {{ deleteLabel }} 吗？该操作会删除整个世界文件夹且无法恢复。
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
.vss {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
}

.vss-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

/* 标题卡 */
.vss-head-card {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
}

.vss-head-icon {
  flex: none;
  color: var(--color-brand);
}

.vss-head-title {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.vss-act-btn {
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

.vss-act-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

/* 列表卡（占满剩余高度，内容区内滚动） */
.vss-list-card {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.vss-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 10px 12px 14px;
}

.vss-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.vss-item {
  display: flex;
  align-items: center;
  padding: 4px 0;
}

.vss-item-body {
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

.vss-item-body:hover {
  border-color: var(--border-strong);
  background: var(--surface-hover);
}

.vss-item-body.selected {
  border-color: var(--color-brand);
}

.vss-thumb {
  flex: none;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid var(--border-default);
}

.vss-local-icon {
  flex: none;
  color: var(--text-tertiary);
}

.vss-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vss-local-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vss-local-meta {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* 底部 action 栏：选中 ≥1 时滑出，固定列表卡底部 */
.vss-actions-bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 12px 12px;
  padding: 10px 14px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
}

.vss-ab-count {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.vss-ab-btn {
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

.vss-ab-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vss-ab-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.vss-ab-btn.danger {
  border: 1px solid var(--color-danger);
  border: 1px solid color-mix(in srgb, var(--color-danger) 40%, transparent);
  background: transparent;
  color: var(--color-danger);
}

.vss-ab-btn.danger:hover:not(:disabled) {
  background: var(--color-danger-bg);
  background: color-mix(in srgb, var(--color-danger) 8%, transparent);
  border-color: var(--color-danger);
}

.vss-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 4px 2px;
}

.vss-dialog-text {
  font-size: 13px;
  color: var(--text-primary);
  line-height: 1.6;
}
</style>
