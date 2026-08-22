<script setup lang="ts">
// 版本设置-Mod 管理 tab：两层卡片竖直布局。
// 上层：搜索卡片（本地过滤）；下层：Tab（全部/可更新）+ 打开文件夹 + Mod items。
// 识别得到 → 详情样式；识别不到 → 本地文件样式；可更新 item 行最右侧更新按钮。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { Search, RefreshCw, Upload, Puzzle, Folder, Trash2 } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { fmtCount, fmtSize } from "../utils/format";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { classifyVersion } from "../utils/versionClassify";
import { dlTranslateMode } from "../store/downloaderConfig";
import IconImage from "./IconImage.vue";
import AlertDialog from "./AlertDialog.vue";
import LoadingSpinner from "./LoadingSpinner.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import {
  mods, modsLoading, modsError, modsNetworkError, modsLoadedFor,
  updates, updatesChecked, checkingUpdates, updatesError, updatingFiles,
  versionLoader, loadVersionLoader, loadMods, checkModUpdates, updateMod, deleteMod,
} from "../store/versionDetail";

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

onMounted(async () => {
  if (!versionName.value) return;
  await loadVersionLoader(versionName.value);
  // 缓存命中（切 Tab 回来/刚从主页进入）不重新加载，避免 loading 闪烁
  if (modsLoadedFor.value !== versionName.value) {
    await loadMods(versionName.value);
  }
});

async function doCheckUpdates() {
  const info = classifyVersion(versionName.value);
  await checkModUpdates(
    versionName.value,
    info.mcVersion,
    versionLoader.value || info.category,
  );
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

// ── 删除 Mod dialog ──
const deleteDialog = ref(false);
const deleteTarget = ref("");

function openDelete(fileName: string) {
  deleteTarget.value = fileName;
  deleteDialog.value = true;
}

async function confirmDelete() {
  const fileName = deleteTarget.value;
  deleteDialog.value = false;
  try {
    await deleteMod(versionName.value, fileName);
    showToast(`已删除 ${fileName}`);
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
          <LoadingSpinner v-if="modsLoading" key="loading" text="加载 Mod 信息…" />
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
                <!-- 最右侧：hover 显示的更新 / 删除（更新按可更新状态禁用） -->
                <div class="vsm-item-actions">
                  <button
                    class="vsm-item-btn update"
                    :disabled="!updMap[m.file_name] || updatingFiles.has(m.file_name)"
                    @click="updMap[m.file_name] && updateMod(versionName, updMap[m.file_name])"
                  >
                    <Upload :size="13" />
                    <span>{{ updatingFiles.has(m.file_name) ? "更新中…" : "更新" }}</span>
                  </button>
                  <button class="vsm-item-btn danger" @click="openDelete(m.file_name)">
                    <Trash2 :size="13" />
                    <span>删除</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 删除 Mod 确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除 Mod">
      <div class="vsm-dialog-body">
        <div class="vsm-dialog-text">
          确定要删除 Mod「{{ deleteTarget }}」吗？该操作会删除文件且无法恢复。
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
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  transition: border-color 0.15s;
}

.vsm-item:hover {
  border-color: var(--border-strong);
}

.vsm-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  transition: padding-right 0.18s ease;
}

/* hover：文本右侧让位，给滑入的按钮留空间 */
.vsm-item:hover .vsm-info {
  padding-right: 160px;
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
  background: rgba(231, 76, 60, 0.1);
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

/* hover 操作区（更新 / 删除）：绝对定位不占布局，滑入 + 淡入 */
.vsm-item-actions {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%) translateX(10px);
  display: flex;
  align-items: center;
  gap: 6px;
  opacity: 0;
  transition: opacity 0.18s ease, transform 0.18s ease;
}

.vsm-item:hover .vsm-item-actions {
  opacity: 1;
  transform: translateY(-50%) translateX(0);
}

.vsm-item-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 12px;
  border-radius: var(--radius-sm);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.vsm-item-btn.update {
  border: 1px solid var(--color-brand);
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

.vsm-item-btn.update:hover:not(:disabled) {
  background: var(--color-brand);
  color: #fff;
}

.vsm-item-btn.update:disabled {
  opacity: 0.45;
  cursor: default;
}

.vsm-item-btn.danger {
  border: 1px solid rgba(231, 76, 60, 0.4);
  background: transparent;
  color: var(--color-danger);
}

.vsm-item-btn.danger:hover {
  background: rgba(231, 76, 60, 0.08);
  border-color: var(--color-danger);
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
