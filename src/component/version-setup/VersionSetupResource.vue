<script setup lang="ts">
// 版本设置-资源管理 tab（材质包/数据包/光影包）：标题卡（打开文件夹）+ 列表卡。
// 双态表现：Modrinth 识别到 → 社区详情；未识别 → 本地文件 + 通用类型图标。
// 多选交互：点击 item 选中，底部 action 栏全选/取消 + 批量删除。
import { computed, ref, onMounted } from "vue";
import { useRoute } from "vue-router";
import { Folder, Trash2, Palette, Database, Sun } from "@lucide/vue";
import { deleteResourceFile } from "../../store/versionDetail";
import { ensureGameFolder } from "../../store/gameDirs";
import { openFolder } from "../../utils/fs";
import { fmtCount, fmtSize } from "../../utils/format";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { dlTranslateMode } from "../../store/downloaderConfig";
import { useSelectable } from "../../utils/useSelectable";
import IconImage from "../common/IconImage.vue";
import AlertDialog from "../common/AlertDialog.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";
import {
  resourceRefs,
  loadResourceDetailed, loadResourceQuick,
  type ResourceKind,
} from "../../store/versionDetail";

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

/** 多选状态：点击 item 切换选中，底部 action 栏批量操作。 */
const { selected, selectedSet, selecting, allVisibleSelected, toggle, toggleAll, clear } =
  useSelectable(computed(() => res.value.list.map(f => f.file_name)));

/** 删除确认弹窗：目标名（选中单项时）或"N 项"批量文案。 */
const deleteDialog = ref(false);
const deleteLabel = ref("");
/** 待删除列表（确认后逐个消费）。 */
const deletePending = ref<string[]>([]);

/** 批量删除选中项：一次确认后逐个删除。 */
function doBatchDelete() {
  const names = [...selected.value];
  if (names.length === 0) return;
  deleteLabel.value = names.length === 1 ? `「${names[0]}」` : `${names.length} 项资源`;
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
      await deleteResourceFile(versionName.value, props.kind, n);
      showToast(`已删除 ${n}`);
    } catch (e) {
      showToast(errMsg(e));
    }
  }
  if (names.length > 0) {
    // 刷新详细列表 + 快速扫描（主页卡片图标判定保持同步）
    await loadResourceDetailed(versionName.value, props.kind);
    await loadResourceQuick(versionName.value, props.kind);
    clear();
  }
}

onMounted(async () => {
  if (!versionName.value) return;
  // 缓存命中（切 Tab 回来/刚从主页进入）不重新加载，避免 loading 闪烁
  if (loadedFor.value !== versionName.value) {
    await loadResourceDetailed(versionName.value, props.kind);
  }
});

/** 打开资源目录（目录不存在先创建）。 */
async function openResourceFolder() {
  try {
    const target = await ensureGameFolder(props.kind, versionName.value);
    await openFolder(target);
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vsr">
    <!-- 标题卡：类型名 + 打开文件夹 -->
    <div class="vsr-card vsr-head-card">
      <component :is="meta.icon" :size="16" class="vsr-head-icon" />
      <span class="vsr-head-title">{{ meta.label }}管理</span>
      <button class="vsr-act-btn" @click="openResourceFolder">
        <Folder :size="13" />
        <span>打开文件夹</span>
      </button>
    </div>

    <!-- 列表卡 -->
    <div class="vsr-card vsr-list-card">
      <div class="vsr-content">
        <Transition name="fade" mode="out-in">
          <LoadingSpinner v-if="res.loading && res.list.length === 0" key="loading" :text="`加载${meta.label}信息…`" />
          <ErrorState v-else-if="res.error" key="err" :text="res.error" />
          <EmptyState v-else-if="res.list.length === 0" key="empty">
            <component :is="meta.icon" :size="40" :stroke-width="1.5" opacity="0.5" />
            <p>{{ meta.label }}目录为空</p>
          </EmptyState>
          <div v-else key="list">
            <div v-if="res.networkError" class="vsr-net-warn">部分资源信息获取失败，已显示本地信息</div>
            <div class="vsr-list">
              <div v-for="f in res.list" :key="f.file_name" class="vsr-item">
                <div
                  class="vsr-item-body"
                  :class="{ selected: selectedSet.has(f.file_name) }"
                  @click="toggle(f.file_name)"
                >
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
                </div>
              </div>
            </div>
          </div>
        </Transition>
      </div>
       </div>

    <!-- 底部 action 栏：选中 ≥1 时滑出，全选/取消 + 批量删除 -->
    <Transition name="fade">
      <div v-if="selecting" class="vsr-actions-bar">
        <button class="vsr-ab-btn" @click="toggleAll">
          {{ allVisibleSelected ? "取消全选" : "全选" }}
        </button>
        <button class="vsr-ab-btn" @click="clear">取消</button>
        <span class="vsr-ab-count">已选 {{ selected.length }} 项</span>
        <button class="vsr-ab-btn danger" @click="doBatchDelete">
          <Trash2 :size="13" />
          <span>删除（{{ selected.length }}）</span>
        </button>
      </div>
    </Transition>

    <!-- 删除确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除资源">
      <div class="vsr-dialog-body">
        <div class="vsr-dialog-text">
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
  display: flex;
  align-items: center;
  padding: 4px 0;
}

.vsr-item-body {
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

.vsr-item-body:hover {
  border-color: var(--border-strong);
  background: var(--surface-hover);
}

.vsr-item-body.selected {
  border-color: var(--color-brand);
}

.vsr-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
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

/* 底部 action 栏：选中 ≥1 时滑出，固定列表卡底部 */
.vsr-actions-bar {
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

.vsr-ab-count {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}

.vsr-ab-btn {
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

.vsr-ab-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vsr-ab-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.vsr-ab-btn.danger {
  border: 1px solid var(--color-danger);
  border: 1px solid color-mix(in srgb, var(--color-danger) 40%, transparent);
  background: transparent;
  color: var(--color-danger);
}

.vsr-ab-btn.danger:hover:not(:disabled) {
  background: var(--color-danger-bg);
  background: color-mix(in srgb, var(--color-danger) 8%, transparent);
  border-color: var(--color-danger);
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
