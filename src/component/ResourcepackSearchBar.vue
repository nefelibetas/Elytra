<script setup lang="ts">
// 资源包搜索表单卡片：名称（独立一行）/ 来源 / 版本 / [搜索] [重置条件]（无加载器）
import { computed, watch } from "vue";
import { resourcepackSearch } from "../store/categorySearch";
import { releases } from "../store/versionDownload";
import DropSelect, { type SelOption } from "./DropSelect.vue";

/** 来源选项（DropSelect 用）。 */
const sourceOptions: SelOption[] = [
  { value: "all", label: "全部" },
  { value: "curseforge", label: "CurseForge" },
  { value: "modrinth", label: "Modrinth" },
];

/** 版本选项：全部 + 正式版列表。 */
const versionOptions = computed<SelOption[]>(() => [
  { value: "", label: "全部" },
  ...releases.value.map((r) => ({ value: r.id, label: r.id })),
]);

// 筛选切换即自动搜索（来源/版本；名称输入走回车）。
// watch 在微任务触发，与控件事件栈解耦（不直接在 DropSelect 回调里发起搜索）
watch(
  () => [resourcepackSearch.source, resourcepackSearch.gameVersion],
  () => resourcepackSearch.search(),
);
</script>

<template>
  <div class="card">
    <div class="card-title">搜索资源包</div>
    <div class="card-body rp-search-form">
      <div class="rp-form-row">
        <label class="rp-form-label" for="rpQueryInput">名称</label>
        <input id="rpQueryInput" class="rp-input" v-model="resourcepackSearch.query" @keydown.enter="resourcepackSearch.search()" />
      </div>
      <div class="rp-form-row">
        <label class="rp-form-label" for="rpSourceSelect">来源</label>
        <DropSelect
          id="rpSourceSelect"
          :model-value="resourcepackSearch.source"
          :options="sourceOptions"
          width="140px"
          @update:model-value="resourcepackSearch.source = $event as typeof resourcepackSearch.source"
        />
        <label class="rp-form-label" for="rpVerSelect">版本</label>
        <DropSelect
          id="rpVerSelect"
          :model-value="resourcepackSearch.gameVersion"
          :options="versionOptions"
          class="rp-fill"
          @update:model-value="resourcepackSearch.gameVersion = $event"
        />
      </div>
      <div class="rp-form-actions">
        <button class="rp-btn rp-btn-primary" :disabled="resourcepackSearch.searching" @click="resourcepackSearch.search()">
          <span v-if="resourcepackSearch.searching" class="rp-spin"></span>{{ resourcepackSearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="rp-btn" @click="resourcepackSearch.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.rp-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.rp-form-row { display: flex; align-items: center; gap: 8px; }
.rp-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.rp-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.rp-input:focus { border-color: var(--color-brand); }
/* DropSelect 填充 flex 行（class 落根 .ds） */
.rp-form-row .rp-fill {
  flex: 1;
  min-width: 120px;
}
.rp-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.rp-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.rp-btn:hover { border-color: var(--color-brand); }
.rp-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.rp-btn-primary:hover { background-color: var(--color-brand-hover); }
.rp-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.rp-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff; border-radius: 50%; animation: rp-spin 0.6s linear infinite;
}
@keyframes rp-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
