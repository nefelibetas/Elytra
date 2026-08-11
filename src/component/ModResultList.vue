<script setup lang="ts">
// Mod 搜索结果列表：加载/错误/空态 + 结果卡片（点击展开版本）+ 分页器
import { modSearch, modTotalPages } from "../store/modSearch";
import { fmtCount, fmtSize, fmtModDate } from "../utils/format";
</script>

<template>
  <div class="mod-result-card">
    <div v-if="modSearch.error" class="mod-state err">{{ modSearch.error }}</div>
    <div v-else-if="modSearch.searching" class="mod-loading">
      <div class="spinner"></div>
      <p class="mod-loading-text">{{ modSearch.query.trim() ? "正在搜索 Mod..." : "正在加载热门 Mod..." }}</p>
    </div>
    <div v-else-if="modSearch.searched && modSearch.results.length === 0" class="mod-state">未找到相关 Mod，试试更换关键字或放宽筛选条件</div>
    <template v-else-if="modSearch.searched">
      <div class="mod-result-count">共 {{ modSearch.total }} 个结果</div>
      <div class="mod-list">
        <div v-for="m in modSearch.results" :key="m.key" class="mod-card">
          <button class="mod-card-hdr" :title="m.description" @click="modSearch.toggleVersions(m)">
            <img v-if="m.icon_url" :src="m.icon_url" width="32" height="32" class="mod-icon" alt="" loading="lazy" />
            <span v-else class="mod-icon mod-icon-ph"></span>
            <span class="mod-info">
              <span class="mod-title-row">
                <span class="mod-title">{{ m.title }}</span>
                <span v-if="modSearch.source === 'all'" class="mod-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
              </span>
              <span class="mod-sub">{{ m.author }} · {{ fmtCount(m.downloads) }} 次下载</span>
            </span>
            <svg class="mod-arrow" :class="{ open: modSearch.expanded[m.key] }" viewBox="0 0 12 12" width="12" height="12" fill="currentColor"><polygon points="1,3 11,3 6,10" /></svg>
          </button>
          <Transition name="expand">
            <div v-if="modSearch.expanded[m.key]" class="mod-versions">
              <div v-if="modSearch.versionsLoading[m.key]" class="mod-versions-hint">正在加载版本...</div>
              <div v-else-if="modSearch.versions[m.key] && modSearch.versions[m.key].length">
                <div v-for="v in modSearch.versions[m.key]" :key="v.version_id" class="mod-ver">
                  <span class="mod-ver-name">{{ v.version_number }}</span>
                  <span class="mod-ver-meta">
                    <template v-if="v.loaders.length">{{ v.loaders.join(", ") }} · </template>{{ v.game_versions.slice(0, 3).join(", ") }}<template v-if="v.game_versions.length > 3"> 等</template><template v-if="v.file_size"> · {{ fmtSize(v.file_size) }}</template>
                  </span>
                  <span class="mod-ver-date">{{ fmtModDate(v.date_published) }}</span>
                </div>
              </div>
              <div v-else class="mod-versions-hint">没有符合筛选条件的版本</div>
            </div>
          </Transition>
        </div>
      </div>
      <div v-if="modTotalPages > 1" class="mod-pager">
        <button class="mod-pager-btn" :disabled="modSearch.page <= 1" @click="modSearch.search(modSearch.page - 1)">上一页</button>
        <span class="mod-pager-info">{{ modSearch.page }} / {{ modTotalPages }}</span>
        <button class="mod-pager-btn" :disabled="modSearch.page >= modTotalPages" @click="modSearch.search(modSearch.page + 1)">下一页</button>
      </div>
    </template>
    <div v-else class="mod-state">输入关键字搜索，或直接搜索查看热门 Mod</div>
  </div>
</template>

<style scoped>
.mod-result-card { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.mod-state { text-align: center; color: var(--text-tertiary); font-size: 13px; padding: 40px 0; }
.mod-state.err { color: #e74c3c; }
.mod-loading { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 40px 0; }
.mod-loading-text { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.mod-result-count { font-size: 12px; color: var(--text-tertiary); padding: 10px 16px 0; }
.mod-list { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px 12px; }
.mod-card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); overflow: hidden; }
.mod-card-hdr {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; border: none; background: transparent;
  color: var(--text-primary); font-size: 13px; cursor: pointer; outline: none;
  transition: background 0.1s; user-select: none;
}
.mod-card-hdr:hover { background-color: var(--surface-hover); }
.mod-icon { flex-shrink: 0; border-radius: 5px; width: 32px; height: 32px; object-fit: cover; }
.mod-icon-ph { background: var(--surface-hover); }
.mod-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.mod-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.mod-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.mod-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: #fff; flex-shrink: 0; font-weight: 600; }
.mod-badge.modrinth { background: #1bd96a; }
.mod-badge.curseforge { background: #f16436; }
.mod-sub { font-size: 11px; color: var(--text-tertiary); }
.mod-arrow { flex-shrink: 0; color: var(--text-tertiary); transition: transform 0.2s; margin-right: 2px; }
.mod-arrow.open { transform: rotate(180deg); }
.mod-versions {
  border-top: 1px solid var(--border-default);
  background: var(--surface-page);
  max-height: 320px;
  overflow-y: auto;
  overflow-x: hidden;
}
.mod-versions-hint { padding: 12px 16px; font-size: 12px; color: var(--text-tertiary); text-align: center; }
.mod-ver {
  display: flex; align-items: center; gap: 10px;
  padding: 9px 16px 9px 60px; font-size: 12px; color: var(--text-primary);
  border-bottom: 1px solid var(--border-subtle, var(--border-default));
  transition: background 0.1s; user-select: none;
}
.mod-ver:last-child { border-bottom: none; }
.mod-ver:hover { background-color: var(--surface-hover); }
.mod-ver-name { font-weight: 500; flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mod-ver-meta { font-size: 11px; color: var(--text-tertiary); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mod-ver-date { font-size: 11px; color: var(--text-tertiary); flex-shrink: 0; }
.mod-pager { display: flex; align-items: center; justify-content: center; gap: 12px; padding: 4px 0 14px; }
.mod-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.mod-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.mod-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.mod-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }

/* expand：max-height 目标值必须写在 enter-to/leave-from（写在 active 上会因同特异性覆盖 from 态导致无动画），
   且目标值与展开区自身 max-height 一致（320px），内容超高时内部滚动不撑出窗口 */
.expand-enter-active, .expand-leave-active { transition: max-height 0.3s ease, opacity 0.25s ease; }
.expand-enter-from, .expand-leave-to { max-height: 0; opacity: 0; }
.expand-enter-to, .expand-leave-from { max-height: 320px; opacity: 1; }
</style>
