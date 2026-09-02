<script setup lang="ts">
// 社区资源搜索表单卡片（通用版）：六套 SearchBar（Mod/光影/资源/数据/整合包/地图）收敛于此。
// 差异全部 props 化：搜索状态实例、标题、来源/加载器筛选开关；整合包「导入本地」走 prepend-actions 插槽。
import { computed, watch } from "vue";
import { releases } from "../../store/versionDownload";
import { modLoaderOptions } from "../../store/communitySearch";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";

/** 搜索状态实例的最小结构（communitySearch 与 createCategorySearch 单例均满足）。 */
export interface CommunitySearchState {
  source: "all" | "modrinth" | "curseforge";
  query: string;
  gameVersion: string;
  /** 仅 Mod 搜索状态有加载器字段 */
  loader?: string;
  searching: boolean;
  search(page?: number): Promise<void> | void;
  reset(): void;
}

const props = withDefaults(
  defineProps<{
    /** 搜索状态实例（reactive 单例，直接变更其字段） */
    search: CommunitySearchState;
    /** 卡片标题，如「搜索光影包」 */
    title: string;
    /** 是否显示来源筛选（worlds 为 CurseForge 单源，不显示） */
    showSource?: boolean;
    /** 是否显示加载器筛选（仅 Mod） */
    showLoader?: boolean;
    /** 表单控件 id 前缀（同页多实例时防 id 冲突；当前 tab 互斥，默认即可） */
    idPrefix?: string;
  }>(),
  {
    showSource: true,
    showLoader: false,
    idPrefix: "cs",
  },
);

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
// watch 在微任务触发，与控件事件栈解耦（不直接在 DropSelect 回调里发起搜索）。
// 无对应字段的状态实例取常量占位（worlds 单源 / 分类搜索无加载器），不会误触发。
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
        />
        <!-- Mod 布局特例：来源与名称同行 -->
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
        <!-- 非 Mod 布局特例：来源独占行首（140px），版本填充余下空间 -->
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
        <!-- 扩展点：整合包「导入本地」等前置动作按钮 -->
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
  width: 12px; height: 12px; border: 2px solid color-mix(in srgb, var(--text-inverse) 40%, transparent);
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
