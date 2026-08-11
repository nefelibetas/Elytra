<script setup lang="ts">
// 整合包搜索表单卡片：名称（独立一行）/ 来源 / 版本 / [搜索] [重置条件]（无加载器）
import { modpackSearch } from "../store/categorySearch";
import { releases } from "../store/versionDownload";
</script>

<template>
  <div class="card">
    <div class="card-title">搜索整合包</div>
    <div class="card-body mo-search-form">
      <div class="mo-form-row">
        <label class="mo-form-label" for="modpackQueryInput">名称</label>
        <input id="modpackQueryInput" class="mo-input" v-model="modpackSearch.query" @keydown.enter="modpackSearch.search()" />
      </div>
      <div class="mo-form-row">
        <label class="mo-form-label" for="modpackSourceSelect">来源</label>
        <select id="modpackSourceSelect" class="mo-select mo-source-select" v-model="modpackSearch.source">
          <option value="all">全部</option>
          <option value="curseforge">CurseForge</option>
          <option value="modrinth">Modrinth</option>
        </select>
        <label class="mo-form-label" for="modpackVerSelect">版本</label>
        <select id="modpackVerSelect" class="mo-select" v-model="modpackSearch.gameVersion">
          <option value="">全部</option>
          <option v-for="r in releases" :key="r.id" :value="r.id">{{ r.id }}</option>
        </select>
      </div>
      <div class="mo-form-actions">
        <button class="mo-btn mo-btn-primary" :disabled="modpackSearch.searching" @click="modpackSearch.search()">
          <span v-if="modpackSearch.searching" class="mo-spin"></span>{{ modpackSearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="mo-btn" @click="modpackSearch.reset()">重置条件</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mo-search-form { padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.mo-form-row { display: flex; align-items: center; gap: 8px; }
.mo-form-label { font-size: 12px; color: var(--text-secondary); flex-shrink: 0; width: 36px; }
.mo-input {
  flex: 1; height: 30px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary); font-size: 13px; outline: none;
  transition: border-color 0.15s;
}
.mo-input:focus { border-color: var(--color-brand); }
.mo-select {
  flex: 1; min-width: 120px; height: 30px; padding: 0 8px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px; outline: none;
}
.mo-source-select { flex: 0 0 140px; }
.mo-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.mo-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.mo-btn:hover { border-color: var(--color-brand); }
.mo-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.mo-btn-primary:hover { background-color: var(--color-brand-hover, #4f46e5); }
.mo-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.mo-spin {
  width: 12px; height: 12px; border: 2px solid rgba(255, 255, 255, 0.4);
  border-top-color: #fff; border-radius: 50%; animation: mo-spin 0.6s linear infinite;
}
@keyframes mo-spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
.card-body { overflow: hidden; }
</style>
