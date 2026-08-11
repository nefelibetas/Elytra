<script setup lang="ts">
// 整合包搜索结果列表：加载/错误/空态 + 结果行 + 分页器（底部栏计数最右）
import { modpackSearch, modpackTotalPages } from "../store/categorySearch";
import { fmtCount } from "../utils/format";
import IconImage from "./IconImage.vue";
</script>

<template>
  <div class="mo-result-card">
    <div v-if="modpackSearch.error" class="mo-state err">{{ modpackSearch.error }}</div>
    <div v-else-if="modpackSearch.searching" class="mo-loading">
      <div class="spinner"></div>
      <p class="mo-loading-text">{{ modpackSearch.query.trim() ? "正在搜索整合包..." : "正在加载热门整合包..." }}</p>
    </div>
    <div v-else-if="modpackSearch.searched && modpackSearch.results.length === 0" class="mo-state">未找到相关整合包，试试更换关键字或放宽筛选条件</div>
    <template v-else-if="modpackSearch.searched">
      <div class="mo-list">
        <div v-for="m in modpackSearch.results" :key="m.key" class="mo-card">
          <div class="mo-card-body" :title="m.description">
            <IconImage :src="m.icon_url" :size="32" />
            <span class="mo-info">
              <span class="mo-title-row">
                <span class="mo-title">{{ m.title }}</span>
                <span v-if="modpackSearch.source === 'all'" class="mo-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
              </span>
              <span class="mo-sub">{{ m.author }} · {{ fmtCount(m.downloads) }} 次下载</span>
            </span>
          </div>
        </div>
      </div>
      <!-- 底部栏：分页器居中，结果计数固定最右侧 -->
      <div class="mo-footer">
        <div v-if="modpackTotalPages > 1" class="mo-pager">
          <button class="mo-pager-btn" :disabled="modpackSearch.page <= 1" @click="modpackSearch.search(modpackSearch.page - 1)">上一页</button>
          <span class="mo-pager-info">{{ modpackSearch.page }} / {{ modpackTotalPages }}</span>
          <button class="mo-pager-btn" :disabled="modpackSearch.page >= modpackTotalPages" @click="modpackSearch.search(modpackSearch.page + 1)">下一页</button>
        </div>
        <span class="mo-result-count">共 {{ modpackSearch.total }} 个结果</span>
      </div>
    </template>
    <div v-else class="mo-state">输入关键字搜索，或直接搜索查看热门整合包</div>
  </div>
</template>

<style scoped>
.mo-result-card {
  flex: 1; min-height: 0; display: flex; flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.mo-state { text-align: center; color: var(--text-tertiary); font-size: 13px; padding: 40px 0; }
.mo-state.err { color: #e74c3c; }
.mo-loading { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 40px 0; }
.mo-loading-text { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.mo-list { display: flex; flex-direction: column; padding: 6px 8px; }
.mo-card { overflow: hidden; }
.mo-card + .mo-card { border-top: 1px solid var(--border-subtle, var(--border-default)); }
.mo-card-body {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; background: transparent;
  color: var(--text-primary); font-size: 13px; user-select: none;
  transition: background 0.1s;
}
.mo-card-body:hover { background-color: var(--surface-hover); }
.mo-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.mo-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.mo-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.mo-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: #fff; flex-shrink: 0; font-weight: 600; }
.mo-badge.modrinth { background: #1bd96a; }
.mo-badge.curseforge { background: #f16436; }
.mo-sub { font-size: 11px; color: var(--text-tertiary); }

/* 底部栏：分页器居中，结果计数绝对定位最右侧 */
.mo-footer { position: relative; padding: 6px 16px 14px; }
.mo-pager { display: flex; align-items: center; justify-content: center; gap: 12px; }
.mo-result-count {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  font-size: 12px; color: var(--text-tertiary); user-select: none;
}
.mo-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.mo-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.mo-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.mo-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }
</style>
