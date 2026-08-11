<script setup lang="ts">
// 数据包搜索表单卡片：名称（独立一行）/ 来源 / 版本 / [搜索] [重置条件]（无加载器）
import { datapackSearch } from "../store/categorySearch";
import { releases } from "../store/versionDownload";
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
        <select id="datapackSourceSelect" class="da-select da-source-select" v-model="datapackSearch.source">
          <option value="all">全部</option>
          <option value="curseforge">CurseForge</option>
          <option value="modrinth">Modrinth</option>
        </select>
        <label class="da-form-label" for="datapackVerSelect">版本</label>
        <select id="datapackVerSelect" class="da-select" v-model="datapackSearch.gameVersion">
          <option value="">全部</option>
          <option v-for="r in releases" :key="r.id" :value="r.id">{{ r.id }}</option>
        </select>
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
.da-select {
  flex: 1; min-width: 120px; height: 30px; padding: 0 8px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px; outline: none;
}
.da-source-select { flex: 0 0 140px; }
.da-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.da-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.da-btn:hover { border-color: var(--color-brand); }
.da-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.da-btn-primary:hover { background-color: var(--color-brand-hover, #4f46e5); }
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
