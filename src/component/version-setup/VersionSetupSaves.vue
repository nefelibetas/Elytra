<script setup lang="ts">
// 版本设置-存档管理 tab：标题卡（打开存档文件夹）+ 列表卡（缩略图 + 名称 + 最后游玩），支持多选批量删除。
import { computed, ref, onMounted, onUnmounted } from "vue";
import { useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { Folder, Trash2, Map } from "@lucide/vue";
import { deleteWorld } from "../../store/task/versionDetail";
import { ensureGameFolder } from "../../store/config/gameDirs";
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
} from "../../store/task/versionDetail";

const route = useRoute();
const { t } = useI18n();

const versionName = computed(() => String(route.query.name ?? ""));

const { selected, selectedSet, selecting, allVisibleSelected, toggle, toggleAll, clear } =
  useSelectable(computed(() => worlds.value.map(w => w.name)));

const deleteDialog = ref(false);
const deleteLabel = ref("");
const deletePending = ref<string[]>([]);

function doBatchDelete() {
  const names = [...selected.value];
  if (names.length === 0) return;
  deleteLabel.value = names.length === 1
    ? t("settings.version.setup.saves.targetOne", { name: names[0] })
    : t("settings.version.setup.saves.targetMany", { count: names.length });
  deletePending.value = names;
  deleteDialog.value = true;
}

async function confirmDelete() {
  const names = [...deletePending.value];
  deleteDialog.value = false;
  deletePending.value = [];
  for (const n of names) {
    try {
      await deleteWorld(versionName.value, n);
      showToast(t("settings.version.setup.saves.deleted", { name: n }));
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
  // 缓存命中则跳过加载，避免 loading 闪烁
  if (worldsLoadedFor.value !== versionName.value) {
    await loadWorlds(versionName.value);
  }
  // 实时轮询存档：游戏运行中新建/删除自动反映（缩略图由 store watch 补载）
  startWorldsLivePolling(versionName.value);
});

onUnmounted(() => stopWorldsLivePolling());

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
      <span class="vss-head-title">{{ t("settings.version.setup.saves.title") }}</span>
      <button class="vss-act-btn" @click="openSavesFolder">
        <Folder :size="13" />
        <span>{{ t("settings.version.setup.saves.openFolder") }}</span>
      </button>
    </div>

    <!-- 列表卡 -->
    <div class="vss-card vss-list-card">
      <div class="vss-content">
        <Transition name="fade" mode="out-in">
          <LoadingSpinner v-if="worldsLoading" key="loading" :text="t('settings.version.setup.saves.loading')" />
          <ErrorState v-else-if="worldsError" key="err" :text="worldsError" />
          <EmptyState v-else-if="worlds.length === 0" key="empty">
            <Map :size="40" :stroke-width="1.5" opacity="0.5" />
            <p>{{ t("settings.version.setup.saves.empty") }}</p>
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
                  <span class="vss-local-meta">{{ t("settings.version.setup.saves.lastPlayed", { date: fmtUnix(w.last_modified) }) }}</span>
                </span>
              </div>
            </div>
          </div>
        </Transition>
      </div>
       </div>

    <!-- 底部 action 栏：选中 ≥1 时出现，固定列表卡底部 -->
    <Transition name="fade">
      <div v-if="selecting" class="vs-actions-bar vs-bar-boxed">
        <button class="vs-ab-btn" @click="toggleAll">
          {{ allVisibleSelected ? t("settings.version.setup.common.deselectAll") : t("settings.version.setup.common.selectAll") }}
        </button>
        <button class="vs-ab-btn" @click="clear">{{ t("settings.version.setup.common.deselect") }}</button>
        <span class="vs-ab-count">{{ t("settings.version.setup.common.selectedCount", { count: selected.length }) }}</span>
        <button class="vs-ab-btn danger" @click="doBatchDelete">
          <Trash2 :size="13" />
          <span>{{ t("settings.version.setup.common.deleteCount", { count: selected.length }) }}</span>
        </button>
      </div>
    </Transition>

    <!-- 删除确认 dialog -->
    <AlertDialog v-model="deleteDialog" :title="t('settings.version.setup.saves.deleteTitle')">
      <div class="vss-dialog-body">
        <div class="vss-dialog-text">
          {{ t("settings.version.setup.saves.deleteConfirm", { target: deleteLabel }) }}
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn danger" @click="confirmDelete">{{ t("settings.version.setup.common.delete") }}</button>
        <button class="dialog-btn" @click="deleteDialog = false">{{ t("settings.version.setup.common.cancel") }}</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style>
@import "../../styles/version-setup-shared.css";
</style>

<style scoped>
.vss {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.vss-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

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
