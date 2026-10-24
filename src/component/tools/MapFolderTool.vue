<script setup lang="ts">
// 工具箱世界预览：选存档文件夹 → 递归发现世界根目录 → 当前详情页内嵌地图视图。
import { computed, nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Map, RefreshCw, X } from "@lucide/vue";
import { open } from "@tauri-apps/plugin-dialog";
import { resolveMapFolders, type MapFolderEntry } from "../../api/map";
import DropSelect from "../common/DropSelect.vue";
import EmptyState from "../common/EmptyState.vue";
import WorldPreviewView from "../map/WorldPreviewView.vue";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

const { t } = useI18n();

const BROWSE = "__browse__";
const busy = ref(false);
const picked = ref("");
const candidates = ref<MapFolderEntry[]>([]);
const selected = ref("");
const viewRef = ref<{ reload: () => Promise<void> } | null>(null);

const options = computed(() => [
  ...candidates.value.map((c) => ({ value: c.root, label: c.name, desc: c.root })),
  { value: BROWSE, label: t("tools.worldpreview.browse") },
]);

const selectedEntry = computed(() => candidates.value.find((c) => c.root === selected.value));

function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

const selectedLabel = computed(
  () => selectedEntry.value?.name ?? (picked.value ? baseName(picked.value) : ""),
);

let lastSelected = "";
watch(selected, (value) => {
  if (value !== BROWSE) {
    lastSelected = value;
    return;
  }
  selected.value = lastSelected;
  void pickFolder();
});

async function pickFolder() {
  if (busy.value) return;
  const path = await open({ directory: true, multiple: false });
  if (!path || Array.isArray(path)) return;
  picked.value = path;
  candidates.value = [];
  selected.value = "";
  busy.value = true;
  try {
    const list = await resolveMapFolders(path);
    candidates.value = list;
    selected.value = list[0]?.root ?? "";
    if (list.length === 0) showToast(t("tools.worldpreview.noWorlds"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    busy.value = false;
  }
}

async function refresh() {
  if (!picked.value || busy.value) return;
  busy.value = true;
  try {
    const list = await resolveMapFolders(picked.value);
    candidates.value = list;
    if (!list.some((c) => c.root === selected.value)) {
      selected.value = list[0]?.root ?? "";
    }
    if (list.length === 0) showToast(t("tools.worldpreview.noWorlds"));
    await nextTick();
    await viewRef.value?.reload();
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    busy.value = false;
  }
}

function clear() {
  picked.value = "";
  candidates.value = [];
  selected.value = "";
}
</script>

<template>
  <div class="wft">
    <div class="wft-bar">
      <div v-if="candidates.length > 1" class="wft-select">
        <DropSelect
          v-model="selected"
          :options="options"
          :placeholder="t('tools.worldpreview.selectSave')"
          :disabled="busy"
          width="100%"
        />
      </div>
      <button
        v-else
        type="button"
        class="wft-select wft-select-btn"
        :class="{ placeholder: !selectedLabel }"
        :disabled="busy"
        :title="picked || undefined"
        @click="pickFolder"
      >
        {{ selectedLabel || t("tools.worldpreview.selectSave") }}
      </button>

      <button type="button" class="wft-action" :disabled="!picked || busy" @click="refresh">
        <RefreshCw :size="14" :class="{ spin: busy }" />
        <span>{{ t("tools.worldpreview.refresh") }}</span>
      </button>
      <button type="button" class="wft-action" :disabled="!picked || busy" @click="clear">
        <X :size="14" />
        <span>{{ t("tools.worldpreview.clear") }}</span>
      </button>
    </div>

    <div v-if="selected" class="wft-map">
      <WorldPreviewView
        ref="viewRef"
        :key="selected"
        :version-name="''"
        :world-name="''"
        :folder="selected"
      />
    </div>
    <EmptyState v-else>
      <Map :size="42" :stroke-width="1.2" opacity="0.35" />
      <p>{{ t("tools.worldpreview.selectFolder") }}</p>
    </EmptyState>
  </div>
</template>

<style scoped>
.wft {
  display: flex;
  flex-direction: column;
  gap: 10px;
  flex: 1;
  min-height: 0;
}

.wft-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.wft-map {
  flex: 1;
  min-height: 0;
}

.wft-map :deep(.world-preview) {
  height: 100%;
}

.wft-select {
  flex: 1;
  min-width: 0;
}

.wft-select-btn {
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--text-primary);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: border-color 0.15s;
}

.wft-select-btn.placeholder {
  color: var(--text-tertiary);
}

.wft-select-btn:hover:not(:disabled),
.wft-select-btn:focus-visible {
  border-color: var(--color-brand);
}

.wft-select-btn:disabled {
  opacity: 0.55;
  cursor: default;
}

.wft-action {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-elevated);
  color: var(--text-secondary);
  font-size: 12.5px;
  cursor: pointer;
}

.wft-action:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.wft-action:disabled {
  opacity: 0.45;
  cursor: default;
}

.wft-action .spin {
  animation: wft-spin 0.9s linear infinite;
}

@keyframes wft-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
