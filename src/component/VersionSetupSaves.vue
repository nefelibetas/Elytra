<script setup lang="ts">
// 版本设置-存档管理 tab：标题卡（打开存档文件夹）+ 列表卡（世界缩略图 + 名称 + 最后游玩 + 删除）。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { Folder, Trash2, Map } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import IconImage from "./IconImage.vue";
import AlertDialog from "./AlertDialog.vue";
import LoadingSpinner from "./LoadingSpinner.vue";
import {
  worlds, worldsLoading, worldsError, worldIcons, worldsLoadedFor,
  loadWorlds, loadWorldIcon,
} from "../store/versionDetail";

const route = useRoute();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

onMounted(async () => {
  if (!versionName.value) return;
  // 缓存命中（切 Tab 回来/刚从主页进入）不重新加载，避免 loading 闪烁
  if (worldsLoadedFor.value !== versionName.value) {
    await loadWorlds(versionName.value);
    for (const w of worlds.value) void loadWorldIcon(versionName.value, w.name);
  }
});

/** unix 秒 → "yyyy-MM-dd HH:mm" */
function fmtUnix(sec: number): string {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 打开存档文件夹（目录不存在先创建）。 */
async function openFolder() {
  try {
    const target = await invoke<string>("ensure_game_folder", {
      kind: "saves",
      versionName: versionName.value,
    });
    await invoke("open_folder", { path: target });
  } catch (e) {
    showToast(errMsg(e));
  }
}

// ── 删除存档 dialog ──
const deleteDialog = ref(false);
const deleteTarget = ref("");

function openDelete(worldName: string) {
  deleteTarget.value = worldName;
  deleteDialog.value = true;
}

async function confirmDelete() {
  const worldName = deleteTarget.value;
  deleteDialog.value = false;
  try {
    await invoke("delete_world", {
      versionName: versionName.value,
      worldName,
    });
    showToast(`已删除存档 ${worldName}`);
    await loadWorlds(versionName.value);
    for (const w of worlds.value) void loadWorldIcon(versionName.value, w.name);
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
      <button class="vss-act-btn" @click="openFolder">
        <Folder :size="13" />
        <span>打开存档文件夹</span>
      </button>
    </div>

    <!-- 列表卡 -->
    <div class="vss-card vss-list-card">
      <div class="vss-content">
        <Transition name="fade" mode="out-in">
          <LoadingSpinner v-if="worldsLoading" key="loading" text="加载存档…" />
          <div v-else-if="worldsError" key="err" class="vss-state err">{{ worldsError }}</div>
          <div v-else-if="worlds.length === 0" key="empty" class="vss-state">还没有存档，启动游戏后会自动创建</div>
          <div v-else key="list" class="vss-list">
            <div v-for="w in worlds" :key="w.name" class="vss-item">
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
              <div class="vss-item-actions">
                <button class="vss-item-btn danger" @click="openDelete(w.name)">
                  <Trash2 :size="13" />
                  <span>删除</span>
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 删除确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除存档" :divider="false">
      <div class="vss-dialog-body">
        <div class="vss-dialog-text">
          确定要删除存档「{{ deleteTarget }}」吗？该操作会删除整个世界文件夹且无法恢复。
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

.vss-state {
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
  padding: 36px 0;
}

.vss-state.err {
  color: var(--color-danger);
}

.vss-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.vss-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  transition: border-color 0.15s;
}

.vss-item:hover {
  border-color: var(--border-strong);
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
  transition: padding-right 0.18s ease;
}

.vss-item:hover .vss-info {
  padding-right: 100px;
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

/* hover 操作区 */
.vss-item-actions {
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

.vss-item:hover .vss-item-actions {
  opacity: 1;
  transform: translateY(-50%) translateX(0);
}

.vss-item-btn {
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

.vss-item-btn.danger {
  border: 1px solid rgba(231, 76, 60, 0.4);
  background: transparent;
  color: var(--color-danger);
}

.vss-item-btn.danger:hover {
  background: var(--color-danger);
  color: #fff;
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
