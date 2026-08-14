<script setup lang="ts">
// 项目详情页（Mod / 光影包 / 资源包 / 数据包 / 整合包共用）：
// 信息卡片 + 版本文件列表（过滤展示，下载走来源页）。
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { showToast } from "../utils/toast";
import { modDetail } from "../store/modDetail";
import { dlTranslateMode, dlShowReleaseOnly } from "../store/downloaderConfig";
import { fetchVersions, type ModVersion } from "../store/modSearch";
import { fmtCount, fmtSize, fmtModDate } from "../utils/format";
import IconImage from "../component/IconImage.vue";

const item = computed(() => modDetail.item);

const sourceLabel = computed(() =>
  item.value?.source === "modrinth" ? "Modrinth" : "CurseForge",
);

// ── 版本列表 ──────────────────────────────

const versions = ref<ModVersion[]>([]);
const loadingVersions = ref(false);
const versionError = ref("");

// ── 版本列表过滤（本地过滤，选项从已加载版本收集，无需额外请求）──

const loaderFilter = ref("");
const gameVersionFilter = ref("");

/** 加载器选项固定优先级（其余按出现顺序兜底） */
const LOADER_ORDER = ["fabric", "forge", "neoforge", "quilt"];
const loaderOptions = computed(() =>
  Array.from(new Set(versions.value.flatMap(v => v.loaders))).sort(
    (a, b) => (LOADER_ORDER.indexOf(a) + 1 || 99) - (LOADER_ORDER.indexOf(b) + 1 || 99),
  ),
);

/** 主版本归类：提取开头 major.minor（"1.20.4"/"1.20.5-pre1"/"26.3-snapshot-8" → "1.20"/"26.3"）；快照等无数字点分前缀原样保留 */
function majorVersion(gv: string): string {
  const m = gv.match(/^\d+\.\d+/);
  return m ? m[0] : gv;
}

/** 当前加载器过滤下的版本文件（版本选项的数据源：只统计该 ModLoader 可用的版本） */
const loaderScopedVersions = computed(() =>
  loaderFilter.value
    ? versions.value.filter(v => v.loaders.includes(loaderFilter.value))
    : versions.value,
);

/** 游戏版本选项：基于当前 ModLoader 统计（按主版本归类），数值化降序（最新在前） */
const gameVersionOptions = computed(() =>
  Array.from(new Set(loaderScopedVersions.value.flatMap(v => v.game_versions).map(majorVersion))).sort((a, b) =>
    new Intl.Collator(undefined, { numeric: true }).compare(b, a),
  ),
);

// 切换加载器后已选版本可能不在新选项里，自动清空
watch(gameVersionOptions, () => {
  if (gameVersionFilter.value && !gameVersionOptions.value.includes(gameVersionFilter.value)) {
    gameVersionFilter.value = "";
  }
});

const filteredVersions = computed(() =>
  versions.value.filter(
    v =>
      (!loaderFilter.value || v.loaders.includes(loaderFilter.value)) &&
      (!gameVersionFilter.value || v.game_versions.some(gv => majorVersion(gv) === gameVersionFilter.value)),
  ),
);

function loaderLabel(l: string): string {
  return l.charAt(0).toUpperCase() + l.slice(1);
}

// ── 自定义下拉（原生 select 弹出层高度在 WebView2 无法限制，选项多时过高）──

const openFilter = ref<"" | "loader" | "game">("");

const POP_MAX_H = 260;
/** 弹层最大高度：按下方可视空间动态计算（始终向下展开且不扩展外层滚动容器的 scrollHeight） */
const popMaxH = ref(POP_MAX_H);

const loaderDrop = ref<HTMLElement | null>(null);
const gameDrop = ref<HTMLElement | null>(null);

function toggleFilter(kind: "" | "loader" | "game") {
  if (openFilter.value === kind) {
    openFilter.value = "";
    return;
  }
  const drop = kind === "loader" ? loaderDrop.value : gameDrop.value;
  if (drop) {
    const rect = drop.getBoundingClientRect();
    popMaxH.value = Math.max(80, Math.min(POP_MAX_H, window.innerHeight - rect.bottom - 8));
  }
  openFilter.value = kind;
}

function pickLoader(l: string) {
  loaderFilter.value = l;
  openFilter.value = "";
}

function pickGame(gv: string) {
  gameVersionFilter.value = gv;
  openFilter.value = "";
}

function onDocPointerDown() {
  openFilter.value = "";
}
onMounted(() => document.addEventListener("pointerdown", onDocPointerDown));
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDocPointerDown));

/** 快照/预发布/候选等非正式版本标记（周快照 26w14a、新式 26.3-snapshot-8、-pre1、-rc1、candidate） */
const SNAPSHOT_RE = /\d{2}w\d{2}|snapshot|pre\d*|rc\d*|candidate/i;
const isSnapshot = (gv: string): boolean => SNAPSHOT_RE.test(gv);

async function loadVersions() {
  const it = item.value;
  if (!it) return;
  loadingVersions.value = true;
  versionError.value = "";
  loaderFilter.value = "";
  gameVersionFilter.value = "";
  try {
    const all = await fetchVersions(it.source, it.id);
    // 仅显示正式版：剔除所有支持的版本都是快照/预发布的文件（混版文件保留）
    versions.value = dlShowReleaseOnly.value
      ? all.filter(v => v.game_versions.some(gv => !isSnapshot(gv)))
      : all;
  } catch (e) {
    versionError.value = String(e);
  } finally {
    loadingVersions.value = false;
  }
}

watch(item, loadVersions, { immediate: true });

// ── 操作 ──────────────────────────────

/** 来源页面链接（item.page_url）。 */
function gotoSource() {
  const it = item.value;
  if (!it?.page_url) return;
  openUrl(it.page_url).catch(() => showToast("打开链接失败"));
}

/** MC百科：有内置条目 id → 直达条目页；未收录 → 名称搜索页降级。 */
function gotoMcmod() {
  const it = item.value;
  if (!it) return;
  const url = it.mcmod_id
    ? `https://www.mcmod.cn/class/${it.mcmod_id}.html`
    : `https://search.mcmod.cn/s?key=${encodeURIComponent(it.title_zh ?? it.title)}&mold=0`;
  openUrl(url).catch(() => showToast("打开链接失败"));
}

/** 复制名称：有译名时复制 "翻译名 | 原名"（与标题显示样式一致），否则原名。 */
async function copyName() {
  const it = item.value;
  if (!it) return;
  const text = it.title_zh ? `${it.title_zh} | ${it.title}` : it.title;
  try {
    await navigator.clipboard.writeText(text);
    showToast("已复制名称");
  } catch {
    showToast("复制失败");
  }
}
</script>

<template>
  <div v-if="item" class="md">
    <!-- 卡片一：项目信息（样式与外部 item 类似；描述行与图标左对齐全宽；按钮在卡片内底部） -->
    <div class="md-card">
      <div class="md-row">
        <IconImage :src="item.icon_url" :size="40" />
        <span class="md-info">
          <span class="md-title-row">
            <span class="md-title">{{ item.title_zh ?? item.title }}</span>
            <span v-if="dlTranslateMode === 'zh' && item.title_zh" class="md-title-en"> | {{ item.title }}</span>
            <span v-for="ld in item.loaders" :key="ld" class="md-ld">{{ ld }}</span>
            <span class="md-badge" :class="item.source">{{ sourceLabel }}</span>
            <span class="md-sub">{{ fmtCount(item.downloads) }} 次下载</span>
          </span>
        </span>
      </div>
      <div class="md-desc">{{ item.translated ?? item.description }}</div>

      <!-- 操作按钮（卡片内底部） -->
      <div class="md-actions">
        <button class="md-btn" @click="gotoSource">转到{{ sourceLabel }}</button>
        <button class="md-btn" @click="gotoMcmod">转到 MC百科</button>
        <button class="md-btn" @click="copyName">复制名称</button>
      </div>
    </div>

    <!-- 卡片二：版本文件列表 -->
    <div class="md-card md-versions">
      <div class="mv-head">
        <span class="mv-title">版本文件<span v-if="!loadingVersions" class="mv-count"> | {{ filteredVersions.length }} 个版本</span></span>
        <span class="mv-spacer"></span>

        <div ref="loaderDrop" class="mv-drop" @pointerdown.stop @click.stop="toggleFilter('loader')">
          <span class="mv-drop-label">{{ loaderFilter ? loaderLabel(loaderFilter) : "全部加载器" }}</span>
          <span class="mv-drop-arrow" :class="{ open: openFilter === 'loader' }"></span>
          <Transition name="expand">
            <div v-if="openFilter === 'loader'" class="mv-drop-pop" :style="{ '--pop-h': popMaxH + 'px' }">
              <div class="mv-drop-opt" :class="{ sel: !loaderFilter }" @click.stop="pickLoader('')">全部加载器</div>
              <div v-for="l in loaderOptions" :key="l" class="mv-drop-opt" :class="{ sel: loaderFilter === l }" @click.stop="pickLoader(l)">{{ loaderLabel(l) }}</div>
            </div>
          </Transition>
        </div>

        <div ref="gameDrop" class="mv-drop" @pointerdown.stop @click.stop="toggleFilter('game')">
          <span class="mv-drop-label">{{ gameVersionFilter || "全部版本" }}</span>
          <span class="mv-drop-arrow" :class="{ open: openFilter === 'game' }"></span>
          <Transition name="expand">
            <div v-if="openFilter === 'game'" class="mv-drop-pop" :style="{ '--pop-h': popMaxH + 'px' }">
              <div class="mv-drop-opt" :class="{ sel: !gameVersionFilter }" @click.stop="pickGame('')">全部版本</div>
              <div v-for="gv in gameVersionOptions" :key="gv" class="mv-drop-opt" :class="{ sel: gameVersionFilter === gv }" @click.stop="pickGame(gv)">{{ gv }}</div>
            </div>
          </Transition>
        </div>
      </div>

      <div v-if="loadingVersions" class="mv-state">
        <div class="spinner"></div>
        <p class="mv-state-text">正在加载版本列表...</p>
      </div>
      <div v-else-if="versionError" class="mv-state err">
        <p>{{ versionError }}</p>
        <button class="md-btn mv-retry" @click="loadVersions">重试</button>
      </div>
      <div v-else-if="versions.length === 0" class="mv-state">暂无可用版本</div>
      <div v-else class="mv-list">
        <div v-if="filteredVersions.length === 0" class="mv-state">没有匹配的版本，试试调整过滤条件</div>
        <div v-for="v in filteredVersions" :key="v.version_id" class="mv-item">
          <div class="mv-info">
            <span class="mv-name" v-tooltip="v.filename">{{ v.filename }}</span>
            <span class="mv-tags">
              <span v-for="gv in v.game_versions.slice(0, 3)" :key="gv" class="mv-tag gv">{{ gv }}</span>
              <span v-for="ld in v.loaders" :key="ld" class="mv-tag ld">{{ ld }}</span>
            </span>
          </div>
          <span class="mv-meta">
            <span class="mv-date">{{ fmtModDate(v.date_published) }}</span>
            <span class="mv-size">{{ fmtSize(v.file_size) }}</span>
          </span>
        </div>
      </div>
    </div>
  </div>
  <div v-else class="md-empty">未找到资源信息</div>
</template>

<style scoped>
.md {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface-page);
  scrollbar-gutter: stable;
}

.md-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 14px 16px;
}

.md-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.md-info {
  flex: 1;
  min-width: 0;
}

.md-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.md-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.md-title-en {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 支持的 ModLoader 徽章（与外部 item 同款） */
.md-ld {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

.md-badge {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  color: #fff;
  font-weight: 600;
}
.md-badge.modrinth { background: #1bd96a; }
.md-badge.curseforge { background: #f16436; }

.md-sub {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

/* 描述：另起一行，左边缘与图标左对齐（全宽） */
.md-desc {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle, var(--border-default));
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
}

/* 操作按钮：卡片内底部（描述下方，分隔线上） */
.md-actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle, var(--border-default));
}

.md-btn {
  flex: 1;
  padding: 8px 0;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  color: var(--text-secondary);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.md-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

/* ── 版本列表 ── */

.md-versions {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 12px 16px;
}

.mv-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-subtle, var(--border-default));
}

.mv-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

/* 数量统计：与标题样式区分（弱化、更小、常规字重） */
.mv-count {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
}

.mv-spacer {
  flex: 1;
}

/* ── 自定义下拉 ── */

.mv-drop {
  position: relative;
  flex: 0 0 170px;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  cursor: pointer;
  user-select: none;
  transition: border-color 0.15s;
}
.mv-drop:hover,
.mv-drop.open {
  border-color: var(--color-brand);
}

.mv-drop-label {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mv-drop-arrow {
  flex-shrink: 0;
  width: 0;
  height: 0;
  border-left: 4px solid transparent;
  border-right: 4px solid transparent;
  border-top: 5px solid var(--text-tertiary);
  transition: transform 0.15s;
}
.mv-drop-arrow.open {
  transform: rotate(180deg);
}

.mv-drop-pop {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 20;
  max-height: var(--pop-h, 260px);
  overflow-y: auto;
  background: var(--surface-card);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 0, 0, 0.25));
  padding: 4px;
}

.mv-drop-opt {
  padding: 6px 10px;
  border-radius: var(--radius-xs, 4px);
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition: background 0.1s, color 0.1s;
}
.mv-drop-opt:hover {
  background: var(--surface-hover);
  color: var(--color-brand);
}
.mv-drop-opt.sel {
  color: var(--color-brand);
  font-weight: 600;
}

/* expand 动画：目标值必须写在 enter-to/leave-from（写在 active 上会因同特异性覆盖 from 态导致无动画） */
.expand-enter-active,
.expand-leave-active {
  transition: max-height 0.2s ease, opacity 0.15s ease;
  overflow: hidden;
}
.expand-enter-from,
.expand-leave-to {
  max-height: 0;
  opacity: 0;
}
.expand-enter-to,
.expand-leave-from {
  max-height: var(--pop-h, 260px);
  opacity: 1;
}

.mv-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 30px 0;
  font-size: 12.5px;
  color: var(--text-tertiary);
}
.mv-state.err { color: var(--color-danger); }
.mv-state-text { font-size: 12.5px; color: var(--text-tertiary); }
.mv-retry { flex: 0 0 auto; padding: 5px 20px; font-size: 12px; }

.spinner {
  width: 32px; height: 32px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.mv-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  margin: 8px -8px -4px;
}

.mv-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  transition: background 0.1s;
}
.mv-item:hover { background-color: var(--surface-hover); }

.mv-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.mv-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mv-tags {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  flex-wrap: wrap;
}

.mv-tag {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
}
.mv-tag.gv { background: var(--surface-hover); color: var(--text-secondary); }
.mv-tag.ld { background: var(--color-brand-bg); color: var(--color-brand); }

.mv-meta {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
}
.mv-date { font-size: 10.5px; color: var(--text-tertiary); }
.mv-size { font-size: 11px; color: var(--text-secondary); }

.md-empty {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  color: var(--text-tertiary);
  background: var(--surface-page);
}
</style>
