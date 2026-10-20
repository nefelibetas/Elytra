<script setup lang="ts">
// 主页右侧版本卡片：Tab 切换资源列表 + 底部操作。业务在 store/versionDetail。
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { Pencil, Folder, FolderOpen, CloudDownload, Puzzle, Map, Settings2, Palette, Database, Sun, ImagePlus } from "@lucide/vue";
import { open } from "@tauri-apps/plugin-dialog";
import { fmtUnix } from "../../utils/format";
import { ensureGameFolder, renameVersionDir } from "../../store/config/gameDirs";
import { scanMissingFiles, submitMissingDownload } from "../../store/task/download";
import { openFolder } from "../../utils/fs";
import { classifyVersion } from "../../utils/versionClassify";
import { LOADER_ICONS, LOADER_TITLES } from "../../utils/versionMeta";
import { fmtCount, fmtSize } from "../../utils/format";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import {
  versionIcons, loadVersionIcon, setVersionIcon,
} from "../../store/task/versionIcon";
import { isLaunching } from "../../store/app/launch";
import VersionItem from "../download/VersionItem.vue";
import AlertDialog from "../common/AlertDialog.vue";
import IconImage from "../common/IconImage.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";
import { dlState } from "../../store/task/download";
import { dlTranslateMode } from "../../store/config/downloaderConfig";
import { selectedVersion, setSelected } from "../../store/app/selectedVersion";
import {
  worlds, worldsLoading, worldsError, worldIcons,
  mods, modsLoading, modsError, modsNetworkError,
  resourceRefs,
  isModLoaderVersion, versionLoader,
  resetVersionDetail, loadVersionLoader, loadWorlds, loadMods, loadModsQuick, loadResourceLists,
  startWorldsLivePolling, stopWorldsLivePolling,
  type ResourceKind,
} from "../../store/task/versionDetail";

const { t } = useI18n();
const versionName = computed(() => selectedVersion.value);

const info = computed(() => classifyVersion(versionName.value));
/** loader 判定：后端 json 特征推断优先，目录名分类兜底；与 gameDirs.ts 同规则，两处图标须一致。 */
const category = computed(() => versionLoader.value || info.value.category);
const displayIcon = computed(() => versionIcons[versionName.value] ?? LOADER_ICONS[category.value] ?? LOADER_ICONS.vanilla);
watch(versionName, name => void loadVersionIcon(name), { immediate: true });
/** 修改图标：选图 → 后端魔数校验后复制为自定义图标。 */
async function changeIcon() {
  const path = await open({
    multiple: false,
    filters: [{ name: t("launch.version.imageFilter"), extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  if (!path) return;
  try {
    await setVersionIcon(versionName.value, String(path));
    showToast(t("launch.version.iconUpdated"));
  } catch (e) {
    showToast(errMsg(e));
  }
}
/** loader 版本号：目录名可解析且与最终 category 一致才显示。 */
const loaderVersion = computed(() =>
  info.value.category === category.value ? info.value.loaderVersion : "",
);
const subtitle = computed(() =>
  category.value === "vanilla"
    ? t("launch.version.vanilla")
    : loaderVersion.value || (LOADER_TITLES[category.value] ?? category.value),
);

const router = useRouter();

/** 进入版本设置页；返回按历史回主页。 */
function goSetup() {
  router.push({ path: "/version-setup", query: { name: versionName.value } });
}

/** 当前 tab（组件内状态，路由重建后回存档）。 */
type TabKind = "worlds" | "mods" | "resourcepacks" | "datapacks" | "shaderpacks";
const tab = ref<TabKind>("worlds");

const TAB_LABEL_KEYS: Record<TabKind, string> = {
  worlds: "launch.version.tabs.worlds",
  mods: "launch.version.tabs.mods",
  resourcepacks: "launch.version.tabs.resourcepacks",
  datapacks: "launch.version.tabs.datapacks",
  shaderpacks: "launch.version.tabs.shaderpacks",
};

const SIDE_ICONS = {
  worlds: Map,
  mods: Puzzle,
  resourcepacks: Palette,
  datapacks: Database,
  shaderpacks: Sun,
} as const;

/** 可见 Tab：存档/光影/材质包恒显，数据包需 1.13+（兼容 26.x 命名版本号），Mod 仅 ModLoader 版本。 */
function supportsDatapacks(mc: string): boolean {
  const parts = mc.split(".").map(Number);
  const min = [1, 13];
  for (let i = 0; i < Math.max(parts.length, min.length); i++) {
    const a = parts[i] ?? 0;
    const b = min[i] ?? 0;
    if (a !== b) return a > b;
  }
  return true;
}
const tabs = computed<TabKind[]>(() => {
  const list: TabKind[] = ["worlds", "shaderpacks", "resourcepacks"];
  if (supportsDatapacks(info.value.mcVersion)) list.push("datapacks");
  if (isModLoaderVersion.value) list.push("mods");
  return list;
});

watch(tabs, kinds => {
  if (!kinds.includes(tab.value)) tab.value = "worlds";
});

const folderMeta = computed(() => {
  switch (tab.value) {
    case "mods":
      return { kind: "mods" as const, labelKey: "launch.action.openMods" };
    case "resourcepacks":
      return { kind: "resourcepacks" as const, labelKey: "launch.action.openResourcepacks" };
    case "datapacks":
      return { kind: "datapacks" as const, labelKey: "launch.action.openDatapacks" };
    case "shaderpacks":
      return { kind: "shaderpacks" as const, labelKey: "launch.action.openShaderpacks" };
    default:
      return { kind: "saves" as const, labelKey: "launch.action.openSaves" };
  }
});

// ── 资源目录（材质包/数据包/光影包）通用列表数据 ──
// 状态在 store resourceRefs，按 tab 解包；非资源 tab 给空态。
function isResourceTab(t: TabKind): t is ResourceKind {
  return t === "resourcepacks" || t === "datapacks" || t === "shaderpacks";
}

const res = computed(() => {
  if (!isResourceTab(tab.value)) {
    return { list: [], loading: false, error: "", networkError: false };
  }
  const r = resourceRefs(tab.value);
  return {
    list: r.list.value,
    loading: r.loading.value,
    error: r.error.value,
    networkError: r.networkError.value,
  };
});

const RES_ICONS = { resourcepacks: Palette, datapacks: Database, shaderpacks: Sun } as const;
const RES_EMPTY_KEYS: Record<ResourceKind, string> = {
  resourcepacks: "launch.version.emptyResourcepacks",
  datapacks: "launch.version.emptyDatapacks",
  shaderpacks: "launch.version.emptyShaderpacks",
};
const resIcon = computed(() => (isResourceTab(tab.value) ? RES_ICONS[tab.value] : Palette));
const resEmptyText = computed(() =>
  isResourceTab(tab.value) ? t(RES_EMPTY_KEYS[tab.value]) : "",
);

// ── 版本切换：重置 + 加载 loader/存档/Mod/资源目录 ──
watch(
  versionName,
  nv => {
    if (!nv) return;
    resetVersionDetail();
    void loadVersionLoader(nv);
    void loadWorlds(nv);
    startWorldsLivePolling(nv);
    void loadModsQuick(nv);
    void loadMods(nv);
    loadResourceLists(nv);
  },
  { immediate: true },
);

onBeforeUnmount(() => stopWorldsLivePolling());

// ── 打开文件夹（不存在则先由后端 ensure_game_folder 创建）──
async function openVersionFolder(kind: "saves" | "version" | "mods" | "resourcepacks" | "datapacks" | "shaderpacks") {
  try {
    const target = await ensureGameFolder(kind, versionName.value);
    await openFolder(target);
  } catch (e) {
    showToast(errMsg(e));
  }
}

// ── 修复：扫描缺失 → 提交任务中心 ──
const checking = ref(false);

async function repairFiles() {
  if (checking.value || isLaunching.value) return;
  checking.value = true;
  try {
    const report = await scanMissingFiles(versionName.value);
    const total =
      (report.client_jar ? 1 : 0) + report.libraries + report.assets + report.natives;
    if (total === 0) {
      showToast(t("launch.version.repairComplete"));
      return;
    }
    const taskId = await submitMissingDownload(versionName.value);
    // 预注册任务：徽标立即出现，不等首次 task-progress 事件
    dlState.addTask({
      taskId,
      name: t("launch.version.repairTaskName", { version: versionName.value }),
      nameKey: "task.backend.name.repair",
      nameParams: { version: versionName.value },
      status: "pending",
      progress: 0,
      speedBps: 0,
      etaSeconds: null,
      completedFiles: 0,
      totalFiles: 0,
      completedBytes: 0,
      totalBytes: 0,
      currentPhase: t("launch.version.waiting"),
      currentPhaseKey: "task.backend.phase.waiting",
      steps: [],
    });
    showToast(t("launch.version.repairSubmitted", { count: total }));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    checking.value = false;
  }
}

// ── 重命名版本 dialog ──
const renameDialog = ref(false);
const renameInput = ref("");
const renameError = ref("");

function openRename() {
  renameInput.value = versionName.value;
  renameError.value = "";
  renameDialog.value = true;
}

async function confirmRename() {
  const newName = renameInput.value.trim();
  if (!newName) {
    renameError.value = t("launch.version.nameRequired");
    return;
  }
  if (newName === versionName.value) {
    renameError.value = t("launch.version.nameSame");
    return;
  }
  try {
    await renameVersionDir(versionName.value, newName);
    renameDialog.value = false;
    // 保持选中新名，触发卡片 watch 刷新
    setSelected(newName);
  } catch (e) {
    renameError.value = errMsg(e);
  }
}

</script>

<template>
  <div class="vdc">
    <!-- 头部：版本信息 + 版本设置 / 重命名 -->
    <div class="vdc-head">
      <VersionItem :icon-src="displayIcon" :version-id="versionName" :subtitle="subtitle">
        <template #action>
          <button class="vdc-icon-btn" v-tooltip="t('launch.action.setup')" @click="goSetup">
            <Settings2 :size="15" />
          </button>
          <button class="vdc-icon-btn" v-tooltip="t('launch.action.addIcon')" @click="changeIcon">
            <ImagePlus :size="15" />
          </button>
          <button class="vdc-icon-btn" v-tooltip="t('launch.action.rename')" @click="openRename">
            <Pencil :size="15" />
          </button>
        </template>
      </VersionItem>
    </div>

    <!-- 中间层：侧边资源导航 + 列表区 -->
    <div class="vdc-main">
      <nav class="vdc-side">
        <div class="vdc-side-nav">
          <button
            v-for="kind in tabs"
            :key="kind"
            type="button"
            class="vdc-side-btn"
            :class="{ active: tab === kind }"
            v-tooltip="t(TAB_LABEL_KEYS[kind])"
            @click="tab = kind"
          >
            <component :is="SIDE_ICONS[kind]" :size="17" />
          </button>
        </div>
        <button
          type="button"
          class="vdc-side-open"
          v-tooltip="t(folderMeta.labelKey)"
          @click="openVersionFolder(folderMeta.kind)"
        >
          <FolderOpen :size="15" />
        </button>
      </nav>
      <div class="vdc-list-area">
        <Transition name="vdc-fade" mode="out-in">
      <!-- ── 存档 tab ── -->
      <div v-if="tab === 'worlds'" key="worlds" class="vdc-pane">
        <LoadingSpinner v-if="worldsLoading" :text="t('launch.version.loadingWorlds')" />
        <ErrorState v-else-if="worldsError" :text="worldsError" />
        <EmptyState v-else-if="worlds.length === 0">
          <Map :size="40" :stroke-width="1.5" opacity="0.5" />
          <p>{{ t("launch.version.emptyWorlds") }}</p>
        </EmptyState>
        <div v-else class="vdc-list">
          <div v-for="w in worlds" :key="w.name" class="vdc-row vdc-world-row">
            <IconImage
              v-if="worldIcons[w.name]"
              :src="worldIcons[w.name]"
              :size="36"
              class="vdc-world-thumb"
            />
            <Map v-else :size="18" class="vdc-row-icon" />
            <div class="vdc-row-info">
              <span class="vdc-row-name">{{ w.name }}</span>
              <span class="vdc-row-meta">{{ t("launch.version.lastPlayed", { date: fmtUnix(w.last_modified) }) }}</span>
            </div>
          </div>
        </div>
        </div>

      <!-- ── Mod tab ── -->
      <div v-else-if="tab === 'mods'" key="mods" class="vdc-pane">
        <LoadingSpinner v-if="modsLoading && mods.length === 0" :text="t('launch.version.loadingMods')" />
        <ErrorState v-else-if="modsError" :text="modsError" />
        <EmptyState v-else-if="mods.length === 0">
          <Puzzle :size="40" :stroke-width="1.5" opacity="0.5" />
          <p v-if="isModLoaderVersion">{{ t("launch.version.emptyModsLoader") }}</p>
          <p v-else>{{ t("launch.version.emptyMods") }}</p>
        </EmptyState>
        <template v-else>
          <div v-if="modsNetworkError" class="vdc-net-warn">
            {{ t("launch.version.modInfoPartial") }}
          </div>
          <div class="vdc-mod-list">
            <template v-for="m in mods" :key="m.file_name">
              <!-- 已识别：社区元数据样式 -->
              <div v-if="m.project_id" class="vdc-mod">
                <IconImage :src="m.icon_url" :size="32" />
                <span class="vdc-mod-info">
                  <span class="vdc-mod-title-row">
                    <span class="vdc-mod-title">{{ m.title_zh ?? m.title ?? m.file_name }}</span>
                    <span v-if="m.title_zh" class="vdc-mod-file">{{ m.file_name }}</span>
                    <span v-if="m.downloads" class="vdc-mod-sub">{{ t("launch.version.downloads", { count: fmtCount(m.downloads) }, m.downloads) }}</span>
                  </span>
                  <span v-if="m.description" class="vdc-mod-desc">
                    {{ dlTranslateMode === "en" ? m.description : (m.description_zh ?? m.description) }}
                  </span>
                  <span v-if="m.current_version" class="vdc-mod-support">
                    <span class="vdc-mod-ver">v{{ m.current_version }}</span>
                  </span>
                </span>
              </div>
              <!-- 未识别：本地文件样式 -->
              <div v-else class="vdc-mod vdc-mod-local">
                <Puzzle :size="20" class="vdc-mod-local-icon" />
                <span class="vdc-mod-info">
                  <span class="vdc-mod-local-name">{{ m.file_name }}</span>
                  <span class="vdc-mod-local-meta">{{ fmtSize(m.size) }}</span>
                </span>
              </div>
            </template>
          </div>
        </template>
        </div>

      <!-- ── 材质包 / 数据包 / 光影包 tab（Modrinth 识别双态列表）── -->
      <div v-else key="resources" class="vdc-pane">
        <LoadingSpinner v-if="res.loading && res.list.length === 0" :text="t('launch.version.loading')" />
        <ErrorState v-else-if="res.error" :text="res.error" />
        <EmptyState v-else-if="res.list.length === 0">
          <component :is="resIcon" :size="40" :stroke-width="1.5" opacity="0.5" />
          <p>{{ resEmptyText }}</p>
        </EmptyState>
        <template v-else>
          <div v-if="res.networkError" class="vdc-net-warn">
            {{ t("launch.version.resourceInfoPartial") }}
          </div>
          <div class="vdc-mod-list">
            <template v-for="f in res.list" :key="f.file_name">
              <!-- 已匹配社区项目 -->
              <div v-if="f.project_id" class="vdc-mod">
                <IconImage :src="f.icon_url" :size="32" />
                <span class="vdc-mod-info">
                  <span class="vdc-mod-title-row">
                    <span class="vdc-mod-title">{{ f.title_zh ?? f.title ?? f.file_name }}</span>
                    <span v-if="f.title_zh" class="vdc-mod-file">{{ f.file_name }}</span>
                    <span v-if="f.downloads" class="vdc-mod-sub">{{ t("launch.version.downloads", { count: fmtCount(f.downloads) }, f.downloads) }}</span>
                  </span>
                  <span v-if="f.description" class="vdc-mod-desc">
                    {{ dlTranslateMode === "en" ? f.description : (f.description_zh ?? f.description) }}
                  </span>
                  <span v-if="f.current_version" class="vdc-mod-support">
                    <span class="vdc-mod-ver">v{{ f.current_version }}</span>
                  </span>
                </span>
              </div>
              <!-- 未匹配：本地文件 -->
              <div v-else class="vdc-mod vdc-mod-local">
                <component :is="resIcon" :size="20" class="vdc-mod-local-icon" />
                <span class="vdc-mod-info">
                  <span class="vdc-mod-local-name">{{ f.file_name }}</span>
                  <span class="vdc-mod-local-meta">{{ fmtSize(f.size) }}</span>
                </span>
              </div>
            </template>
          </div>
        </template>
        </div>
        </Transition>
      </div>
    </div>

    <!-- 底部操作 -->
    <div class="vdc-foot">
      <button class="vdc-foot-btn" @click="openVersionFolder('version')">
        <Folder :size="14" />
        <span>{{ t("launch.action.gameFolder") }}</span>
      </button>
      <button class="vdc-foot-btn" :disabled="checking || isLaunching" :title="isLaunching ? t('launch.status.gameStarting') : ''" @click="repairFiles">
        <span v-if="checking" class="vdc-repair-spinner"></span>
        <CloudDownload v-else :size="14" />
        <span>{{ checking ? t("launch.action.checking") : t("launch.action.repair") }}</span>
      </button>
    </div>

    <!-- 重命名版本 dialog -->
    <AlertDialog v-model="renameDialog" :title="t('launch.version.renameTitle')">
      <div class="vdc-dialog-body">
        <input
          v-model="renameInput"
          class="vdc-input"
          type="text"
          maxlength="64"
          @keydown.enter="confirmRename"
          autocomplete="off"
        />
        <div v-if="renameError" class="vdc-err">{{ renameError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmRename">{{ t("launch.version.confirm") }}</button>
        <button class="dialog-btn" @click="renameDialog = false">{{ t("launch.version.cancel") }}</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vdc {
  /* 占满右区，内容 flex column 分布 */
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: 16px;
  box-shadow: var(--shadow-md);
  overflow: hidden;
}

/* ===== 头部 ===== */
.vdc-head {
  flex: none;
  border-bottom: 1px solid var(--border-default);
}

.vdc-head :deep(.ver-item) {
  padding: 14px 16px;
}

.vdc-icon-btn {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.vdc-icon-btn:hover {
  background-color: var(--color-brand-bg);
  color: var(--color-brand);
}

/* ===== 中间层：侧边资源导航 + 列表区 ===== */
.vdc-main {
  flex: 1;
  min-height: 0;
  display: flex;
  overflow: hidden;
}

.vdc-side {
  flex: none;
  width: 44px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0;
  border-right: 1px solid var(--border-default);
  background: var(--surface-elevated);
}

.vdc-side-nav {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}

.vdc-side-btn {
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.vdc-side-btn:hover {
  color: var(--text-primary);
  background: var(--surface-hover);
}

.vdc-side-btn.active {
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

/* 打开当前资源目录，底部常驻 */
.vdc-side-open {
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border-strong);
  border-radius: 8px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}

.vdc-side-open:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

@keyframes vdc-spin {
  to {
    transform: rotate(360deg);
  }
}

/* ===== 列表区（tab 内容 pane 内滚动） ===== */
.vdc-list-area {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

/* tab 内容 pane：滚动容器 + 切换淡入淡出 */
.vdc-pane {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
}

.vdc-fade-enter-active,
.vdc-fade-leave-active {
  transition: opacity 0.15s ease;
}

.vdc-fade-enter-from,
.vdc-fade-leave-to {
  opacity: 0;
}

.vdc-err {
  padding: 20px;
  font-size: 12px;
  color: var(--color-danger);
  text-align: center;
}

.vdc-list {
  padding: 6px 0;
}

.vdc-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 16px;
  transition: background 0.1s;
}

.vdc-row:hover {
  background: var(--surface-hover);
}

.vdc-row-icon {
  flex: none;
  color: var(--text-tertiary);
}

/* 世界行：缩略图或 Map 占位 */
.vdc-world-row {
  padding: 6px 16px;
}

.vdc-world-thumb {
  flex: none;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid var(--border-default);
}

.vdc-row-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.vdc-row-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vdc-row-meta {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* ===== Mod 列表 ===== */
.vdc-net-warn {
  font-size: 11px;
  color: var(--color-danger);
  padding: 6px 16px 0;
  text-align: center;
}

.vdc-mod-list {
  display: flex;
  flex-direction: column;
  padding: 4px 8px;
}

.vdc-mod {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 8px;
  border-radius: var(--radius-sm);
  transition: background 0.1s;
}

.vdc-mod:hover {
  background: var(--surface-hover);
}

.vdc-mod-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vdc-mod-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.vdc-mod-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vdc-mod-file {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 1;
}

.vdc-mod-sub {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.vdc-mod-desc {
  font-size: 11px;
  color: var(--text-tertiary);
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.vdc-mod-support {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.vdc-mod-ver {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

/* 未识别 mod：本地文件样式 */
.vdc-mod-local {
  gap: 10px;
}

.vdc-mod-local-icon {
  flex: none;
  color: var(--text-tertiary);
  margin-left: 6px;
}

.vdc-mod-local-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.vdc-mod-local-meta {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* ===== 底部 ===== */
.vdc-foot {
  flex: none;
  display: flex;
  gap: 8px;
  padding: 10px 16px;
  border-top: 1px solid var(--border-default);
  background: var(--surface-elevated);
}

.vdc-foot-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 8px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  cursor: pointer;
  transition: all 0.15s;
}

.vdc-foot-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vdc-foot-btn:disabled {
  opacity: 0.65;
  cursor: default;
  border-color: var(--border-strong);
  color: var(--text-secondary);
}

/* 修复按钮检测态环形进度条（复用 vdc-spin keyframes） */
.vdc-repair-spinner {
  width: 13px;
  height: 13px;
  border: 2px solid var(--border-strong);
  border-top-color: var(--color-brand);
  border-radius: 50%;
  animation: vdc-spin 0.8s linear infinite;
}

/* dialog */
.vdc-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.vdc-input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}

.vdc-input:focus {
  border-color: var(--color-brand);
}

</style>
