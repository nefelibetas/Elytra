<script setup lang="ts">
// 数据包搜索表单卡片：名称（独立一行）/ 来源 / 版本 / [搜索] [重置条件]（无加载器）
import { computed } from "vue";
import { datapackSearch } from "../store/categorySearch";
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
</script>

<template>
  <div class="card">
    <div class="card-title">搜索数据包</div>
    <div class="card-body da-search-form">
      <div class="da-form-row">
        <label class="da-form-label" for="datapackQueryInput">名称</label>
        <input id="datapackQueryInput" class="da-input" v-model="datapackSearch.query" @keydown.enter="datapackSearch.search()" />
      </div>
      <div class="da-form-row">
        <label class="da-form-label" for="datapackSourceSelect">来源</label>
        <DropSelect
          id="datapackSourceSelect"
          :model-value="datapackSearch.source"
          :options="sourceOptions"
          width="140px"
          @update:model-value="datapackSearch.source = $event as typeof datapackSearch.source"
        />
        <label class="da-form-label" for="datapackVerSelect">版本</label>
        <DropSelect
          id="datapackVerSelect"
          :model-value="datapackSearch.gameVersion"
          :options="versionOptions"
          class="da-fill"
          @update:model-value="datapackSearch.gameVersion = $event"
        />
      </div>
      <div class="da-form-actions">
        <button class="da-btn da-btn-primary" :disabled="datapackSearch.searching" @click="datapackSearch.search()">
          <span v-if="datapackSearch.searching" class="da-spin"></span>{{ datapackSearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="da-btn" @click="datapackSearch.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.da-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.da-form-row { display: flex; align-items: center; gap: 8px; }
.da-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.da-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.da-input:focus { border-color: var(--color-brand); }
/* DropSelect 填充 flex 行（class 落根 .ds） */
.da-form-row .da-fill {
  flex: 1;
  min-width: 120px;
}
.da-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.da-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.da-btn:hover { border-color: var(--color-brand); }
.da-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.da-btn-primary:hover { background-color: var(--color-brand-hover); }
.da-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.da-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff; border-radius: 50%; animation: da-spin 0.6s linear infinite;
}
@keyframes da-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
