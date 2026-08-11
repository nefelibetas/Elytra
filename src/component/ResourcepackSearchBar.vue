<script setup lang="ts">
// 资源包搜索表单卡片：名称（独立一行）/ 来源 / 版本 / [搜索] [重置条件]（无加载器）
import { resourcepackSearch } from "../store/categorySearch";
import { releases } from "../store/versionDownload";
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
        <select id="rpSourceSelect" class="rp-select rp-source-select" v-model="resourcepackSearch.source">
          <option value="all">全部</option>
          <option value="curseforge">CurseForge</option>
          <option value="modrinth">Modrinth</option>
        </select>
        <label class="rp-form-label" for="rpVerSelect">版本</label>
        <select id="rpVerSelect" class="rp-select" v-model="resourcepackSearch.gameVersion">
          <option value="">全部</option>
          <option v-for="r in releases" :key="r.id" :value="r.id">{{ r.id }}</option>
        </select>
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
.rp-select {
  flex: 1; min-width: 120px; height: 30px; padding: 0 8px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px; outline: none;
}
.rp-source-select { flex: 0 0 140px; }
.rp-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.rp-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.rp-btn:hover { border-color: var(--color-brand); }
.rp-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.rp-btn-primary:hover { background-color: var(--color-brand-hover, #4f46e5); }
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
