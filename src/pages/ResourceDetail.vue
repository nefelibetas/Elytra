<script setup lang="ts">
// 资源详情页（Mod / 光影包 / 资源包 / 数据包 / 整合包共用）：
// 信息卡片 + 版本文件列表（过滤展示 + 直接提交文件下载）。
// 仅 Mod 需要 ModLoader 筛选；非 Mod 项目由 backTab 判定后隐藏相关 UI。
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { resourceDetail, openResourceDetail } from "../store/resourceDetail";
import { selectedVersion } from "../store/selectedVersion";
import { dlTranslateMode, dlShowReleaseOnly } from "../store/downloaderConfig";
import {
  fetchVersions, fetchModDetails,
  type ModVersion, type ModDependencyItem, type ModDisplayItem,
} from "../store/modSearch";
import { registerTaskName, type DownloadFileKind } from "../store/download";
import { fmtCount, fmtSize, fmtModDate } from "../utils/format";
import IconImage from "../component/IconImage.vue";

const item = computed(() => resourceDetail.item);

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

/** 仅 Mod 需要 ModLoader 筛选（来源 tab 判定；非 Mod 隐藏加载器下拉）。 */
const isMod = computed(() => resourceDetail.backTab === "mod");
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

// 切换加载器后已选版本可能不在新选项里，自动清空（继承值除外：列表搜索已按该版本过滤）
watch(gameVersionOptions, () => {
  if (
    gameVersionFilter.value &&
    gameVersionFilter.value !== inheritedVersion &&
    !gameVersionOptions.value.includes(gameVersionFilter.value)
  ) {
    gameVersionFilter.value = "";
  }
});

/** 版本类型优先级：正式版置顶（用户逻辑默认下载稳定版），beta/alpha 依次靠后；
 *  同组内按发布时间倒序（最新在前）。无正式版时 beta 组自然置顶（最新可能只有 alpha）。 */
const TYPE_ORDER: Record<string, number> = { release: 0, beta: 1, alpha: 2, unknown: 3 };

const filteredVersions = computed(() =>
  versions.value
    .filter(
      v =>
        (!loaderFilter.value || v.loaders.includes(loaderFilter.value)) &&
        (!gameVersionFilter.value || v.game_versions.some(gv => majorVersion(gv) === gameVersionFilter.value)),
    )
    .sort((a, b) => {
      const t = (TYPE_ORDER[a.version_type] ?? 3) - (TYPE_ORDER[b.version_type] ?? 3);
      if (t !== 0) return t;
      return b.date_published.localeCompare(a.date_published);
    }),
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

/** 当前选中的版本文件 id（点击 mv-item 选中；切换项目/重新加载时重置）。
 *  必须在 loadVersions 前声明：immediate watch 会在 setup 期立即调用它。 */
const selectedVersionId = ref("");

async function loadVersions() {
  const it = item.value;
  if (!it) return;
  loadingVersions.value = true;
  versionError.value = "";
  try {
    const all = await fetchVersions(it.source, it.id);
    // 仅显示正式版：剔除所有支持的版本都是快照/预发布的文件（混版文件保留）
    versions.value = dlShowReleaseOnly.value
      ? all.filter(v => v.game_versions.some(gv => !isSnapshot(gv)))
      : all;
  } catch (e) {
    versionError.value = errMsg(e);
  } finally {
    loadingVersions.value = false;
  }
}

/** 项目切换：应用 store 同步来的版本筛选/选中版本（从上一详情进入/返回还原），再加载版本。
 *  列表页版本选项是完整 ID（如 "1.21.1"），详情页是主版本归类（如 "1.21"），
 *  继承时做 major 归一化，并记录 inheritedVersion——watch(gameVersionOptions) 的清空
 *  逻辑跳过继承值（列表搜索已按该版本过滤，进入详情的 mod 必然支持它，不应被清空）。 */
let inheritedVersion: string | null = null;

function onItemChange() {
  loaderFilter.value = resourceDetail.pendingLoader;
  inheritedVersion = majorVersion(resourceDetail.pendingGameVersion);
  gameVersionFilter.value = inheritedVersion;
  selectedVersionId.value = resourceDetail.pendingSelectedVersion;
  loadVersions();
}

watch(item, onItemChange, { immediate: true });

// ── 前置依赖（选中版本文件的 required 依赖；不同版本依赖可能不同）──────────

const depItems = ref<ModDependencyItem[]>([]);
const loadingDeps = ref(false);
const depError = ref("");

/** 选中版本的 required 依赖项目 ID（去重保序）。 */
const selectedDepIds = computed(() => {
  const v = versions.value.find(x => x.version_id === selectedVersionId.value);
  if (!v) return [];
  const seen = new Set<string>();
  const order: string[] = [];
  for (const dep of v.dependencies) {
    if (dep.relation === "required" && dep.project_id && !seen.has(dep.project_id)) {
      seen.add(dep.project_id);
      order.push(dep.project_id);
    }
  }
  return order;
});

async function loadDeps() {
  const it = item.value;
  const ids = selectedDepIds.value;
  if (!it || ids.length === 0) {
    depItems.value = [];
    depError.value = "";
    return;
  }
  loadingDeps.value = true;
  depError.value = "";
  try {
    depItems.value = await fetchModDetails(it.source, ids);
  } catch (e) {
    depError.value = errMsg(e);
  } finally {
    loadingDeps.value = false;
  }
}

watch(selectedDepIds, loadDeps);

/** 点击版本文件行：选中并刷新前置依赖。 */
function pickVersion(v: ModVersion) {
  selectedVersionId.value = v.version_id;
}

/** 依赖详情 → 可跳转的 ModDisplayItem（详情页字段，无搜索元数据）。 */
function depToItem(d: ModDependencyItem): ModDisplayItem {
  const src = item.value?.source ?? "modrinth";
  return {
    id: d.id,
    title: d.title,
    title_zh: null,
    mcmod_id: null,
    description: d.description,
    translated: d.translated,
    loaders: [],
    game_version_range: null,
    author: "",
    downloads: 0,
    icon_url: d.icon_url,
    page_url: "",
    source: src,
    key: `${src}:${d.id}`,
  };
}

// ── 提交文件下载 ──────────────────────────────

/** 来源 tab → 落盘目录 kind；modpack 无对应落盘方案（开放待研究），不显示下载按钮。 */
const fileKind = computed<DownloadFileKind | null>(() => {
  switch (resourceDetail.backTab) {
    case "mod": return "mod";
    case "shader": return "shader";
    case "resourcepack": return "resourcepack";
    case "datapack": return "datapack";
    case "modpack": return null;
    default: return null;
  }
});

/** 下载目标版本目录（启动页当前选中的版本 id；后端按此解析实际目录名，会话内读取）。 */
const savedVersion = ref(selectedVersion.value);

/** 提交选中版本文件的下载任务（download_single_file → 任务中心自动注册）。
 *  落盘版本隔离：versions/{版本目录}/{kind_dir}/{filename}（后端 resolve 兜底 _{n} 冲突）。 */
async function submitDownload() {
  const v = versions.value.find(x => x.version_id === selectedVersionId.value);
  const kind = fileKind.value;
  if (!v || !kind) return;
  if (!savedVersion.value) {
    showToast("请先在版本选择页选择版本");
    return;
  }
  try {
    const taskId = await invoke<string>("download_single_file", {
      task: {
        version_dir_name: savedVersion.value,
        kind,
        url: v.download_url,
        filename: v.filename,
      },
    });
    // 注册资源名：任务完成提示显示 "下载任务[资源名]完成"（fallback 文件名）
    if (taskId) {
      registerTaskName(taskId, item.value?.title ?? v.filename);
    }
    showToast("已提交下载任务");
  } catch (e) {
    showToast(errMsg(e));
  }
}

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
    <!-- 项目切换动画：进入依赖 mod 详情时内容整体淡入淡出（同路由 push 不触发路由级过渡） -->
    <Transition name="md-switch" mode="out-in">
      <div :key="item.key" class="md-body">
        <!-- 卡片一：项目信息（样式与外部 item 类似；描述行与图标左对齐全宽；按钮在卡片内底部） -->
        <div class="md-card">
          <div class="md-row">
            <IconImage :src="item.icon_url" :size="40" />
            <span class="md-info">
              <span class="md-title-row">
                <span class="md-title">{{ item.title_zh ?? item.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && item.title_zh" class="md-title-en"> | {{ item.title }}</span>
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

        <!-- 双卡片并列：左版本文件，右前置依赖（选中版本后出现） -->
        <div class="md-cards-row">
          <div class="md-card md-versions">
            <div class="mv-head">
              <span class="mv-title">版本文件<span v-if="!loadingVersions" class="mv-count"> | {{ filteredVersions.length }} 个版本</span></span>
              <span class="mv-spacer"></span>

              <button
                v-if="fileKind"
                class="mv-dl"
                :disabled="!selectedVersionId"
                v-tooltip="selectedVersionId ? '下载所选版本' : '先选择一个版本'"
                @click="submitDownload"
              >下载</button>

              <div v-if="isMod" ref="loaderDrop" class="mv-drop" @pointerdown.stop @click.stop="toggleFilter('loader')">
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
              <div
                v-for="v in filteredVersions"
                :key="v.version_id"
                class="mv-item"
                :class="{ active: v.version_id === selectedVersionId }"
                @click="pickVersion(v)"
              >
                <div class="mv-info">
                  <span class="mv-name-row">
                    <span v-for="ld in v.loaders" :key="ld" class="md-ld" :class="`md-ld-${ld}`">{{ ld }}</span>
                    <span class="mv-name" v-tooltip="v.filename">{{ v.filename }}</span>
                    <span v-if="v.version_type === 'alpha'" class="mv-type alpha">alpha</span>
                    <span v-else-if="v.version_type === 'beta'" class="mv-type beta">beta</span>
                  </span>
                  <span class="mv-tags">
                    <span v-for="gv in v.game_versions.slice(0, 3)" :key="gv" class="mv-tag gv">{{ gv }}</span>
                  </span>
                </div>
                <span class="mv-meta">
                  <span class="mv-date">{{ fmtModDate(v.date_published) }}</span>
                  <span class="mv-size">{{ fmtSize(v.file_size) }}</span>
                </span>
              </div>
            </div>
          </div>

          <!-- 右：前置依赖卡片（选中版本且有 required 依赖才出现；宽度展开动画） -->
          <Transition name="mv-side">
            <div v-if="selectedVersionId && selectedDepIds.length > 0" class="md-card md-deps-card">
              <div class="mv-head">
                <span class="mv-title">前置依赖<span v-if="!loadingDeps" class="mv-count"> | {{ depItems.length }} 个</span></span>
              </div>
              <div v-if="loadingDeps" class="md-dep-state">
                <div class="spinner"></div>
                <p class="md-dep-state-text">正在加载前置依赖...</p>
              </div>
              <div v-else-if="depError" class="md-dep-state err">{{ depError }}</div>
              <div v-else-if="depItems.length === 0" class="md-dep-empty">未找到前置依赖信息</div>
              <div v-else class="md-dep-list">
                <div v-for="d in depItems" :key="d.id" class="md-dep-item" @click="openResourceDetail(depToItem(d), { loader: loaderFilter, gameVersion: gameVersionFilter, selectedVersion: selectedVersionId })">
                  <IconImage :src="d.icon_url" :size="28" />
                  <span class="md-dep-info">
                    <span class="md-dep-title">{{ d.title }}</span>
                    <span class="md-dep-desc">{{ d.translated ?? d.description }}</span>
                  </span>
                </div>
              </div>
            </div>
          </Transition>
        </div>
      </div>
    </Transition>
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

/* 项目切换动画：进入/返回依赖详情时整体淡入淡出 */
.md-switch-enter-active,
.md-switch-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.md-switch-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.md-switch-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

/* 单列布局：项目信息 + 双卡片行，版本卡片撑满剩余高度 */
.md-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
}
.md-body > .md-card { flex-shrink: 0; }

/* 双卡片并列：左版本文件（flex:1），右前置依赖（固定 240px，动态出现） */
.md-cards-row {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 12px;
}
.md-versions {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.md-versions .mv-head { flex-shrink: 0; }
.md-versions .mv-list,
.md-versions .mv-state { flex: 1; min-height: 0; }

/* 右侧前置依赖卡片：宽度展开动画（目标值写在 enter-to/leave-from） */
.mv-side-enter-active,
.mv-side-leave-active {
  transition: width 0.22s ease, opacity 0.18s ease;
  overflow: hidden;
}
.mv-side-enter-from,
.mv-side-leave-to {
  width: 0;
  opacity: 0;
}
.mv-side-enter-to,
.mv-side-leave-from {
  width: 240px;
  opacity: 1;
}
.md-deps-card {
  flex: 0 0 240px;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.md-deps-card .mv-head { flex-shrink: 0; }
.md-deps-card .md-dep-list,
.md-deps-card .md-dep-state,
.md-deps-card .md-dep-empty {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  margin-top: 12px;
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

/* 支持的 ModLoader 徽章（与外部 item 同款；放在标题前，专属色随主题变量切换） */
.md-ld {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
}
.md-ld-fabric {
  background: color-mix(in srgb, var(--ld-fabric) 12%, transparent);
  color: var(--ld-fabric);
}
.md-ld-forge {
  background: color-mix(in srgb, var(--ld-forge) 12%, transparent);
  color: var(--ld-forge);
}
.md-ld-neoforge {
  background: color-mix(in srgb, var(--ld-neoforge) 12%, transparent);
  color: var(--ld-neoforge);
}
.md-ld-quilt {
  background: color-mix(in srgb, var(--ld-quilt) 12%, transparent);
  color: var(--ld-quilt);
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
  white-space: nowrap;
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

/* 下载按钮：主操作（品牌填充），与筛选下拉等高；未选中版本时置灰 */
.mv-dl {
  flex-shrink: 0;
  height: 30px;
  padding: 0 16px;
  border: 1px solid var(--color-brand);
  border-radius: var(--radius-sm);
  background: var(--color-brand);
  color: #fff;
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s;
  outline: none;
}
.mv-dl:hover:not(:disabled) {
  filter: brightness(1.12);
}
.mv-dl:active:not(:disabled) {
  filter: brightness(0.95);
}
.mv-dl:disabled {
  background: var(--surface-hover);
  border-color: var(--border-default);
  color: var(--text-tertiary);
  cursor: not-allowed;
}

/* ── 自定义下拉 ── */

.mv-drop {
  position: relative;
  flex: 0 0 140px;
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
  margin: 8px 0 -4px;
}

.mv-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background 0.1s, border-color 0.1s;
}
.mv-item:hover { background-color: var(--surface-hover); }
.mv-item.active {
  background: var(--color-brand-bg);
  border: 1px solid var(--color-brand);
  padding: 7px 9px;
  box-shadow: inset 0 0 0 1px var(--color-brand);
}

.mv-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.mv-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.mv-name {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 版本类型标记（非正式版）：alpha 红 / beta 橙，随主题语义色 */
.mv-type {
  flex: none;
  padding: 0 6px;
  border-radius: 8px;
  font-size: 10px;
  font-weight: 700;
  line-height: 1.6;
  text-transform: uppercase;
}
.mv-type.alpha { background: var(--color-danger-bg); color: var(--color-danger); }
.mv-type.beta { background: rgba(217, 119, 6, 0.12); color: var(--color-warning); }

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

.mv-meta {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
}
.mv-date { font-size: 10.5px; color: var(--text-tertiary); }
.mv-size { font-size: 11px; color: var(--text-secondary); }

/* ── 前置依赖（右侧面板，选中版本后出现）── */
.md-dep-empty,
.md-dep-state {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 14px 10px;
  text-align: center;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-tertiary);
  background: var(--surface-page);
  border-radius: var(--radius-sm);
}
.md-dep-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
}
.md-dep-state.err {
  color: var(--color-danger, #e5484d);
  font-size: 12px;
}
.md-dep-state-text {
  font-size: 12px;
  color: var(--text-tertiary);
}
.md-dep-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.md-dep-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.md-dep-item:hover {
  border-color: var(--color-brand);
  background: var(--surface-hover);
}
.md-dep-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.md-dep-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.md-dep-desc {
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

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
