<script setup lang="ts">
// 主页右侧版本卡片：单卡片，Tab 切换存档 / Mod 内容 + 打开存档按钮 + 底部（游戏文件夹 / 修复）。
// 业务在 store/versionDetail；重命名/修复/打开文件夹为本地编排。
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Pencil, Save, Folder, CloudDownload, Puzzle, Map, Settings2 } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { classifyVersion } from "../utils/versionClassify";
import { LOADER_ICONS } from "../utils/versionMeta";
import { fmtCount, fmtSize } from "../utils/format";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import VersionItem from "./VersionItem.vue";
import AlertDialog from "./AlertDialog.vue";
import IconImage from "./IconImage.vue";
import { dlState } from "../store/download";
import { dlTranslateMode } from "../store/downloaderConfig";
import { selectedVersion, setSelected } from "../store/selectedVersion";
import {
  worlds, worldsLoading, worldsError,
  mods, modsLoading, modsError, modsNetworkError,
  isModLoaderVersion,
  resetVersionDetail, loadVersionLoader, loadWorlds, loadMods,
} from "../store/versionDetail";

const versionName = computed(() => selectedVersion.value);
const info = computed(() => classifyVersion(versionName.value));
const icon = computed(() => LOADER_ICONS[info.value.category] ?? LOADER_ICONS.vanilla);
const subtitle = computed(() =>
  info.value.category === "vanilla" ? "原版" : `${info.value.categoryTitle} ${info.value.loaderVersion}`,
);

const router = useRouter();

/** 进入版本设置页（下层页面）；返回时按历史回到主页。 */
function goSetup() {
  router.push({ path: "/version-setup", query: { name: versionName.value } });
}

/** 当前 tab：存档 | Mod（组件内状态，路由重建回存档）。 */
const tab = ref<"worlds" | "mods">("worlds");

// ── 版本切换：重置 + 加载 loader/存档/Mod ──
watch(
  versionName,
  nv => {
    if (!nv) return;
    resetVersionDetail();
    loadVersionLoader(nv);
    loadWorlds(nv);
    loadMods(nv);
  },
  { immediate: true },
);

// ── 打开文件夹（目录不存在先创建；后端 ensure_game_folder 支持 version/saves/mods）──
async function openFolder(kind: "saves" | "version" | "mods") {
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

// ── 修复：扫描缺失 → 提交任务中心 ──
async function repairFiles() {
  showToast("正在检测游戏完整性…");
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

/** unix 秒 → "yyyy-MM-dd HH:mm" */
function fmtUnix(sec: number): string {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
</script>

<template>
  <div class="vdc">
    <!-- 头部：版本信息 + 版本设置 / 重命名 -->
    <div class="vdc-head">
      <VersionItem :icon-src="icon" :version-id="versionName" :subtitle="subtitle">
        <template #action>
          <button class="vdc-icon-btn" v-tooltip="'版本设置'" @click="goSetup">
            <Settings2 :size="15" />
          </button>
          <button class="vdc-icon-btn" v-tooltip="'重命名版本'" @click="openRename">
            <Pencil :size="15" />
          </button>
        </template>
      </VersionItem>
    </div>

    <!-- Tab 行 + 上下文操作（存档 → 打开存档 / Mod → 打开 Mod） -->
    <div class="vdc-tabbar">
      <div class="vdc-tabs">
        <div
          class="vdc-tab"
          :class="{ active: tab === 'worlds' }"
          @click="tab = 'worlds'"
        >存档</div>
        <div
          class="vdc-tab"
          :class="{ active: tab === 'mods' }"
          @click="tab = 'mods'"
        >Mod</div>
      </div>
      <button class="vdc-tab-btn" @click="tab === 'worlds' ? openFolder('saves') : openFolder('mods')">
        <Save v-if="tab === 'worlds'" :size="14" />
        <Folder v-else :size="14" />
        <span>{{ tab === "worlds" ? "打开存档" : "打开 Mod" }}</span>
      </button>
    </div>

    <!-- 内容区 -->
    <div class="vdc-body">
      <!-- ── 存档 tab ── -->
      <template v-if="tab === 'worlds'">
        <div v-if="worldsLoading" class="vdc-hint">加载存档…</div>
        <div v-else-if="worldsError" class="vdc-err">{{ worldsError }}</div>
        <div v-else-if="worlds.length === 0" class="vdc-empty">
          <Map :size="40" opacity="0.3" />
          <p>还没有存档<br />启动游戏后会自动创建</p>
        </div>
        <div v-else class="vdc-list">
          <div v-for="w in worlds" :key="w.name" class="vdc-row">
            <Map :size="16" class="vdc-row-icon" />
            <div class="vdc-row-info">
              <span class="vdc-row-name">{{ w.name }}</span>
              <span class="vdc-row-meta">最后游玩 {{ fmtUnix(w.last_modified) }}</span>
            </div>
          </div>
        </div>
      </template>

      <!-- ── Mod tab ── -->
      <template v-else>
        <div v-if="modsLoading" class="vdc-loading">
          <div class="spinner"></div>
          <p class="vdc-loading-text">正在加载 Mod 信息…</p>
        </div>
        <div v-else-if="modsError" class="vdc-err">{{ modsError }}</div>
        <div v-else-if="mods.length === 0" class="vdc-empty">
          <Puzzle :size="40" opacity="0.3" />
          <p v-if="isModLoaderVersion">mods 目录为空<br />可从资源中心下载 Mod</p>
          <p v-else>mods 目录为空</p>
        </div>
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
      </template>
    </div>

    <!-- 底部操作 -->
    <div class="vdc-foot">
      <button class="vdc-foot-btn" @click="openFolder('version')">
        <Folder :size="14" />
        <span>游戏文件夹</span>
      </button>
      <button class="vdc-foot-btn" @click="repairFiles">
        <CloudDownload :size="14" />
        <span>修复</span>
      </button>
    </div>

    <!-- 重命名版本 dialog -->
    <AlertDialog v-model="renameDialog" title="重命名版本" :divider="false">
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

/* ===== Tab 行 ===== */
.vdc-tabbar {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  border-bottom: 1px solid var(--border-default);
  background: var(--surface-elevated);
}

.vdc-tabs {
  display: flex;
  gap: 2px;
}

.vdc-tab {
  padding: 10px 14px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
  position: relative;
  transition: color 0.15s;
}

.vdc-tab:hover {
  color: var(--text-primary);
}

.vdc-tab.active {
  color: var(--color-brand);
  font-weight: 600;
}

.vdc-tab.active::after {
  content: "";
  position: absolute;
  left: 14px;
  right: 14px;
  bottom: 0;
  height: 2px;
  border-radius: 1px;
  background: var(--color-brand);
}

.vdc-tab-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.vdc-tab-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.vdc-tab-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.vdc-tab-btn .spin {
  animation: vdc-spin 0.9s linear infinite;
}

@keyframes vdc-spin {
  to {
    transform: rotate(360deg);
  }
}

/* ===== 内容区 ===== */
.vdc-body {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  min-height: 0;
}

.vdc-hint {
  padding: 36px 20px;
  text-align: center;
  font-size: 12.5px;
  color: var(--text-tertiary);
  user-select: none;
}

/* 空态：图标 + 换行文本，内容区完全居中 */
.vdc-empty {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  padding: 20px;
  color: var(--text-tertiary);
  text-align: center;
  user-select: none;
}

.vdc-empty p {
  font-size: 13px;
  line-height: 1.7;
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
.vdc-loading {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  padding: 40px 0;
}

.vdc-loading-text {
  font-size: 13px;
  color: var(--text-tertiary);
}

.spinner {
  width: 36px;
  height: 36px;
  border: 3px solid var(--border-default);
  border-top-color: var(--color-brand);
  border-radius: 50%;
  animation: vdc-spin 0.7s linear infinite;
}

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

.dialog-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.dialog-btn.primary {
  background: var(--color-brand);
  border-color: var(--color-brand);
  color: #fff;
}

.dialog-btn.primary:hover {
  opacity: 0.88;
}
</style>
