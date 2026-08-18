<script setup lang="ts">
// 地图存档搜索表单卡片：名称（独立一行）/ 游戏版本 / [搜索] [重置条件]
// CurseForge 单源（Modrinth 无 worlds project_type），无来源切换控件
import { computed, watch } from "vue";
import { worldsSearch } from "../store/categorySearch";
import { releases } from "../store/versionDownload";
import DropSelect, { type SelOption } from "./DropSelect.vue";

/** 版本选项：全部 + 正式版列表。 */
const versionOptions = computed<SelOption[]>(() => [
  { value: "", label: "全部" },
  ...releases.value.map((r) => ({ value: r.id, label: r.id })),
]);

// 版本切换即自动搜索；名称输入走回车
watch(
  () => worldsSearch.gameVersion,
  () => worldsSearch.search(),
);
</script>

<template>
  <div class="card">
    <div class="card-title">搜索地图存档</div>
    <div class="card-body w-search-form">
      <div class="w-form-row">
        <label class="w-form-label" for="wQueryInput">名称</label>
        <input id="wQueryInput" class="w-input" v-model="worldsSearch.query" @keydown.enter="worldsSearch.search()" />
      </div>
      <div class="w-form-row">
        <label class="w-form-label" for="wVerSelect">版本</label>
        <DropSelect
          id="wVerSelect"
          :model-value="worldsSearch.gameVersion"
          :options="versionOptions"
          class="w-fill"
          @update:model-value="worldsSearch.gameVersion = $event"
        />
      </div>
      <div class="w-form-actions">
        <button class="w-btn w-btn-primary" :disabled="worldsSearch.searching" @click="worldsSearch.search()">
          <span v-if="worldsSearch.searching" class="w-spin"></span>{{ worldsSearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="w-btn" @click="worldsSearch.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.w-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.w-form-row { display: flex; align-items: center; gap: 8px; }
.w-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.w-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.w-input:focus { border-color: var(--color-brand); }
/* DropSelect 填充 flex 行（class 落根 .ds） */
.w-form-row .w-fill {
  flex: 1;
  min-width: 120px;
}
.w-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.w-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.w-btn:hover { border-color: var(--color-brand); }
.w-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.w-btn-primary:hover { background-color: var(--color-brand-hover); }
.w-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.w-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff; border-radius: 50%; animation: w-spin 0.6s linear infinite;
}
@keyframes w-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
