<script setup lang="ts">
// 版本设置-资源管理 tab（材质包/数据包/光影包）：标题卡（打开文件夹）+ 列表卡。
// 双态表现：Modrinth 识别到 → 社区详情；未识别 → 本地文件 + 通用类型图标。
import { computed, onMounted } from "vue";
import { useRoute } from "vue-router";
import { Folder, Trash2, Palette, Database, Sun } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { fmtCount, fmtSize } from "../utils/format";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { dlTranslateMode } from "../store/downloaderConfig";
import IconImage from "./IconImage.vue";
import AlertDialog from "./AlertDialog.vue";
import LoadingSpinner from "./LoadingSpinner.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import { useConfirmDelete } from "../utils/useConfirmDelete";
import {
  resourceRefs,
  loadResourceDetailed, loadResourceQuick,
  type ResourceKind,
} from "../store/versionDetail";

const props = defineProps<{ kind: ResourceKind }>();

const route = useRoute();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

const META = {
  resourcepacks: { label: "材质包", icon: Palette },
  datapacks: { label: "数据包", icon: Database },
  shaderpacks: { label: "光影包", icon: Sun },
} as const;

const meta = computed(() => META[props.kind]);

/** 当前类型对应的列表状态（store 按 kind 取 ref，此处解包供模板直用）。 */
const res = computed(() => {
  const r = resourceRefs(props.kind);
  return {
    list: r.list.value,
    loading: r.loading.value,
    error: r.error.value,
    networkError: r.networkError.value,
  };
});

/** 当前类型已加载缓存的版本名。 */
const loadedFor = computed(() => resourceRefs(props.kind).loadedFor.value);

onMounted(async () => {
  if (!versionName.value) return;
  // 缓存命中（切 Tab 回来/刚从主页进入）不重新加载，避免 loading 闪烁
  if (loadedFor.value !== versionName.value) {
    await loadResourceDetailed(versionName.value, props.kind);
  }
});

/** 打开资源目录（目录不存在先创建）。 */
async function openFolder() {
  try {
    const target = await invoke<string>("ensure_game_folder", {
      kind: props.kind,
      versionName: versionName.value,
    });
    await invoke("open_folder", { path: target });
  } catch (e) {
    showToast(errMsg(e));
  }
}

// ── 删除资源 dialog ──
const { deleteDialog, deleteTarget, openDelete, confirmDelete } = useConfirmDelete(
  async fileName => {
    try {
      await invoke("delete_resource_file", {
        versionName: versionName.value,
        kind: props.kind,
        fileName,
      });
      showToast(`已删除 ${fileName}`);
      // 刷新详细列表 + 快速扫描（主页卡片图标判定保持同步）
      await loadResourceDetailed(versionName.value, props.kind);
      await loadResourceQuick(versionName.value, props.kind);
    } catch (e) {
      showToast(errMsg(e));
    }
  },
);
</script>

<template>
  <div class="vsr">
    <!-- 标题卡：类型名 + 打开文件夹 -->
    <div class="vsr-card vsr-head-card">
      <component :is="meta.icon" :size="16" class="vsr-head-icon" />
      <span class="vsr-head-title">{{ meta.label }}管理</span>
      <button class="vsr-act-btn" @click="openFolder">
        <Folder :size="13" />
        <span>打开文件夹</span>
      </button>
    </div>

    <!-- 列表卡 -->
    <div class="vsr-card vsr-list-card">
      <div class="vsr-content">
        <Transition name="fade" mode="out-in">
          <LoadingSpinner v-if="res.loading" key="loading" :text="`加载${meta.label}信息…`" />
          <ErrorState v-else-if="res.error" key="err" :text="res.error" />
          <EmptyState v-else-if="res.list.length === 0" key="empty">
            <component :is="meta.icon" :size="40" :stroke-width="1.5" opacity="0.5" />
            <p>{{ meta.label }}目录为空</p>
          </EmptyState>
          <div v-else key="list">
            <div v-if="res.networkError" class="vsr-net-warn">部分资源信息获取失败，已显示本地信息</div>
            <div class="vsr-list">
              <div v-for="f in res.list" :key="f.file_name" class="vsr-item">
                <!-- 识别到：社区详情 -->
                <template v-if="f.project_id">
                  <IconImage :src="f.icon_url" :size="32" />
                  <span class="vsr-info">
                    <span class="vsr-title-row">
                      <span class="vsr-title">{{ f.title_zh ?? f.title ?? f.file_name }}</span>
                      <span v-if="f.title_zh" class="vsr-file">{{ f.file_name }}</span>
                      <span v-if="f.downloads" class="vsr-sub">{{ fmtCount(f.downloads) }} 次下载</span>
                    </span>
                    <span v-if="f.description" class="vsr-desc">
                      {{ dlTranslateMode === "en" ? f.description : (f.description_zh ?? f.description) }}
                    </span>
                    <span v-if="f.current_version" class="vsr-support">
                      <span class="vsr-ver">v{{ f.current_version }}</span>
                    </span>
                  </span>
                </template>
                <!-- 未识别：本地文件 + 通用类型图标 -->
                <template v-else>
                  <component :is="meta.icon" :size="20" class="vsr-local-icon" />
                  <span class="vsr-info">
                    <span class="vsr-local-name">{{ f.file_name }}</span>
                    <span class="vsr-local-meta">{{ fmtSize(f.size) }}</span>
                  </span>
                </template>
                <!-- hover 删除按钮 -->
                <div class="vsr-item-actions">
                  <button class="vsr-item-btn danger" @click="openDelete(f.file_name)">
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

    <!-- 删除确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除资源">
      <div class="vsr-dialog-body">
        <div class="vsr-dialog-text">
          确定要删除「{{ deleteTarget }}」吗？该操作会删除文件且无法恢复。
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
.vsr {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-width: 640px;
}

.vsr-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

/* 标题卡 */
.vsr-head-card {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
}

.vsr-head-icon {
  flex: none;
  color: var(--color-brand);
}

.vsr-head-title {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.vsr-act-btn {
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

.vsr-act-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

/* 列表卡（占满剩余高度，内容区内滚动） */
.vsr-list-card {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.vsr-content {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 10px 12px 14px;
}

.vsr-net-warn {
  font-size: 11px;
  color: var(--color-danger);
  padding: 0 0 8px;
}

.vsr-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.vsr-item {
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

.vsr-item:hover {
  border-color: var(--border-strong);
}

.vsr-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  transition: padding-right 0.18s ease;
}

.vsr-item:hover .vsr-info {
  padding-right: 100px;
}

.vsr-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.vsr-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vsr-file {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 1;
}

.vsr-sub {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.vsr-desc {
  font-size: 11px;
  color: var(--text-tertiary);
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.vsr-support {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.vsr-ver {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

/* 本地文件样式 */
.vsr-local-icon {
  flex: none;
  color: var(--text-tertiary);
}

.vsr-local-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vsr-local-meta {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* hover 操作区 */
.vsr-item-actions {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%) translateX(10px);
  display: flex;
  align-items: center;
  gap: 6px;
  opacity: 0;
  pointer-events: none; /* 隐藏态不拦截 item 点击 */
  transition: opacity 0.18s ease, transform 0.18s ease;
}

.vsr-item:hover .vsr-item-actions {
  opacity: 1;
  transform: translateY(-50%) translateX(0);
  pointer-events: auto;
}

.vsr-item-btn {
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

.vsr-item-btn.danger {
  border: 1px solid color-mix(in srgb, var(--color-danger) 40%, transparent);
  background: transparent;
  color: var(--color-danger);
}

.vsr-item-btn.danger:hover {
  background: var(--color-danger);
  color: var(--text-inverse);
}

.vsr-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 4px 2px;
}

.vsr-dialog-text {
  font-size: 13px;
  color: var(--text-primary);
  line-height: 1.6;
}
</style>
