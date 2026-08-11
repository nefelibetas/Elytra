<script setup lang="ts">
// Mod 搜索表单卡片：名称 / 来源 / 版本 / 加载器 / [搜索] [重置条件]
import { modSearch, modLoaderOptions } from "../store/modSearch";
import { releases } from "../store/versionDownload";
</script>

<template>
  <div class="card">
    <div class="card-title">搜索 Mod</div>
    <div class="card-body mod-search-form">
      <div class="mod-form-row">
        <label class="mod-form-label" for="modQueryInput">名称</label>
        <input id="modQueryInput" class="mod-input" v-model="modSearch.query" @keydown.enter="modSearch.search()" />
        <label class="mod-form-label" for="modSourceSelect">来源</label>
        <select id="modSourceSelect" class="mod-select mod-source-select" v-model="modSearch.source">
          <option value="all">全部</option>
          <option value="curseforge">CurseForge</option>
          <option value="modrinth">Modrinth</option>
        </select>
      </div>
      <div class="mod-form-row">
        <label class="mod-form-label" for="modVerSelect">版本</label>
        <select id="modVerSelect" class="mod-select" v-model="modSearch.gameVersion">
          <option value="">全部</option>
          <option v-for="r in releases" :key="r.id" :value="r.id">{{ r.id }}</option>
        </select>
        <label class="mod-form-label" for="modLoaderSelect">加载器</label>
        <select id="modLoaderSelect" class="mod-select" v-model="modSearch.loader">
          <option v-for="o in modLoaderOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>
      <div class="mod-form-actions">
        <button class="mod-btn mod-btn-primary" :disabled="modSearch.searching" @click="modSearch.search()">
          <span v-if="modSearch.searching" class="mod-spin"></span>{{ modSearch.searching ? "搜索中" : "搜索" }}
        </button>
        <button class="mod-btn" @click="modSearch.reset()">重置条件</button>
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
.mod-select {
  flex: 1; min-width: 120px; height: 30px; padding: 0 8px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px; outline: none;
}
.mod-source-select { flex: 0 0 140px; }
.mod-form-actions { display: flex; justify-content: flex-end; gap: 10px; }
.mod-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.mod-btn:hover { border-color: var(--color-brand); }
.mod-btn-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.mod-btn-primary:hover { background-color: var(--color-brand-hover, #4f46e5); }
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
