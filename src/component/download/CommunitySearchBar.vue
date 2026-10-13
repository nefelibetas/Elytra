<script setup lang="ts">
// 每类 xSearchBar 与 xResultList 成对，必须绑定同一 search 单例（同名的 store 搜索状态）。
import { computed, watch } from "vue";
import { releases } from "../../store/task/versionDownload";
import { modLoaderOptions } from "../../store/community/communitySearch";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";

/** 搜索状态实例的最小结构（communitySearch 与 createCategorySearch 单例均满足）。 */
export interface CommunitySearchState {
  source: "all" | "modrinth" | "curseforge";
  query: string;
  gameVersion: string;
  loader?: string;
  searching: boolean;
  search(page?: number): Promise<void> | void;
  reset(): void;
}

const props = withDefaults(
  defineProps<{
    /** 搜索状态实例（reactive 单例，直接变更其字段） */
    search: CommunitySearchState;
    title: string;
    /** 是否显示来源筛选（worlds 为 CurseForge 单源，不显示） */
    showSource?: boolean;
    showLoader?: boolean;
    /** 表单控件 id 前缀（同页多实例防 id 冲突；当前 tab 互斥） */
    idPrefix?: string;
  }>(),
  {
    showSource: true,
    showLoader: false,
    idPrefix: "cs",
  },
);

const sourceOptions: SelOption[] = [
  { value: "all", label: "全部" },
  { value: "curseforge", label: "CurseForge" },
  { value: "modrinth", label: "Modrinth" },
];

const versionOptions = computed<SelOption[]>(() => [
  { value: "", label: "全部" },
  ...releases.value.map((r) => ({ value: r.id, label: r.id })),
]);

// 来源/版本/加载器切换即自动搜索（名称走回车）；watch 在微任务触发，无对应字段的实例取常量占位不误触发。
watch(
  () => [
    props.showSource ? props.search.source : "",
    props.search.gameVersion,
    props.search.loader ?? "",
  ],
  () => props.search.search(),
);
</script>

<template>
  <div class="card">
    <div class="card-title">{{ title }}</div>
    <div class="card-body cs-search-form">
      <div class="cs-form-row">
        <label class="cs-form-label" :for="`${idPrefix}QueryInput`">名称</label>
        <input
          :id="`${idPrefix}QueryInput`"
          class="cs-input"
          v-model="search.query"
          @keydown.enter="search.search()"
          autocomplete="off"
        />
        <template v-if="showSource && showLoader">
          <label class="cs-form-label" :for="`${idPrefix}SourceSelect`">来源</label>
          <DropSelect
            :id="`${idPrefix}SourceSelect`"
            :model-value="search.source"
            :options="sourceOptions"
            width="140px"
            @update:model-value="search.source = $event as CommunitySearchState['source']"
          />
        </template>
      </div>
      <div v-if="showSource || showLoader" class="cs-form-row">
        <template v-if="showSource && !showLoader">
          <label class="cs-form-label" :for="`${idPrefix}SourceSelect`">来源</label>
          <DropSelect
            :id="`${idPrefix}SourceSelect`"
            :model-value="search.source"
            :options="sourceOptions"
            width="140px"
            @update:model-value="search.source = $event as CommunitySearchState['source']"
          />
        </template>
        <label class="cs-form-label" :for="`${idPrefix}VerSelect`">版本</label>
        <DropSelect
          :id="`${idPrefix}VerSelect`"
          :model-value="search.gameVersion"
          :options="versionOptions"
          class="cs-fill"
          @update:model-value="search.gameVersion = $event"
        />
        <template v-if="showLoader && search.loader !== undefined">
          <label class="cs-form-label" :for="`${idPrefix}LoaderSelect`">加载器</label>
          <DropSelect
            :id="`${idPrefix}LoaderSelect`"
            :model-value="search.loader"
            :options="modLoaderOptions"
            class="cs-fill"
            @update:model-value="search.loader = $event"
          />
        </template>
      </div>
      <div class="cs-form-actions">
        <slot name="prepend-actions"></slot>
        <button class="cs-btn cs-btn-primary" :disabled="search.searching" @click="search.search()">
          <span v-if="search.searching" class="cs-spin"></span>{{ search.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="cs-btn" @click="search.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cs-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.cs-form-row { display: flex; align-items: center; gap: 8px; }
.cs-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.cs-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.cs-input:focus { border-color: var(--color-brand); }
/* DropSelect 填充 flex 行（class 落根 .ds） */
.cs-form-row .cs-fill {
  flex: 1;
  min-width: 120px;
}
.cs-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.cs-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.cs-btn:hover { border-color: var(--color-brand); }
.cs-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: var(--text-inverse); }
.cs-btn-primary:hover { background-color: var(--color-brand-hover); }
.cs-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.cs-spin {
  width: 12px; height: 12px; border: 2px solid var(--text-inverse); border: 2px solid color-mix(in srgb, var(--text-inverse) 40%, transparent);
  border-top-color: var(--text-inverse); border-radius: 50%; animation: cs-spin 0.6s linear infinite;
}
@keyframes cs-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
