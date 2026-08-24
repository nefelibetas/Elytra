<script setup lang="ts">
// 主页右侧版本卡片：单卡片，Tab 切换存档 / Mod 内容 + 打开存档按钮 + 底部（游戏文件夹 / 修复）。
// 业务在 store/versionDetail；重命名/修复/打开文件夹为本地编排。
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Pencil, Folder, FolderOpen, CloudDownload, Puzzle, Map, Settings2, Palette, Database, Sun, ImagePlus } from "@lucide/vue";
import { open } from "@tauri-apps/plugin-dialog";
import { fmtUnix } from "../utils/format";
import { invoke } from "@tauri-apps/api/core";
import { classifyVersion } from "../utils/versionClassify";
import { LOADER_ICONS, LOADER_TITLES } from "../utils/versionMeta";
import { fmtCount, fmtSize } from "../utils/format";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import {
  versionIcons, loadVersionIcon, setVersionIcon,
} from "../store/versionIcon";
import { isLaunching } from "../store/launch";
import VersionItem from "./VersionItem.vue";
import AlertDialog from "./AlertDialog.vue";
import IconImage from "./IconImage.vue";
import LoadingSpinner from "./LoadingSpinner.vue";
import EmptyState from "./EmptyState.vue";
import ErrorState from "./ErrorState.vue";
import { dlState } from "../store/download";
import { dlTranslateMode } from "../store/downloaderConfig";
import { selectedVersion, setSelected } from "../store/selectedVersion";
import {
  worlds, worldsLoading, worldsError, worldIcons, loadWorldIcon,
  mods, modsQuick, modsLoading, modsError, modsNetworkError,
  resourceRefs,
  isModLoaderVersion, versionLoader,
  resetVersionDetail, loadVersionLoader, loadWorlds, loadMods, loadModsQuick, loadResourceLists,
  type ResourceKind,
} from "../store/versionDetail";

const versionName = computed(() => selectedVersion.value);
const info = computed(() => classifyVersion(versionName.value));
/** loader 判定：后端 json 特征推断优先（PCL2 等目录名无标记实例也能识别），目录名分类兜底。
 *  与版本选择页（gameDirs.ts）同规则，保证两处图标一致。 */
const category = computed(() => versionLoader.value || info.value.category);
/** 展示图标：自定义/整合包项目图（懒加载缓存）优先，回退 loader 图标。 */
const displayIcon = computed(() => versionIcons[versionName.value] ?? LOADER_ICONS[category.value] ?? LOADER_ICONS.vanilla);
// 版本切换即懒加载该版本的持久化图标（整合包 logo / 用户自定义）
watch(versionName, name => void loadVersionIcon(name), { immediate: true });
/**
 * 修改图标：本地选图 → 后端魔数校验后复制为版本自定义图标。
 * 成功后 store 已刷新缓存，卡片立即显示新图。
 */
async function changeIcon() {
  const path = await open({
    multiple: false,
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  if (!path) return;
  try {
    await setVersionIcon(versionName.value, String(path));
    showToast("图标已更新");
  } catch (e) {
    showToast(errMsg(e));
  }
}
/** loader 版本号：目录名可解析且与最终 category 一致才显示（后端推断时目录名可能无版本号）。 */
const loaderVersion = computed(() =>
  info.value.category === category.value ? info.value.loaderVersion : "",
);
const subtitle = computed(() =>
  category.value === "vanilla"
    ? "原版"
    : loaderVersion.value || (LOADER_TITLES[category.value] ?? category.value),
);

const router = useRouter();

/** 进入版本设置页（下层页面）；返回时按历史回到主页。 */
function goSetup() {
  router.push({ path: "/version-setup", query: { name: versionName.value } });
}

/** 当前 tab：存档 | Mod | 材质包 | 数据包 | 光影包（组件内状态，路由重建回存档）。 */
type TabKind = "worlds" | "mods" | "resourcepacks" | "datapacks" | "shaderpacks";
const tab = ref<TabKind>("worlds");

const TAB_LABELS: Record<TabKind, string> = {
  worlds: "存档",
  mods: "Mod",
  resourcepacks: "材质包",
  datapacks: "数据包",
  shaderpacks: "光影包",
};

/** 各资源类型的侧边导航图标。 */
const SIDE_ICONS = {
  worlds: Map,
  mods: Puzzle,
  resourcepacks: Palette,
  datapacks: Database,
  shaderpacks: Sun,
} as const;

/**
 * 可见 Tab 列表（按实际存在的资源展示）：
 * - 存档始终显示（不管是否为空）
 * - Mod 仅 ModLoader 版本且 mods 目录有文件时显示
 * - 材质包/数据包/光影包仅目录非空时显示
 */
const tabs = computed<TabKind[]>(() => {
  const list: TabKind[] = ["worlds"];
  if (isModLoaderVersion.value && modsQuick.value.length > 0) list.push("mods");
  for (const k of ["resourcepacks", "datapacks", "shaderpacks"] as const) {
    if (resourceRefs(k).quick.value.length > 0) list.push(k);
  }
  return list;
});

// 资源列表变化（加载完成/删除后）时，若当前 tab 已不可见则回退存档。
watch(tabs, t => {
  if (!t.includes(tab.value)) tab.value = "worlds";
});

// 存档列表就绪后懒加载各世界缩略图（本地读取，缺失保持 Map 占位）。
watch(worlds, list => {
  for (const w of list) void loadWorldIcon(versionName.value, w.name);
});

/** 当前 tab 对应的"打开文件夹"参数与 tooltip 文案。 */
const folderMeta = computed(() => {
  switch (tab.value) {
    case "mods":
      return { kind: "mods" as const, label: "打开 Mod" };
    case "resourcepacks":
      return { kind: "resourcepacks" as const, label: "打开材质包" };
    case "datapacks":
      return { kind: "datapacks" as const, label: "打开数据包" };
    case "shaderpacks":
      return { kind: "shaderpacks" as const, label: "打开光影包" };
    default:
      return { kind: "saves" as const, label: "打开存档" };
  }
});

// ── 资源目录（材质包/数据包/光影包）通用列表数据 ──
// 状态选择下沉 store 的 resourceRefs：此处按 tab 解包；非资源 tab 给空态兜底。
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

/** 各资源类型的空态图标与文案。 */
const RES_ICONS = { resourcepacks: Palette, datapacks: Database, shaderpacks: Sun } as const;
const RES_EMPTY_TEXTS: Record<ResourceKind, string> = {
  resourcepacks: "还没有材质包\n可将 zip 包放入 resourcepacks 目录",
  datapacks: "还没有数据包\n可将 zip 包放入 datapacks 目录",
  shaderpacks: "还没有光影包\n可将 zip 包放入 shaderpacks 目录",
};
const resIcon = computed(() => (isResourceTab(tab.value) ? RES_ICONS[tab.value] : Palette));
const resEmptyText = computed(() => (isResourceTab(tab.value) ? RES_EMPTY_TEXTS[tab.value] : ""));

// ── 版本切换：重置 + 加载 loader/存档/Mod/资源目录 ──
watch(
  versionName,
  nv => {
    if (!nv) return;
    resetVersionDetail();
    loadVersionLoader(nv);
    loadWorlds(nv);
    loadModsQuick(nv);
    loadMods(nv);
    loadResourceLists(nv);
  },
  { immediate: true },
);

// ── 打开文件夹（目录不存在先创建；后端 ensure_game_folder 支持 version/saves/mods/资源目录）──
async function openFolder(kind: "saves" | "version" | "mods" | "resourcepacks" | "datapacks" | "shaderpacks") {
  try {
    const target = await invoke<string>("ensure_game_folder", {
      kind,
      versionName: versionName.value,
    });
    await invoke("open_folder", { path: target });
  } catch (e) {
    showToast(errMsg(e));
  }
}

// ── 修复：扫描缺失 → 提交任务中心（检测期间按钮显示环形进度 + "检测中…"）──
const checking = ref(false);

async function repairFiles() {
  if (checking.value || isLaunching.value) return;
  checking.value = true;
  try {
    const report = await invoke<{
      client_jar: boolean;
      libraries: number;
      assets: number;
      natives: number;
    }>("scan_missing_files", { versionDirName: versionName.value });
    const total =
      (report.client_jar ? 1 : 0) + report.libraries + report.assets + report.natives;
    if (total === 0) {
      showToast("游戏文件完整，无需补全");
      return;
    }
    const taskId = await invoke<string>("submit_missing_download", {
      versionDirName: versionName.value,
    });
    // 预注册任务：任务中心徽标立即出现（首次 task-progress 事件到达前也能看到）
    dlState.addTask({
      taskId,
      name: `${versionName.value} 版本修复`,
      status: "pending",
      progress: 0,
      speedBps: 0,
      etaSeconds: null,
      completedFiles: 0,
      totalFiles: 0,
      completedBytes: 0,
      totalBytes: 0,
      currentPhase: "等待中",
      steps: [],
    });
    showToast(`已提交 ${total} 个缺失文件到任务中心`);
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
    renameError.value = "名称不能为空";
    return;
  }
  if (newName === versionName.value) {
    renameError.value = "新名称与当前名称相同";
    return;
  }
  try {
    await invoke("rename_version_dir", {
      versionName: versionName.value,
      newName,
    });
    renameDialog.value = false;
    // 保持选中新名：卡片 watch 自动刷新内容
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
          <button class="vdc-icon-btn" v-tooltip="'版本设置'" @click="goSetup">
            <Settings2 :size="15" />
          </button>
          <button class="vdc-icon-btn" v-tooltip="'修改图标'" @click="changeIcon">
            <ImagePlus :size="15" />
          </button>
          <button class="vdc-icon-btn" v-tooltip="'重命名版本'" @click="openRename">
            <Pencil :size="15" />
          </button>
        </template>
      </VersionItem>
    </div>

    <!-- 中间层：侧边资源导航 + 列表区 -->
    <div class="vdc-main">
      <nav class="vdc-side">
        <div v-if="tabs.length > 1" class="vdc-side-nav">
          <button
            v-for="t in tabs"
            :key="t"
            type="button"
            class="vdc-side-btn"
            :class="{ active: tab === t }"
            v-tooltip="TAB_LABELS[t]"
            @click="tab = t"
          >
            <component :is="SIDE_ICONS[t]" :size="17" />
          </button>
        </div>
        <button
          type="button"
          class="vdc-side-open"
          v-tooltip="folderMeta.label"
          @click="openFolder(folderMeta.kind)"
        >
          <FolderOpen :size="15" />
        </button>
      </nav>
      <div class="vdc-list-area">
        <Transition name="vdc-fade" mode="out-in">
      <!-- ── 存档 tab ── -->
      <div v-if="tab === 'worlds'" key="worlds" class="vdc-pane">
        <LoadingSpinner v-if="worldsLoading" text="加载存档…" />
        <ErrorState v-else-if="worldsError" :text="worldsError" />
        <EmptyState v-else-if="worlds.length === 0">
          <Map :size="40" :stroke-width="1.5" opacity="0.5" />
          <p>还没有存档<br />启动游戏后会自动创建</p>
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
              <span class="vdc-row-meta">最后游玩 {{ fmtUnix(w.last_modified) }}</span>
            </div>
          </div>
        </div>
        </div>

      <!-- ── Mod tab ── -->
      <div v-else-if="tab === 'mods'" key="mods" class="vdc-pane">
        <LoadingSpinner v-if="modsLoading && mods.length === 0" text="正在加载 Mod 信息…" />
        <ErrorState v-else-if="modsError" :text="modsError" />
        <EmptyState v-else-if="mods.length === 0">
          <Puzzle :size="40" :stroke-width="1.5" opacity="0.5" />
          <p v-if="isModLoaderVersion">mods 目录为空<br />可从资源中心下载 Mod</p>
          <p v-else>mods 目录为空</p>
        </EmptyState>
        <template v-else>
          <div v-if="modsNetworkError" class="vdc-net-warn">
            部分 Mod 信息获取失败，已显示本地信息
          </div>
          <div class="vdc-mod-list">
            <template v-for="m in mods" :key="m.file_name">
              <!-- 识别得到：Modrinth 元数据详情样式 -->
              <div v-if="m.project_id" class="vdc-mod">
                <IconImage :src="m.icon_url" :size="32" />
                <span class="vdc-mod-info">
                  <span class="vdc-mod-title-row">
                    <span class="vdc-mod-title">{{ m.title_zh ?? m.title ?? m.file_name }}</span>
                    <span v-if="m.title_zh" class="vdc-mod-file">{{ m.file_name }}</span>
                    <span v-if="m.downloads" class="vdc-mod-sub">{{ fmtCount(m.downloads) }} 次下载</span>
                  </span>
                  <span v-if="m.description" class="vdc-mod-desc">
                    {{ dlTranslateMode === "en" ? m.description : (m.description_zh ?? m.description) }}
                  </span>
                  <span v-if="m.current_version" class="vdc-mod-support">
                    <span class="vdc-mod-ver">v{{ m.current_version }}</span>
                  </span>
                </span>
              </div>
              <!-- 识别不到：本地文件样式 -->
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
        <LoadingSpinner v-if="res.loading && res.list.length === 0" text="加载中…" />
        <ErrorState v-else-if="res.error" :text="res.error" />
        <EmptyState v-else-if="res.list.length === 0">
          <component :is="resIcon" :size="40" :stroke-width="1.5" opacity="0.5" />
          <p>{{ resEmptyText }}</p>
        </EmptyState>
        <template v-else>
          <div v-if="res.networkError" class="vdc-net-warn">
            部分资源信息获取失败，已显示本地信息
          </div>
          <div class="vdc-mod-list">
            <template v-for="f in res.list" :key="f.file_name">
              <!-- 匹配到社区项目：图标 + 标题 + 描述 + 下载量 -->
              <div v-if="f.project_id" class="vdc-mod">
                <IconImage :src="f.icon_url" :size="32" />
                <span class="vdc-mod-info">
                  <span class="vdc-mod-title-row">
                    <span class="vdc-mod-title">{{ f.title_zh ?? f.title ?? f.file_name }}</span>
                    <span v-if="f.title_zh" class="vdc-mod-file">{{ f.file_name }}</span>
                    <span v-if="f.downloads" class="vdc-mod-sub">{{ fmtCount(f.downloads) }} 次下载</span>
                  </span>
                  <span v-if="f.description" class="vdc-mod-desc">
                    {{ dlTranslateMode === "en" ? f.description : (f.description_zh ?? f.description) }}
                  </span>
                  <span v-if="f.current_version" class="vdc-mod-support">
                    <span class="vdc-mod-ver">v{{ f.current_version }}</span>
                  </span>
                </span>
              </div>
              <!-- 未匹配：本地文件 + 通用类型图标 -->
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
      <button class="vdc-foot-btn" @click="openFolder('version')">
        <Folder :size="14" />
        <span>游戏文件夹</span>
      </button>
      <button class="vdc-foot-btn" :disabled="checking || isLaunching" :title="isLaunching ? '游戏启动中' : ''" @click="repairFiles">
        <span v-if="checking" class="vdc-repair-spinner"></span>
        <CloudDownload v-else :size="14" />
        <span>{{ checking ? "检测中…" : "修复" }}</span>
      </button>
    </div>

    <!-- 重命名版本 dialog -->
    <AlertDialog v-model="renameDialog" title="重命名版本">
      <div class="vdc-dialog-body">
        <input
          v-model="renameInput"
          class="vdc-input"
          type="text"
          maxlength="64"
          @keydown.enter="confirmRename"
        />
        <div v-if="renameError" class="vdc-err">{{ renameError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmRename">确定</button>
        <button class="dialog-btn" @click="renameDialog = false">取消</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vdc {
  /* 圆角卡片占满右区：宽高 100%，内容由 flex column 分布（右区本身宽>高） */
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

/* 打开当前资源目录：侧边栏底部常驻，位置不随导航数量变化 */
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

/* 空态：图标 + 换行文本，内容区完全居中 */
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

/* 世界行：真实缩略图（icon.png）或 Map 占位；缩略图圆角卡片 */
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

/* ===== Mod 列表（社区资源风格：图标 + 标题/文件名 + 描述 + 版本支持）===== */
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

/* 修复按钮检测态：环形进度条（旋转环，复用 vdc-spin keyframes） */
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
