<script setup lang="ts">
// 资源详情页（Mod/光影/资源包/数据包/整合包共用）：信息卡片 + 版本文件列表（本地过滤）；仅 Mod 显示 ModLoader 筛选。
import { ref, computed, watch, onMounted, onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { resourceDetail, openResourceDetail } from "../store/community/resourceDetail";
import { goBack } from "../store/app/nav";
import { selectedVersion } from "../store/app/selectedVersion";
import { dlTranslateMode, dlShowReleaseOnly } from "../store/config/downloaderConfig";
import {
  fetchVersions, fetchCommunityDetails,
  type CommunityVersion, type CommunityDependencyItem, type CommunityDisplayItem,
} from "../store/community/communitySearch";
import {
  downloadFiles,
  registerTaskName,
  type DownloadFileInput,
  type DownloadFileKind,
} from "../store/task/download";
import { resolveModDependencies, type ModDependencyPlan } from "../store/community/modDependencies";
import { installModpack } from "../store/task/modpack";
import ModDownloadDialog from "../component/download/ModDownloadDialog.vue";
import { fmtCount, fmtSize, fmtModDate } from "../utils/format";
import IconImage from "../component/common/IconImage.vue";

const item = computed(() => resourceDetail.item);
const { t } = useI18n();

const sourceLabel = computed(() =>
  item.value?.source === "modrinth" ? "Modrinth" : "CurseForge",
);

// ── 版本列表 ──────────────────────────────

const versions = ref<CommunityVersion[]>([]);
const loadingVersions = ref(false);
const versionError = ref("");

// ── 版本列表过滤（本地过滤，选项从已加载版本收集）──

const loaderFilter = ref("");
const gameVersionFilter = ref("");

const LOADER_ORDER = ["fabric", "forge", "neoforge", "quilt"];

const isMod = computed(() => resourceDetail.backTab === "mod");
const loaderOptions = computed(() =>
  Array.from(new Set(versions.value.flatMap(v => v.loaders))).sort(
    (a, b) => (LOADER_ORDER.indexOf(a) + 1 || 99) - (LOADER_ORDER.indexOf(b) + 1 || 99),
  ),
);

/** 主版本归类：提取开头 major.minor（"1.20.4"→"1.20"）；无该前缀（快照等）原样保留 */
function majorVersion(gv: string): string {
  const m = gv.match(/^\d+\.\d+/);
  return m ? m[0] : gv;
}

const loaderScopedVersions = computed(() =>
  loaderFilter.value
    ? versions.value.filter(v => v.loaders.includes(loaderFilter.value))
    : versions.value,
);

const gameVersionOptions = computed(() =>
  Array.from(new Set(loaderScopedVersions.value.flatMap(v => v.game_versions).map(majorVersion))).sort((a, b) =>
    new Intl.Collator(undefined, { numeric: true }).compare(b, a),
  ),
);

// 切换加载器后已选版本若不在新选项内则清空（继承值除外：列表搜索已按其过滤）
watch(gameVersionOptions, () => {
  if (
    gameVersionFilter.value &&
    gameVersionFilter.value !== inheritedVersion &&
    !gameVersionOptions.value.includes(gameVersionFilter.value)
  ) {
    gameVersionFilter.value = "";
  }
});

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

// ── 自定义下拉（WebView2 无法限制原生 select 弹出层高度）──

const openFilter = ref<"" | "loader" | "game">("");

const POP_MAX_H = 260;
/** 弹层最大高度：按下方可视空间动态计算（向下展开，不扩展外层容器 scrollHeight） */
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

/** 快照/预发布/候选等非正式版本标记（26w14a、26.3-snapshot-8、-pre1、-rc1、candidate） */
const SNAPSHOT_RE = /\d{2}w\d{2}|snapshot|pre\d*|rc\d*|candidate/i;
const isSnapshot = (gv: string): boolean => SNAPSHOT_RE.test(gv);

/** 当前选中版本文件 id（切换项目/重新加载时重置）；须在 loadVersions 前声明：immediate watch 在 setup 期即调用。 */
const selectedVersionId = ref("");

/** 版本请求序号：自增；响应返回时与当前值不符即过期（项目已切换/返回/重试）→ 丢弃。 */
let versionsSeq = 0;

async function loadVersions() {
  const seq = ++versionsSeq;
  const it = item.value;
  if (!it) return;
  // 先清空旧数据：否则依赖卡片会用旧列表 + 旧选中版本算出残留前置依赖
  versions.value = [];
  loadingVersions.value = true;
  versionError.value = "";
  try {
    const all = await fetchVersions(it.source, it.id);
    if (seq !== versionsSeq) return; // 过期响应丢弃
    // 仅显示正式版：剔除所有支持版本均为快照/预发布的文件（混版保留）
    versions.value = dlShowReleaseOnly.value
      ? all.filter(v => v.game_versions.some(gv => !isSnapshot(gv)))
      : all;
  } catch (e) {
    if (seq !== versionsSeq) return;
    versionError.value = errMsg(e);
  } finally {
    if (seq === versionsSeq) loadingVersions.value = false;
  }
}

/** 项目切换：应用 store 同步来的筛选/选中版本（进入/返回还原）后加载版本；列表页选项为完整 ID、详情页为主版本归类，继承时做 major 归一化并记录 inheritedVersion（watch 清空逻辑跳过它）。 */
let inheritedVersion: string | null = null;

function onItemChange() {
  versionsSeq++; // 项目切换：使在飞请求过期（返回恢复路径不调用 loadVersions）
  loaderFilter.value = resourceDetail.pendingLoader;
  inheritedVersion = majorVersion(resourceDetail.pendingGameVersion);
  gameVersionFilter.value = inheritedVersion;
  selectedVersionId.value = resourceDetail.pendingSelectedVersion;
  if (resourceDetail.pendingVersions !== null) {
    // 返回还原：恢复快照，不重新请求
    versions.value = resourceDetail.pendingVersions;
    versionError.value = resourceDetail.pendingVersionError;
    loadingVersions.value = false;
    resourceDetail.pendingVersions = null;
    resourceDetail.pendingVersionError = "";
    return;
  }
  void loadVersions();
}

watch(item, onItemChange, { immediate: true });

// ── 前置依赖（选中版本的 required 依赖；不同版本可能不同）──

const depItems = ref<CommunityDependencyItem[]>([]);
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

/** 选中版本的 incompatible 冲突项目 ID（去重保序；只读警示，不拦截安装）。 */
const selectedIncompatIds = computed(() => {
  const v = versions.value.find(x => x.version_id === selectedVersionId.value);
  if (!v) return [];
  const seen = new Set<string>();
  const order: string[] = [];
  for (const dep of v.dependencies) {
    if (dep.relation === "incompatible" && dep.project_id && !seen.has(dep.project_id)) {
      seen.add(dep.project_id);
      order.push(dep.project_id);
    }
  }
  return order;
});

const requiredIdSet = computed(() => new Set(selectedDepIds.value));
const incompatIdSet = computed(() => new Set(selectedIncompatIds.value));
const requiredItems = computed(() => depItems.value.filter(d => requiredIdSet.value.has(d.id)));
const incompatItems = computed(() => depItems.value.filter(d => incompatIdSet.value.has(d.id)));

/** 依赖请求序号：自增；响应返回时与当前值不符即过期（选中版本已切换/返回）→ 丢弃。 */
let depsSeq = 0;

async function loadDeps() {
  const seq = ++depsSeq;
  const it = item.value;
  const reqIds = selectedDepIds.value;
  const incIds = selectedIncompatIds.value;
  if (!it || (reqIds.length === 0 && incIds.length === 0)) {
    depItems.value = [];
    depError.value = "";
    return;
  }
  // 返回还原：恢复依赖快照，不重新请求（seq 已自增，在飞请求随之过期）
  if (resourceDetail.pendingDepItems !== null) {
    depItems.value = resourceDetail.pendingDepItems;
    depError.value = resourceDetail.pendingDepError;
    loadingDeps.value = false;
    resourceDetail.pendingDepItems = null;
    resourceDetail.pendingDepError = "";
    return;
  }
  loadingDeps.value = true;
  depError.value = "";
  try {
    const items = await fetchCommunityDetails(it.source, Array.from(new Set([...reqIds, ...incIds])));
    if (seq !== depsSeq) return; // 过期响应丢弃
    depItems.value = items;
  } catch (e) {
    if (seq !== depsSeq) return;
    depError.value = errMsg(e);
  } finally {
    if (seq === depsSeq) loadingDeps.value = false;
  }
}

watch(selectedDepIds, loadDeps);

function pickVersion(v: CommunityVersion) {
  selectedVersionId.value = v.version_id;
  // 同步回 store：随 tab 切换卸载重建时用 pendingSelectedVersion 恢复选中态
  resourceDetail.pendingSelectedVersion = v.version_id;
}

function depToItem(d: CommunityDependencyItem): CommunityDisplayItem {
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
    category: null,
    source: src,
    key: `${src}:${d.id}`,
  };
}

// ── 提交文件下载 ──────────────────────────────

/** 来源 tab → 落盘目录 kind；modpack 走链式安装（isModpack 分流），无单文件下载。 */
const fileKind = computed<DownloadFileKind | null>(() => {
  switch (resourceDetail.backTab) {
    case "mod": return "mod";
    case "shader": return "shader";
    case "resourcepack": return "resourcepack";
    case "datapack": return "datapack";
    case "worlds": return "world";
    default: return null;
  }
});

const isModpack = computed(() => resourceDetail.backTab === "modpack");

async function submitModpackInstall() {
  if (!selectedVersionId.value) {
    showToast(t("download.detail.selectFileFirst"));
    return;
  }
  const v = versions.value.find(x => x.version_id === selectedVersionId.value);
  if (!v) {
    showToast(t("download.detail.selectedFileMissing"));
    return;
  }
  try {
    await installModpack({
      name: item.value?.title ?? v.filename,
      source: {
        type: "online",
        url: v.download_url,
        sha1: v.sha1 ?? "",
        size: v.file_size,
      },
      // 平台项目图标：随请求透传，安装期持久化到版本目录供列表展示
      icon_url: item.value?.icon_url ?? null,
    });
    showToast(t("download.detail.modpackSubmitted"));
    backAfterSubmit();
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 提交成功后退出一层：依赖链进入的回到上一层详情，从列表进入的回到列表（多层依赖无需逐级手动返回）。 */
function backAfterSubmit() {
  goBack();
}

// ── 前置 Mod 确认 ──

const depDialogOpen = ref(false);
const depDialogLoading = ref(false);
const depDialogError = ref("");
const depPlan = ref<ModDependencyPlan | null>(null);
const depModName = ref("");
const depModVersion = ref("");
const depModLoader = ref("");
let depDialogSeq = 0;
let pendingMain: { version: CommunityVersion; target: string } | null = null;

function mainDownloadFile(version: CommunityVersion): DownloadFileInput {
  return { kind: "mod", url: version.download_url, filename: version.filename };
}

function dependencyDownloadFiles(selectedOptional: string[]): DownloadFileInput[] {
  if (!pendingMain) return [];
  const files = [mainDownloadFile(pendingMain.version)];
  const seen = new Set(files.map(file => file.filename));
  for (const dep of depPlan.value?.items ?? []) {
    if (!dep.url || !dep.filename || dep.error || dep.installed) continue;
    const included =
      dep.root_optional === null
      || (dep.root_optional !== null && selectedOptional.includes(dep.root_optional));
    if (!included || seen.has(dep.filename)) continue;
    seen.add(dep.filename);
    files.push({ kind: "mod", url: dep.url, filename: dep.filename });
  }
  return files;
}

async function submitModFiles(files: DownloadFileInput[], name: string) {
  const target = pendingMain?.target;
  if (!target || files.length === 0) return;
  try {
    const taskId = await downloadFiles(target, files);
    if (taskId) registerTaskName(taskId, name);
    showToast(t("download.detail.submitted"));
    depDialogOpen.value = false;
    pendingMain = null;
    backAfterSubmit();
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 解析前置并打开确认框；无前置则直接提交主 Mod。 */
async function openModDownload(version: CommunityVersion, target: string) {
  pendingMain = { version, target };
  depModName.value = item.value?.title ?? version.filename;
  depModVersion.value = version.version_number;
  depModLoader.value = version.loaders.join(" / ");
  depPlan.value = null;
  depDialogError.value = "";
  depDialogLoading.value = true;
  depDialogOpen.value = true;
  const seq = ++depDialogSeq;
  try {
    const plan = await resolveModDependencies({
      source: item.value?.source ?? "modrinth",
      projectId: item.value?.id ?? "",
      versionId: version.version_id,
      versionDirName: target,
    });
    if (seq !== depDialogSeq || !depDialogOpen.value) return;
    depPlan.value = plan;
    if (plan.items.length === 0 && plan.errors.length === 0) {
      await submitModFiles([mainDownloadFile(version)], depModName.value);
    }
  } catch (e) {
    if (seq !== depDialogSeq || !depDialogOpen.value) return;
    depDialogError.value = errMsg(e);
  } finally {
    if (seq === depDialogSeq) depDialogLoading.value = false;
  }
}

function onDependencyConfirm(selectedOptional: string[]) {
  void submitModFiles(dependencyDownloadFiles(selectedOptional), depModName.value);
}

function onDependencyMainOnly() {
  if (!pendingMain) return;
  void submitModFiles([mainDownloadFile(pendingMain.version)], depModName.value);
}

/** 提交选中版本下载任务（任务中心自动注册）；落盘 versions/{版本目录}/{kind_dir}/{filename}（后端兜底 _{n} 冲突）。 */
async function submitDownload() {
  // modpack 分流：不要求已选启动版本（安装器自建版本目录）
  if (isModpack.value) {
    await submitModpackInstall();
    return;
  }
  // 任何未满足条件都明确提示（不静默）
  if (!selectedVersionId.value) {
    showToast(t("download.detail.selectFileFirst"));
    return;
  }
  const kind = fileKind.value;
  if (!kind) {
    showToast(t("download.detail.unsupportedType"));
    return;
  }
  const v = versions.value.find(x => x.version_id === selectedVersionId.value);
  if (!v) {
    showToast(t("download.detail.selectedFileMissing"));
    return;
  }
  // 下载目标版本：实时读取 selectedVersion（下载完成后 onTaskDone 自动选中更新）
  const targetVersion = selectedVersion.value;
  if (!targetVersion) {
    showToast(t("download.detail.noTargetVersion"));
    return;
  }
  if (kind === "mod") {
    await openModDownload(v, targetVersion);
    return;
  }
  try {
    // 诊断：invoke 超时兜底（5s）——后端命令卡住时给出反馈而非静默"无反应"
    const taskId = await Promise.race([
      downloadFiles(targetVersion, [
        { kind, url: v.download_url, filename: v.filename },
      ]),
      new Promise<never>((_, rej) =>
        setTimeout(() => rej(new Error(t("download.detail.requestTimeout"))), 5000),
      ),
    ]);
    // 注册资源名：完成提示显示 "下载任务[资源名]完成"（fallback 文件名）
    if (taskId) {
      registerTaskName(taskId, item.value?.title ?? v.filename);
    }
    showToast(t("download.detail.submitted"));
    backAfterSubmit();
  } catch (e) {
    showToast(errMsg(e));
  }
}

// ── 操作 ──────────────────────────────

function gotoSource() {
  const it = item.value;
  if (!it?.page_url) return;
  openUrl(it.page_url).catch(() => showToast(t("download.detail.openLinkFailed")));
}

function gotoMcmod() {
  const it = item.value;
  if (!it) return;
  const url = it.mcmod_id
    ? `https://www.mcmod.cn/class/${it.mcmod_id}.html`
    : `https://search.mcmod.cn/s?key=${encodeURIComponent(it.title_zh ?? it.title)}&mold=0`;
  openUrl(url).catch(() => showToast(t("download.detail.openLinkFailed")));
}

async function copyName() {
  const it = item.value;
  if (!it) return;
  const text = it.title_zh ? `${it.title_zh} | ${it.title}` : it.title;
  try {
    await navigator.clipboard.writeText(text);
    showToast(t("download.detail.copiedName"));
  } catch {
    showToast(t("download.detail.copyFailed"));
  }
}
</script>

<template>
  <div v-if="item" class="md">
    <!-- 项目切换动画：内容整体淡入淡出（同路由 push 不触发路由级过渡） -->
    <Transition name="md-switch" mode="out-in">
      <div :key="item.key" class="md-body">
        <!-- 卡片一：项目信息（样式与外部 item 类似） -->
        <div class="md-card">
          <div class="md-row">
            <IconImage :src="item.icon_url" :size="40" />
            <span class="md-info">
              <span class="md-title-row">
                <span class="md-title">{{ item.title_zh ?? item.title }}</span>
                <span v-if="dlTranslateMode === 'zh' && item.title_zh" class="md-title-en"> | {{ item.title }}</span>
                <span class="md-badge" :class="item.source">{{ sourceLabel }}</span>
                <span class="md-sub">{{ t("download.detail.downloads", { count: fmtCount(item.downloads) }) }}</span>
              </span>
            </span>
          </div>
          <div class="md-desc">{{ item.translated ?? item.description }}</div>

          <div class="md-actions">
            <button class="md-btn" @click="gotoSource">{{ t("download.detail.gotoSource", { source: sourceLabel }) }}</button>
            <!-- 光影包/地图存档无 MC百科条目，隐藏跳转 -->
            <button
              v-if="item.category !== 'shader' && item.category !== 'worlds'"
              class="md-btn"
              @click="gotoMcmod"
            >{{ t("download.detail.gotoMcmod") }}</button>
            <button class="md-btn" @click="copyName">{{ t("download.detail.copyName") }}</button>
          </div>
        </div>

        <div class="md-cards-row">
          <div class="md-card md-versions">
            <div class="mv-head">
              <span class="mv-title">{{ t("download.detail.versionFiles") }}<span v-if="!loadingVersions" class="mv-count"> | {{ t("download.detail.versionCount", { count: filteredVersions.length }, filteredVersions.length) }}</span></span>
              <span class="mv-spacer"></span>

              <button
                v-if="fileKind || isModpack"
                class="mv-dl"
                :class="{ 'mv-dl-disabled': !selectedVersionId }"
                v-tooltip="selectedVersionId ? (isModpack ? t('download.detail.installTooltip') : t('download.detail.downloadTooltip')) : t('download.detail.selectVersionTooltip')"
                @click="submitDownload"
              >{{ isModpack ? t("download.detail.install") : t("download.detail.download") }}</button>

              <div v-if="isMod" ref="loaderDrop" class="mv-drop" @pointerdown.stop @click.stop="toggleFilter('loader')">
                <span class="mv-drop-label">{{ loaderFilter ? loaderLabel(loaderFilter) : t("download.search.allLoaders") }}</span>
                <span class="mv-drop-arrow" :class="{ open: openFilter === 'loader' }"></span>
                <Transition name="expand">
                  <div v-if="openFilter === 'loader'" class="mv-drop-pop" :style="{ '--expand-h': popMaxH + 'px' }">
                    <div class="mv-drop-opt" :class="{ sel: !loaderFilter }" @click.stop="pickLoader('')">{{ t("download.search.allLoaders") }}</div>
                    <div v-for="l in loaderOptions" :key="l" class="mv-drop-opt" :class="{ sel: loaderFilter === l }" @click.stop="pickLoader(l)">{{ loaderLabel(l) }}</div>
                  </div>
                </Transition>
              </div>

              <div ref="gameDrop" class="mv-drop" @pointerdown.stop @click.stop="toggleFilter('game')">
                <span class="mv-drop-label">{{ gameVersionFilter || t("download.search.allVersions") }}</span>
                <span class="mv-drop-arrow" :class="{ open: openFilter === 'game' }"></span>
                <Transition name="expand">
                  <div v-if="openFilter === 'game'" class="mv-drop-pop" :style="{ '--expand-h': popMaxH + 'px' }">
                    <div class="mv-drop-opt" :class="{ sel: !gameVersionFilter }" @click.stop="pickGame('')">{{ t("download.search.allVersions") }}</div>
                    <div v-for="gv in gameVersionOptions" :key="gv" class="mv-drop-opt" :class="{ sel: gameVersionFilter === gv }" @click.stop="pickGame(gv)">{{ gv }}</div>
                  </div>
                </Transition>
              </div>
            </div>

            <div v-if="loadingVersions" class="mv-state">
              <div class="spinner"></div>
              <p class="mv-state-text">{{ t("download.detail.loadingVersions") }}</p>
            </div>
            <div v-else-if="versionError" class="mv-state err">
              <p>{{ versionError }}</p>
              <button class="md-btn mv-retry" @click="loadVersions">{{ t("download.detail.retry") }}</button>
            </div>
            <div v-else-if="versions.length === 0" class="mv-state">{{ t("download.detail.noVersions") }}</div>
            <div v-else class="mv-list">
              <div v-if="filteredVersions.length === 0" class="mv-state">{{ t("download.detail.noMatchingVersions") }}</div>
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

          <Transition name="mv-side">
            <div v-if="selectedVersionId && (selectedDepIds.length > 0 || selectedIncompatIds.length > 0)" class="md-card md-deps-card">
              <div class="mv-head">
                <span class="mv-title">{{ t("download.detail.dependencies") }}<span v-if="!loadingDeps" class="mv-count"> | {{ t("download.detail.dependencyCount", { count: selectedDepIds.length }, selectedDepIds.length) }}</span></span>
              </div>
              <div v-if="!loadingDeps && incompatItems.length > 0" class="md-dep-warn">
                <div class="md-dep-warn-title">{{ t("download.detail.incompatibleTitle", { count: incompatItems.length }) }}</div>
                <div v-for="d in incompatItems" :key="d.id" class="md-dep-warn-item">{{ d.title }}</div>
                <div class="md-dep-warn-tip">{{ t("download.detail.incompatibleTip") }}</div>
              </div>
              <div v-if="loadingDeps" class="md-dep-state">
                <div class="spinner"></div>
                <p class="md-dep-state-text">{{ t("download.detail.loadingDependencies") }}</p>
              </div>
              <div v-else-if="depError" class="md-dep-state err">{{ depError }}</div>
              <div v-else-if="depItems.length === 0" class="md-dep-empty">{{ t("download.detail.noDependencies") }}</div>
              <div v-else class="md-dep-list">
                <div v-for="d in requiredItems" :key="d.id" class="md-dep-item" @click="openResourceDetail(
                  depToItem(d),
                  { loader: loaderFilter, gameVersion: gameVersionFilter, selectedVersion: selectedVersionId },
                  { versions, versionError, depItems, depError },
                )">
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
  <div v-else class="md-empty">{{ t("download.detail.notFound") }}</div>

  <ModDownloadDialog
    v-model="depDialogOpen"
    :loading="depDialogLoading"
    :error="depDialogError"
    :mod-name="depModName"
    :mod-version="depModVersion"
    :mod-loader="depModLoader"
    :plan="depPlan"
    @confirm="onDependencyConfirm"
    @main-only="onDependencyMainOnly"
  />
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

.md-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  height: 100%;
}
.md-body > .md-card { flex-shrink: 0; }

/* 双卡片并列：左版本文件 flex:1，右前置依赖固定 240px（动态出现） */
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

.md-title-en {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ModLoader 徽章（与外部 item 同款；专属色随主题变量切换） */
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
  color: var(--ld-fabric);
}
.md-ld-forge {
  color: var(--ld-forge);
}
.md-ld-neoforge {
  color: var(--ld-neoforge);
}
.md-ld-quilt {
  color: var(--ld-quilt);
}
.md-ld-optifine {
  color: var(--ld-optifine);
}

/* 徽章底色：color-mix 需 WebKitGTK ≥ 2.42（Ubuntu 22.04 的 2.40 不支持）；不支持则降级为纯文字色徽章 */
@supports (color-mix(in srgb, red 12%, transparent)) {
  .md-ld-fabric {
    background: color-mix(in srgb, var(--ld-fabric) 12%, transparent);
  }
  .md-ld-forge {
    background: color-mix(in srgb, var(--ld-forge) 12%, transparent);
  }
  .md-ld-neoforge {
    background: color-mix(in srgb, var(--ld-neoforge) 12%, transparent);
  }
  .md-ld-quilt {
    background: color-mix(in srgb, var(--ld-quilt) 12%, transparent);
  }
  .md-ld-optifine {
    background: color-mix(in srgb, var(--ld-optifine) 12%, transparent);
  }
}

.md-badge {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  color: var(--text-inverse);
  font-weight: 600;
}
.md-badge.modrinth { background: var(--color-modrinth); }
.md-badge.curseforge { background: var(--color-curseforge); }

.md-sub {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.md-desc {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle, var(--border-default));
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
}

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

.mv-count {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
}

.mv-spacer {
  flex: 1;
}

.mv-dl {
  flex-shrink: 0;
  height: 30px;
  padding: 0 16px;
  border: 1px solid var(--color-brand);
  border-radius: var(--radius-sm);
  background: var(--color-brand);
  color: var(--text-inverse);
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s;
  outline: none;
}
.mv-dl:hover:not(.mv-dl-disabled) {
  filter: brightness(1.12);
}
.mv-dl:active:not(.mv-dl-disabled) {
  filter: brightness(0.95);
}
.mv-dl-disabled {
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
  max-height: var(--expand-h, 260px);
  overflow-y: auto;
  background: var(--surface-card);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md);
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

/* expand 动画由共享骨架（styles/transitions.css）提供；元素通过内联 --expand-h 注入目标高度 */

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
.mv-type.beta { background: var(--color-warning-bg); color: var(--color-warning); }

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
  color: var(--color-danger);
  font-size: 12px;
}

.md-dep-warn {
  display: flex;
  flex-direction: column;
  gap: 3px;
  margin-top: 12px;
  padding: 10px 12px;
  border: 1px solid var(--color-danger);
  border: 1px solid color-mix(in srgb, var(--color-danger) 35%, transparent);
  border-radius: var(--radius-sm);
  background: var(--color-danger-bg);
}
.md-dep-warn-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-danger);
}
.md-dep-warn-item {
  font-size: 11.5px;
  color: var(--color-danger);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.md-dep-warn-tip {
  font-size: 10.5px;
  color: var(--color-danger);
  opacity: 0.85;
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
