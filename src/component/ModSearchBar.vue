<script setup lang="ts">
// Mod 搜索表单卡片：名称 / 来源 / 版本 / 加载器 / [搜索] [重置条件]
import { computed, watch } from "vue";
import { communitySearch, modLoaderOptions } from "../store/communitySearch";
import { releases } from "../store/versionDownload";
import DropSelect, { type SelOption } from "./DropSelect.vue";

/** 来源选项（DropSelect 用）。 */
const sourceOptions: SelOption[] = [
  { value: "all", label: "全部" },
  { value: "curseforge", label: "CurseForge" },
  { value: "modrinth", label: "Modrinth" },
];

/** 版本选项：全部 + 正式版列表（releases 为 computed）。 */
const versionOptions = computed<SelOption[]>(() => [
  { value: "", label: "全部" },
  ...releases.value.map((r) => ({ value: r.id, label: r.id })),
]);

// 筛选切换即自动搜索（来源/版本/加载器；名称输入走回车）。
// watch 在微任务触发，与控件事件栈解耦（不直接在 DropSelect 回调里发起搜索）
watch(
  () => [communitySearch.source, communitySearch.gameVersion, communitySearch.loader],
  () => communitySearch.search(),
);
</script>

<template>
  <div class="card">
    <div class="card-title">搜索 Mod</div>
    <div class="card-body mod-search-form">
      <div class="mod-form-row">
        <label class="mod-form-label" for="modQueryInput">名称</label>
        <input id="modQueryInput" class="mod-input" v-model="communitySearch.query" @keydown.enter="communitySearch.search()" />
        <label class="mod-form-label" for="modSourceSelect">来源</label>
        <DropSelect
          id="modSourceSelect"
          :model-value="communitySearch.source"
          :options="sourceOptions"
          width="140px"
          @update:model-value="communitySearch.source = $event as typeof communitySearch.source"
        />
      </div>
      <div class="mod-form-row">
        <label class="mod-form-label" for="modVerSelect">版本</label>
        <DropSelect
          id="modVerSelect"
          :model-value="communitySearch.gameVersion"
          :options="versionOptions"
          class="mod-fill"
          @update:model-value="communitySearch.gameVersion = $event"
        />
        <label class="mod-form-label" for="modLoaderSelect">加载器</label>
        <DropSelect
          id="modLoaderSelect"
          :model-value="communitySearch.loader"
          :options="modLoaderOptions"
          class="mod-fill"
          @update:model-value="communitySearch.loader = $event"
        />
      </div>
      <div class="mod-form-actions">
        <button class="mod-btn mod-btn-primary" :disabled="communitySearch.searching" @click="communitySearch.search()">
          <span v-if="communitySearch.searching" class="mod-spin"></span>{{ communitySearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="mod-btn" @click="communitySearch.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mod-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.mod-form-row { display: flex; align-items: center; gap: 8px; }
.mod-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.mod-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.mod-input:focus { border-color: var(--color-brand); }
/* DropSelect 填充 flex 行（class 落根 .ds） */
.mod-form-row .mod-fill {
  flex: 1;
  min-width: 120px;
}
.mod-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.mod-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.mod-btn:hover { border-color: var(--color-brand); }
.mod-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.mod-btn-primary:hover { background-color: var(--color-brand-hover); }
.mod-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.mod-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff; border-radius: 50%; animation: dl-spin 0.6s linear infinite;
}
@keyframes dl-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
