<script setup lang="ts">
// 地图存档搜索结果列表：加载/错误/空态 + 结果行 + 分页器（底部栏计数最右）
import { worldsSearch, worldsTotalPages } from "../store/categorySearch";
import { dlTranslateMode } from "../store/downloaderConfig";
import { openResourceDetail } from "../store/resourceDetail";
import { fmtCount } from "../utils/format";
import IconImage from "./IconImage.vue";
</script>

<template>
  <div class="w-result-card">
    <div v-if="worldsSearch.error" class="w-state err">{{ worldsSearch.error }}</div>
    <div v-else-if="worldsSearch.searching" class="w-loading">
      <div class="spinner"></div>
      <p class="w-loading-text">{{ worldsSearch.query.trim() ? "正在搜索地图存档..." : "正在加载热门地图存档..." }}</p>
    </div>
    <div v-else-if="worldsSearch.searched && worldsSearch.results.length === 0" class="w-state">未找到相关地图存档，试试更换关键字或放宽筛选条件</div>
    <template v-else-if="worldsSearch.searched">
      <div class="w-list">
        <div v-for="m in worldsSearch.results" :key="m.key" class="w-card">
          <div class="w-card-body" @click="openResourceDetail(m, { loader: '', gameVersion: worldsSearch.gameVersion, selectedVersion: '' })">
            <IconImage :src="m.icon_url" :size="32" />
            <span class="w-info">
              <span class="w-title-row">
                <span class="w-title">{{ m.title_zh ?? m.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && m.title_zh" class="w-title-en"> | {{ m.title }}</span>
                <span v-if="worldsSearch.source === 'all'" class="w-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
                <span class="w-sub">{{ fmtCount(m.downloads) }} 次下载</span>
              </span>
              <span v-if="dlTranslateMode !== 'en'" class="w-desc">{{ m.translated ?? m.description }}</span>
              <span v-if="m.loaders.length || m.game_version_range" class="w-support">
                <span v-for="ld in m.loaders" :key="ld" class="w-ld">{{ ld }}</span>
                <span v-if="m.game_version_range" class="w-gv">{{ m.game_version_range }}</span>
              </span>
            </span>
          </div>
        </div>
      </div>
      <!-- 底部栏：分页器居中，结果计数固定最右侧 -->
      <div class="w-footer">
        <div v-if="worldsTotalPages > 1" class="w-pager">
          <button class="w-pager-btn" :disabled="worldsSearch.page <= 1" @click="worldsSearch.search(worldsSearch.page - 1)">上一页</button>
          <span class="w-pager-info">{{ worldsSearch.page }} / {{ worldsTotalPages }}</span>
          <button class="w-pager-btn" :disabled="worldsSearch.page >= worldsTotalPages" @click="worldsSearch.search(worldsSearch.page + 1)">下一页</button>
        </div>
        <span class="w-result-count">共 {{ worldsSearch.total }} 个结果</span>
      </div>
    </template>
    <div v-else class="w-state">输入关键字搜索，或直接搜索查看热门地图存档</div>
  </div>
</template>

<style scoped>
.w-result-card {
  flex: 1; min-height: 0; display: flex; flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.w-state { text-align: center; color: var(--text-tertiary); font-size: 13px; padding: 40px 0; }
.w-state.err { color: var(--color-danger); }
.w-loading { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 40px 0; }
.w-loading-text { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.w-list { display: flex; flex-direction: column; padding: 6px 8px; }
.w-card { overflow: hidden; }
.w-card + .w-card { border-top: 1px solid var(--border-subtle, var(--border-default)); }
.w-card-body {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; background: transparent;
  color: var(--text-primary); font-size: 13px; user-select: none;
  transition: background 0.1s;
}
.w-card-body:hover { background-color: var(--surface-hover); }
.w-card-body { cursor: pointer; }
.w-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.w-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.w-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.w-title-en {
  font-size: 11px; font-weight: 400; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.w-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: #fff; flex-shrink: 0; font-weight: 600; }
.w-badge.modrinth { background: #1bd96a; }
.w-badge.curseforge { background: #f16436; }
.w-sub { flex-shrink: 0; margin-left: auto; font-size: 11px; color: var(--text-tertiary); white-space: nowrap; }
.w-desc {
  font-size: 11px; color: var(--text-tertiary); line-height: 1.4;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  max-width: 100%;
}
.w-support { display: flex; align-items: center; gap: 5px; min-width: 0; }
.w-ld {
  flex-shrink: 0; font-size: 10px; font-weight: 600; padding: 0 5px;
  border-radius: 6px; background: var(--color-brand-bg); color: var(--color-brand);
}
.w-gv {
  font-size: 10px; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

/* 底部栏：分页器居中，结果计数绝对定位最右侧 */
.w-footer { position: relative; padding: 6px 16px 14px; }
.w-pager { display: flex; align-items: center; justify-content: center; gap: 12px; }
.w-result-count {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  font-size: 12px; color: var(--text-tertiary); user-select: none;
}
.w-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.w-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.w-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.w-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }
</style>
