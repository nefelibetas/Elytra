<script setup lang="ts">
// Mod 搜索结果列表：加载/错误/空态 + 结果行 + 分页器（底部栏计数最右）
import { communitySearch, communityTotalPages } from "../store/communitySearch";
import { dlTranslateMode } from "../store/downloaderConfig";
import { openResourceDetail } from "../store/resourceDetail";
import { fmtCount } from "../utils/format";
import IconImage from "./IconImage.vue";
</script>

<template>
  <div class="mod-result-card">
    <div v-if="communitySearch.error" class="mod-state err">{{ communitySearch.error }}</div>
    <div v-else-if="communitySearch.searching" class="mod-loading">
      <div class="spinner"></div>
      <p class="mod-loading-text">{{ communitySearch.query.trim() ? "正在搜索 Mod..." : "正在加载热门 Mod..." }}</p>
    </div>
    <div v-else-if="communitySearch.searched && communitySearch.results.length === 0" class="mod-state">未找到相关 Mod，试试更换关键字或放宽筛选条件</div>
    <template v-else-if="communitySearch.searched">
      <div class="mod-list">
        <div v-for="m in communitySearch.results" :key="m.key" class="mod-card">
          <div class="mod-card-body" @click="openResourceDetail(m, { loader: communitySearch.loader, gameVersion: communitySearch.gameVersion, selectedVersion: '' })">
            <IconImage :src="m.icon_url" :size="32" />
            <span class="mod-info">
              <span class="mod-title-row">
                <span class="mod-title">{{ m.title_zh ?? m.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && m.title_zh" class="mod-title-en"> | {{ m.title }}</span>
                <span v-if="communitySearch.source === 'all'" class="mod-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
                <span class="mod-sub">{{ fmtCount(m.downloads) }} 次下载</span>
              </span>
              <span v-if="dlTranslateMode !== 'en'" class="mod-desc">{{ m.translated ?? m.description }}</span>
              <span v-if="m.loaders.length || m.game_version_range" class="mod-support">
                <span v-for="ld in m.loaders" :key="ld" class="mod-ld">{{ ld }}</span>
                <span v-if="m.game_version_range" class="mod-gv">{{ m.game_version_range }}</span>
              </span>
            </span>
          </div>
        </div>
      </div>
      <!-- 底部栏：分页器居中，结果计数固定最右侧 -->
      <div class="mod-footer">
        <div v-if="communityTotalPages > 1" class="mod-pager">
          <button class="mod-pager-btn" :disabled="communitySearch.page <= 1" @click="communitySearch.search(communitySearch.page - 1)">上一页</button>
          <span class="mod-pager-info">{{ communitySearch.page }} / {{ communityTotalPages }}</span>
          <button class="mod-pager-btn" :disabled="communitySearch.page >= communityTotalPages" @click="communitySearch.search(communitySearch.page + 1)">下一页</button>
        </div>
        <span class="mod-result-count">共 {{ communitySearch.total }} 个结果</span>
      </div>
    </template>
    <div v-else class="mod-state">输入关键字搜索，或直接搜索查看热门 Mod</div>
  </div>
</template>

<style scoped>
.mod-result-card {
  flex: 1; min-height: 0; display: flex; flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.mod-state { text-align: center; color: var(--text-tertiary); font-size: 13px; padding: 40px 0; }
.mod-state.err { color: var(--color-danger); }
.mod-loading { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 40px 0; }
.mod-loading-text { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.mod-list { display: flex; flex-direction: column; padding: 6px 8px; }
.mod-card { overflow: hidden; }
.mod-card + .mod-card { border-top: 1px solid var(--border-subtle, var(--border-default)); }
.mod-card-body {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; background: transparent;
  color: var(--text-primary); font-size: 13px; user-select: none;
  transition: background 0.1s;
}
.mod-card-body:hover { background-color: var(--surface-hover); }
.mod-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.mod-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.mod-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.mod-title-en {
  font-size: 11px; font-weight: 400; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.mod-card-body { cursor: pointer; }
.mod-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: #fff; flex-shrink: 0; font-weight: 600; }
.mod-badge.modrinth { background: #1bd96a; }
.mod-badge.curseforge { background: #f16436; }
.mod-sub { flex-shrink: 0; margin-left: auto; font-size: 11px; color: var(--text-tertiary); white-space: nowrap; }
.mod-desc {
  font-size: 11px; color: var(--text-tertiary); line-height: 1.4;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  max-width: 100%;
}
.mod-support { display: flex; align-items: center; gap: 5px; min-width: 0; }
.mod-ld {
  flex-shrink: 0; font-size: 10px; font-weight: 600; padding: 0 5px;
  border-radius: 6px; background: var(--color-brand-bg); color: var(--color-brand);
}
.mod-gv {
  font-size: 10px; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
/* 底部栏：分页器居中，结果计数绝对定位最右侧 */
.mod-footer { position: relative; padding: 6px 16px 14px; }
.mod-pager { display: flex; align-items: center; justify-content: center; gap: 12px; }
.mod-result-count {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  font-size: 12px; color: var(--text-tertiary); user-select: none;
}
.mod-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.mod-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.mod-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.mod-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }
</style>
