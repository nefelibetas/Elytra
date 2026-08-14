<script setup lang="ts">
// 光影包搜索表单卡片：名称（独立一行）/ 来源 / 版本 / [搜索] [重置条件]（无加载器）
import { computed } from "vue";
import { shaderSearch } from "../store/categorySearch";
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
    <div class="card-title">搜索光影包</div>
    <div class="card-body sh-search-form">
      <div class="sh-form-row">
        <label class="sh-form-label" for="shQueryInput">名称</label>
        <input id="shQueryInput" class="sh-input" v-model="shaderSearch.query" @keydown.enter="shaderSearch.search()" />
      </div>
      <div class="sh-form-row">
        <label class="sh-form-label" for="shSourceSelect">来源</label>
        <DropSelect
          id="shSourceSelect"
          :model-value="shaderSearch.source"
          :options="sourceOptions"
          width="140px"
          @update:model-value="shaderSearch.source = $event as typeof shaderSearch.source"
        />
        <label class="sh-form-label" for="shVerSelect">版本</label>
        <DropSelect
          id="shVerSelect"
          :model-value="shaderSearch.gameVersion"
          :options="versionOptions"
          class="sh-fill"
          @update:model-value="shaderSearch.gameVersion = $event"
        />
      </div>
      <div class="sh-form-actions">
        <button class="sh-btn sh-btn-primary" :disabled="shaderSearch.searching" @click="shaderSearch.search()">
          <span v-if="shaderSearch.searching" class="sh-spin"></span>{{ shaderSearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="sh-btn" @click="shaderSearch.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sh-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.sh-form-row { display: flex; align-items: center; gap: 8px; }
.sh-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.sh-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.sh-input:focus { border-color: var(--color-brand); }
/* DropSelect 填充 flex 行（class 落根 .ds） */
.sh-form-row .sh-fill {
  flex: 1;
  min-width: 120px;
}
.sh-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.sh-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.sh-btn:hover { border-color: var(--color-brand); }
.sh-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.sh-btn-primary:hover { background-color: var(--color-brand-hover); }
.sh-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.sh-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff; border-radius: 50%; animation: sh-spin 0.6s linear infinite;
}
@keyframes sh-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
