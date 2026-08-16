<script setup lang="ts">
// 资源包搜索结果列表：加载/错误/空态 + 结果行 + 分页器（底部栏计数最右）
import { resourcepackSearch, resourcepackTotalPages } from "../store/categorySearch";
import { dlTranslateMode } from "../store/downloaderConfig";
import { openResourceDetail } from "../store/resourceDetail";
import { fmtCount } from "../utils/format";
import IconImage from "./IconImage.vue";
</script>

<template>
  <div class="rp-result-card">
    <div v-if="resourcepackSearch.error" class="rp-state err">{{ resourcepackSearch.error }}</div>
    <div v-else-if="resourcepackSearch.searching" class="rp-loading">
      <div class="spinner"></div>
      <p class="rp-loading-text">{{ resourcepackSearch.query.trim() ? "正在搜索资源包..." : "正在加载热门资源包..." }}</p>
    </div>
    <div v-else-if="resourcepackSearch.searched && resourcepackSearch.results.length === 0" class="rp-state">未找到相关资源包，试试更换关键字或放宽筛选条件</div>
    <template v-else-if="resourcepackSearch.searched">
      <div class="rp-list">
        <div v-for="m in resourcepackSearch.results" :key="m.key" class="rp-card">
          <div class="rp-card-body" @click="openResourceDetail(m, { loader: '', gameVersion: resourcepackSearch.gameVersion, selectedVersion: '' })">
            <IconImage :src="m.icon_url" :size="32" />
            <span class="rp-info">
              <span class="rp-title-row">
                <span class="rp-title">{{ m.title_zh ?? m.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && m.title_zh" class="rp-title-en"> | {{ m.title }}</span>
                <span v-if="resourcepackSearch.source === 'all'" class="rp-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
                <span class="rp-sub">{{ fmtCount(m.downloads) }} 次下载</span>
              </span>
              <span v-if="dlTranslateMode !== 'en'" class="rp-desc">{{ m.translated ?? m.description }}</span>
              <span v-if="m.loaders.length || m.game_version_range" class="rp-support">
                <span v-for="ld in m.loaders" :key="ld" class="rp-ld">{{ ld }}</span>
                <span v-if="m.game_version_range" class="rp-gv">{{ m.game_version_range }}</span>
              </span>
            </span>
          </div>
        </div>
      </div>
      <!-- 底部栏：分页器居中，结果计数固定最右侧 -->
      <div class="rp-footer">
        <div v-if="resourcepackTotalPages > 1" class="rp-pager">
          <button class="rp-pager-btn" :disabled="resourcepackSearch.page <= 1" @click="resourcepackSearch.search(resourcepackSearch.page - 1)">上一页</button>
          <span class="rp-pager-info">{{ resourcepackSearch.page }} / {{ resourcepackTotalPages }}</span>
          <button class="rp-pager-btn" :disabled="resourcepackSearch.page >= resourcepackTotalPages" @click="resourcepackSearch.search(resourcepackSearch.page + 1)">下一页</button>
        </div>
        <span class="rp-result-count">共 {{ resourcepackSearch.total }} 个结果</span>
      </div>
    </template>
    <div v-else class="rp-state">输入关键字搜索，或直接搜索查看热门资源包</div>
  </div>
</template>

<style scoped>
.rp-result-card {
  flex: 1; min-height: 0; display: flex; flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.rp-state { text-align: center; color: var(--text-tertiary); font-size: 13px; padding: 40px 0; }
.rp-state.err { color: var(--color-danger); }
.rp-loading { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 40px 0; }
.rp-loading-text { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.rp-list { display: flex; flex-direction: column; padding: 6px 8px; }
.rp-card { overflow: hidden; }
.rp-card + .rp-card { border-top: 1px solid var(--border-subtle, var(--border-default)); }
.rp-card-body {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; background: transparent;
  color: var(--text-primary); font-size: 13px; user-select: none;
  transition: background 0.1s;
}
.rp-card-body:hover { background-color: var(--surface-hover); }
.rp-card-body { cursor: pointer; }
.rp-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.rp-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.rp-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.rp-title-en {
  font-size: 11px; font-weight: 400; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.rp-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: #fff; flex-shrink: 0; font-weight: 600; }
.rp-badge.modrinth { background: #1bd96a; }
.rp-badge.curseforge { background: #f16436; }
.rp-sub { flex-shrink: 0; margin-left: auto; font-size: 11px; color: var(--text-tertiary); white-space: nowrap; }
.rp-desc {
  font-size: 11px; color: var(--text-tertiary); line-height: 1.4;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  max-width: 100%;
}
.rp-support { display: flex; align-items: center; gap: 5px; min-width: 0; }
.rp-ld {
  flex-shrink: 0; font-size: 10px; font-weight: 600; padding: 0 5px;
  border-radius: 6px; background: var(--color-brand-bg); color: var(--color-brand);
}
.rp-gv {
  font-size: 10px; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

/* 底部栏：分页器居中，结果计数绝对定位最右侧 */
.rp-footer { position: relative; padding: 6px 16px 14px; }
.rp-pager { display: flex; align-items: center; justify-content: center; gap: 12px; }
.rp-result-count {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  font-size: 12px; color: var(--text-tertiary); user-select: none;
}
.rp-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.rp-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.rp-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.rp-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }
</style>
