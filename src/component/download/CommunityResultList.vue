<script setup lang="ts">
// 社区资源搜索结果列表（通用版）：六套 ResultList 收敛于此。
// 加载/错误/空态 + 结果行 + 分页器（底部栏计数最右）；差异全部 props 化：
// 搜索状态实例、总页数、类别文案、空态图标、详情回填加载器。
import type { Component } from "vue";
import type { CommunityDisplayItem } from "../../store/community/communitySearch";
import { dlTranslateMode } from "../../store/config/downloaderConfig";
import { openResourceDetail } from "../../store/community/resourceDetail";
import { fmtCount } from "../../utils/format";
import IconImage from "../common/IconImage.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";

/** 搜索状态实例的最小结构（communitySearch 与 createCategorySearch 单例均满足）。 */
export interface CommunityResultState {
  source: "all" | "modrinth" | "curseforge";
  query: string;
  gameVersion: string;
  searching: boolean;
  error: string;
  searched: boolean;
  results: CommunityDisplayItem[];
  total: number;
  page: number;
  search(page?: number): Promise<void> | void;
}

const props = withDefaults(
  defineProps<{
    /** 搜索状态实例（reactive 单例，只读展示 + 翻页调用 search） */
    search: CommunityResultState;
    /** 总页数（computed 传入） */
    totalPages: number;
    /** 类别名词（加载/空态文案复用），如「光影包」 */
    label: string;
    /** 空态图标组件 */
    emptyIcon: Component;
    /** 打开详情时回填的加载器（Mod 传 search.loader 保持响应式，其余缺省空串） */
    detailLoader?: string;
  }>(),
  {
    detailLoader: "",
  },
);
</script>

<template>
  <div class="cs-result-card">
    <ErrorState v-if="search.error" :text="search.error" />
    <LoadingSpinner
      v-else-if="search.searching"
      :text="search.query.trim() ? `正在搜索${label}...` : `正在加载热门${label}...`"
    />
    <EmptyState v-else-if="search.searched && search.results.length === 0">
      <component :is="emptyIcon" :size="40" :stroke-width="1.5" opacity="0.5" />
      <p>未找到相关{{ label }}，试试更换关键字或放宽筛选条件</p>
    </EmptyState>
    <template v-else-if="search.searched">
      <div class="cs-list">
        <div v-for="m in search.results" :key="m.key" class="cs-card">
          <div
            class="cs-card-body"
            @click="openResourceDetail(m, { loader: detailLoader, gameVersion: search.gameVersion, selectedVersion: '' })"
          >
            <IconImage :src="m.icon_url" :size="32" />
            <span class="cs-info">
              <span class="cs-title-row">
                <span class="cs-title">{{ m.title_zh ?? m.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && m.title_zh" class="cs-title-en"> | {{ m.title }}</span>
                <span v-if="search.source === 'all'" class="cs-badge" :class="m.source">{{ m.source === "modrinth" ? "Modrinth" : "CurseForge" }}</span>
                <span class="cs-sub">{{ fmtCount(m.downloads) }} 次下载</span>
              </span>
              <span v-if="dlTranslateMode !== 'en'" class="cs-desc">{{ m.translated ?? m.description }}</span>
              <span v-if="m.loaders.length || m.game_version_range" class="cs-support">
                <span v-for="ld in m.loaders" :key="ld" class="cs-ld" :class="ld">{{ ld }}</span>
                <span v-if="m.game_version_range" class="cs-gv">{{ m.game_version_range }}</span>
              </span>
            </span>
          </div>
        </div>
      </div>
      <!-- 底部栏：分页器居中，结果计数固定最右侧 -->
      <div class="cs-footer">
        <div v-if="totalPages > 1" class="cs-pager">
          <button class="cs-pager-btn" :disabled="search.page <= 1" @click="search.search(search.page - 1)">上一页</button>
          <span class="cs-pager-info">{{ search.page }} / {{ totalPages }}</span>
          <button class="cs-pager-btn" :disabled="search.page >= totalPages" @click="search.search(search.page + 1)">下一页</button>
        </div>
        <!-- 聚合（all）模式的 total 是去重后的已知下限而非精确总数，用「至少」避免误导；单源为精确值用「共」 -->
        <span class="cs-result-count">{{ search.source === "all" ? "至少" : "共" }} {{ search.total }} 个结果</span>
      </div>
    </template>
    <EmptyState v-else>
      <component :is="emptyIcon" :size="40" :stroke-width="1.5" opacity="0.5" />
      <p>输入关键字搜索，或直接搜索查看热门{{ label }}</p>
    </EmptyState>
  </div>
</template>

<style scoped>
.cs-result-card {
  flex: 1; min-height: 0; display: flex; flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.cs-list { display: flex; flex-direction: column; padding: 6px 8px; }
.cs-card { overflow: hidden; }
.cs-card + .cs-card { border-top: 1px solid var(--border-subtle, var(--border-default)); }
.cs-card-body {
  width: 100%; display: flex; align-items: center; gap: 10px;
  padding: 8px 14px; background: transparent;
  color: var(--text-primary); font-size: 13px; user-select: none;
  transition: background 0.1s;
}
.cs-card-body:hover { background-color: var(--surface-hover); }
.cs-card-body { cursor: pointer; }
.cs-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; }
.cs-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.cs-title { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.cs-title-en {
  font-size: 11px; font-weight: 400; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.cs-badge { font-size: 10px; padding: 1px 6px; border-radius: 8px; color: var(--text-inverse); flex-shrink: 0; font-weight: 600; }
.cs-badge.modrinth { background: var(--color-modrinth); }
.cs-badge.curseforge { background: var(--color-curseforge); }
.cs-sub { flex-shrink: 0; margin-left: auto; font-size: 11px; color: var(--text-tertiary); white-space: nowrap; }
.cs-desc {
  font-size: 11px; color: var(--text-tertiary); line-height: 1.4;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  max-width: 100%;
}
.cs-support { display: flex; align-items: center; gap: 5px; min-width: 0; }
.cs-ld {
  flex-shrink: 0; font-size: 10px; font-weight: 600; padding: 0 5px;
  border-radius: 6px; background: var(--color-brand-bg); color: var(--color-brand);
}
/* 按加载器区分配色：与详情页 .md-ld-* 共用同一套 --ld-* token；
   未匹配到的加载器（未来新增/第三方）保留品牌色兜底。 */
.cs-ld.fabric { color: var(--ld-fabric); }
.cs-ld.forge { color: var(--ld-forge); }
.cs-ld.neoforge { color: var(--ld-neoforge); }
.cs-ld.quilt { color: var(--ld-quilt); }
.cs-ld.optifine { color: var(--ld-optifine); }
/* 徽章底色：color-mix 需 WebKitGTK ≥ 2.42（Ubuntu 22.04 的 2.40 不支持）。
 * 不支持的平台降级为纯文字色徽章，避免整条声明失效丢色（与详情页同款降级）。 */
@supports (color-mix(in srgb, red 12%, transparent)) {
  .cs-ld.fabric { background: color-mix(in srgb, var(--ld-fabric) 12%, transparent); }
  .cs-ld.forge { background: color-mix(in srgb, var(--ld-forge) 12%, transparent); }
  .cs-ld.neoforge { background: color-mix(in srgb, var(--ld-neoforge) 12%, transparent); }
  .cs-ld.quilt { background: color-mix(in srgb, var(--ld-quilt) 12%, transparent); }
  .cs-ld.optifine { background: color-mix(in srgb, var(--ld-optifine) 12%, transparent); }
}
.cs-gv {
  font-size: 10px; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

/* 底部栏：分页器居中，结果计数固定最右侧。
 *  min-height 不能省：只有计数没有分页器时（结果不足一页）footer 会塌陷到纯 padding，
 *  绝对定位的计数就会紧贴最后一个条目，看起来像列表项而不是卡片右下角。 */
.cs-footer {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 46px;
  padding: 6px 16px 14px;
}
.cs-pager { display: flex; align-items: center; justify-content: center; gap: 12px; }
.cs-result-count {
  position: absolute; right: 16px; top: 50%; transform: translateY(-50%);
  font-size: 12px; color: var(--text-tertiary); user-select: none;
}
.cs-pager-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-card); color: var(--text-primary); font-size: 12px;
  cursor: pointer; transition: all 0.15s;
}
.cs-pager-btn:hover:not(:disabled) { border-color: var(--color-brand); }
.cs-pager-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.cs-pager-info { font-size: 12px; color: var(--text-tertiary); user-select: none; }
</style>
