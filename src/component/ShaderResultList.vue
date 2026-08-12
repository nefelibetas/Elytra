<script setup lang="ts">
// 光影包搜索结果列表：加载/错误/空态 + 结果行 + 分页器（底部栏计数最右）
import { shaderSearch, shaderTotalPages } from "../store/categorySearch";
import type { ModDisplayItem } from "../store/modSearch";
import { dlTranslateMode } from "../store/downloaderConfig";
import { openModDetail } from "../store/modDetail";
import { fmtCount } from "../utils/format";
import IconImage from "./IconImage.vue";

/** 结果卡片悬浮提示：zh = 中文简介；en = 英文简介 */
function cardTitle(m: ModDisplayItem): string {
  return dlTranslateMode.value === "zh" ? (m.translated ?? m.description) : m.description;
}
</script>

<template>
  <div class="sh-result-card">
    <div v-if="shaderSearch.error" class="sh-state err">{{ shaderSearch.error }}</div>
    <div v-else-if="shaderSearch.searching" class="sh-loading">
      <div class="spinner"></div>
      <p class="sh-loading-text">{{ shaderSearch.query.trim() ? "正在搜索光影包..." : "正在加载热门光影包..." }}</p>
    </div>
    <div v-else-if="shaderSearch.searched && shaderSearch.results.length === 0" class="sh-state">未找到相关光影包，试试更换关键字或放宽筛选条件</div>
    <template v-else-if="shaderSearch.searched">
      <div class="sh-list">
        <div v-for="m in shaderSearch.results" :key="m.key" class="sh-card">
          <div class="sh-card-body" :title="cardTitle(m)" @click="openModDetail(m)">
            <IconImage :src="m.icon_url" :size="32" />
            <span class="sh-info">
              <span class="sh-title-row">
                <span class="sh-title">{{ m.title_zh ?? m.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && m.title_zh" class="sh-title-en"> | {{ m.title }}</span>
                <span v-if="shaderSearch.source === 'all'" class="sh-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
                <span class="sh-sub">{{ fmtCount(m.downloads) }} 次下载</span>
              </span>
              <span v-if="dlTranslateMode !== 'en'" class="sh-desc">{{ m.translated ?? m.description }}</span>
              <span v-if="m.loaders.length || m.game_version_range" class="sh-support">
                <span v-for="ld in m.loaders" :key="ld" class="sh-ld">{{ ld }}</span>
                <span v-if="m.game_version_range" class="sh-gv">{{ m.game_version_range }}</span>
              </span>
            </span>
          </div>
        </div>
      </div>
      <!-- 底部栏：分页器居中，结果计数固定最右侧 -->
      <div class="sh-footer">
        <div v-if="shaderTotalPages > 1" class="sh-pager">
          <button class="sh-pager-btn" :disabled="shaderSearch.page <= 1" @click="shaderSearch.search(shaderSearch.page - 1)">上一页</button>
          <span class="sh-pager-info">{{ shaderSearch.page }} / {{ shaderTotalPages }}</span>
          <button class="sh-pager-btn" :disabled="shaderSearch.page >= shaderTotalPages" @click="shaderSearch.search(shaderSearch.page + 1)">下一页</button>
        </div>
        <span class="sh-result-count">共 {{ shaderSearch.total }} 个结果</span>
      </div>
    </template>
    <div v-else class="sh-state">输入关键字搜索，或直接搜索查看热门光影包</div>
  </div>
</template>

<style scoped>
.sh-result-card {
  flex: 1; min-height: 0; display: flex; flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.sh-state { text-align: center; color: var(--text-tertiary); font-size: 13px; padding: 40px 0; }
.sh-state.err { color: #e74c3c; }
.sh-loading { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 40px 0; }
.sh-loading-text { font-size: 13px; color: var(--text-tertiary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.sh-list { display: flex; flex-direction: column; padding: 6px 8px; }
.sh-card { overflow: hidden; }
.sh-card + .sh-card { border-top: 1px solid var(--border-subtle, var(--border-default)); }
.sh-card-body {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; background: transparent;
  color: var(--text-primary); font-size: 13px; user-select: none;
  transition: background 0.1s;
}
.sh-card-body:hover { background-color: var(--surface-hover); }
.sh-card-body { cursor: pointer; }
.sh-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.sh-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.sh-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.sh-title-en {
  font-size: 11px; font-weight: 400; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.sh-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: #fff; flex-shrink: 0; font-weight: 600; }
.sh-badge.modrinth { background: #1bd96a; }
.sh-badge.curseforge { background: #f16436; }
.sh-sub { flex-shrink: 0; margin-left: auto; font-size: 11px; color: var(--text-tertiary); white-space: nowrap; }
.sh-desc {
  font-size: 11px; color: var(--text-tertiary); line-height: 1.4;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  max-width: 100%;
}
.sh-support { display: flex; align-items: center; gap: 5px; min-width: 0; }
.sh-ld {
  flex-shrink: 0; font-size: 10px; font-weight: 600; padding: 0 5px;
  border-radius: 6px; background: var(--color-brand-bg); color: var(--color-brand);
}
.sh-gv {
  font-size: 10px; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

/* 底部栏：分页器居中，结果计数绝对定位最右侧 */
.sh-footer { position: relative; padding: 6px 16px 14px; }
.sh-pager { display: flex; align-items: center; justify-content: center; gap: 12px; }
.sh-result-count {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  font-size: 12px; color: var(--text-tertiary); user-select: none;
}
.sh-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.sh-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.sh-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.sh-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }
</style>
