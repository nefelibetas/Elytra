<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { useRoute, useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { classifyVersion } from "../utils/versionClassify";
import { showToast } from "../utils/toast";
import { CATEGORY_ICONS } from "../utils/versionIcons";
import VersionItem from "../component/VersionItem.vue";
import AlertDialog from "../component/AlertDialog.vue";
import { dlState } from "../store/download";

const route = useRoute();
const router = useRouter();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));
const info = computed(() => classifyVersion(versionName.value));
const icon = computed(() => CATEGORY_ICONS[info.value.category] ?? CATEGORY_ICONS.vanilla);

type TabKey = "overview" | "settings" | "mods" | "export";

const tabs: { key: TabKey; label: string; icon: string }[] = [
  { key: "overview", label: "概览", icon: "info" },
  { key: "settings", label: "设置", icon: "settings" },
  { key: "mods", label: "Mod 管理", icon: "puzzle" },
  { key: "export", label: "导出", icon: "export" },
];

const activeTab = ref<TabKey>("overview");

/** 当前激活游戏目录（用于拼接文件夹路径）。 */
const gameDir = ref("");
const dirError = ref("");

onMounted(async () => {
  try {
    const d = await invoke<{ name: string; path: string }>("get_active_game_dir");
    gameDir.value = d.path;
  } catch (e) {
    dirError.value = String(e);
  }
});

/** 打开版本对应文件夹（目录不存在则先创建，失败 toast 反馈）。 */
async function openFolder(kind: "version" | "saves" | "mods") {
  try {
    const target = await invoke<string>("ensure_game_folder", {
      kind,
      versionName: versionName.value,
    });
    await invoke("open_folder", { path: target });
  } catch (e) {
    showToast(String(e));
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
    // 重命名后回到版本选择页并刷新列表
    router.push({ path: "/version-select", query: { refresh: String(Date.now()) } });
  } catch (e) {
    renameError.value = String(e);
  }
}

// ── 删除版本 dialog ──
const deleteDialog = ref(false);

async function confirmDelete() {
  try {
    await invoke("delete_version_dir", { versionName: versionName.value });
    deleteDialog.value = false;
    router.push({ path: "/version-select", query: { refresh: String(Date.now()) } });
  } catch (e) {
    showToast(String(e));
    deleteDialog.value = false;
  }
}

/** 补全文件：先提示检测，扫描缺失 → 提交到下载管理。 */
async function repairFiles() {
  showToast("正在检测游戏完整性…");
  try {
    const report = await invoke<{
      client_jar: boolean;
      libraries: number;
      assets: number;
    }>("scan_missing_files", { versionDirName: versionName.value });
    const total =
      (report.client_jar ? 1 : 0) + report.libraries + report.assets;
    if (total === 0) {
      showToast("游戏文件完整，无需补全");
      return;
    }
    const taskId = await invoke<string>("submit_missing_download", {
      versionDirName: versionName.value,
    });
    // 预注册任务：FAB 立即出现（首次 task-progress 事件到达前也能看到）
    dlState.addTask({
      taskId,
      name: `${versionName.value} 版本修复`,
      status: "pending",
      progress: 0,
      speedBps: 0,
      completedFiles: 0,
      totalFiles: 0,
      currentPhase: "等待中",
      steps: [],
      previousRoute: router.currentRoute.value.fullPath,
    });
    showToast(`已提交 ${total} 个缺失文件到下载管理`);
  } catch (e) {
    showToast(String(e));
  }
}

/** 导出启动脚本：保存对话框选路径 → 后端按 PCL 风格生成平台对应脚本。 */
async function exportScript() {
  try {
    const isWin = /windows/i.test(navigator.userAgent);
    const ext = isWin ? "bat" : "sh";
    const path = await save({
      defaultPath: `启动 ${versionName.value}.${ext}`,
      filters: [{ name: "启动脚本", extensions: [ext] }],
    });
    if (!path) return; // 用户取消

    // 收集当前登录态，生成可直接启动的命令
    const mode = localStorage.getItem("elytra.authMode") ?? "offline";
    let username = "";
    let accessToken = "";
    let uuid = "";
    let xuid = "";
    if (mode === "online") {
      const acc = await invoke<{
        name: string;
        access_token: string;
        uuid: string;
        xuid: string;
      } | null>("get_ms_account");
      if (!acc) {
        showToast("未登录正版账号，请先在启动页登录");
        return;
      }
      username = acc.name;
      accessToken = acc.access_token;
      uuid = acc.uuid;
      xuid = acc.xuid;
    } else {
      username =
        (await invoke<string>("get_last_offline_username")) || "Steve";
    }

    await invoke("export_launch_script", {
      savePath: path,
      versionDirName: versionName.value,
      authMode: mode,
      username,
      accessToken,
      uuid,
      xuid,
    });
    showToast("启动脚本已导出");
  } catch (e) {
    showToast(String(e));
  }
}
</script>

<template>
  <div class="vs">
    <!-- 左侧导航（标题栏由窗口 TitleBar 提供） -->
    <div class="vs-left">
      <div class="vs-nav-list">
        <div
          v-for="t in tabs"
          :key="t.key"
          class="vs-nav-item"
          :class="{ active: activeTab === t.key }"
          @click="activeTab = t.key"
        >
          <svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor">
            <path v-if="t.icon === 'info'" d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 3.4a1.1 1.1 0 110 2.2 1.1 1.1 0 010-2.2zm1 7.8H7V6.8h2v5.4z" />
            <g v-if="t.icon === 'settings'" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" transform="translate(4 4) scale(0.667)">
              <line x1="21" y1="4" x2="14" y2="4"/>
              <line x1="10" y1="4" x2="3" y2="4"/>
              <line x1="21" y1="12" x2="12" y2="12"/>
              <line x1="8" y1="12" x2="3" y2="12"/>
              <line x1="21" y1="20" x2="16" y2="20"/>
              <line x1="12" y1="20" x2="3" y2="20"/>
              <line x1="14" y1="2" x2="14" y2="6"/>
              <line x1="8" y1="10" x2="8" y2="14"/>
              <line x1="16" y1="18" x2="16" y2="22"/>
            </g>
            <path v-if="t.icon === 'puzzle'" d="M9.5 1h1.4a1.4 1.4 0 011.4 1.4v1.4h.7a1.5 1.5 0 010 3h-.7v3.3a1.4 1.4 0 01-1.4 1.4H7.6v-.7a1.5 1.5 0 00-3 0v.7H2.9a1.4 1.4 0 01-1.4-1.4V9.9h.7a1.5 1.5 0 000-3h-.7V2.4A1.4 1.4 0 012.9 1h2.4v.7a1.5 1.5 0 003 0V1z" />
            <path v-if="t.icon === 'export'" d="M8 1.5a1 1 0 01.7.3l3.5 3.5a1 1 0 01-1.4 1.4L9 4.4V11a1 1 0 01-2 0V4.4L5.2 6.7a1 1 0 01-1.4-1.4l3.5-3.5a1 1 0 01.7-.3zM2 12.5a1 1 0 011 1V14h10v-.5a1 1 0 112 0V15a1 1 0 01-1 1H2a1 1 0 01-1-1v-1.5a1 1 0 011-1z" />
          </svg>
          <span>{{ t.label }}</span>
        </div>
      </div>
    </div>

    <!-- 右侧：当前 tab 内容 -->
    <div class="vs-right">
      <!-- 概览 -->
      <div v-if="activeTab === 'overview'" class="vs-page">
        <div v-if="dirError" class="vs-err">{{ dirError }}</div>

        <!-- 版本信息卡片 -->
        <div class="setup-card">
          <VersionItem
            :icon-src="icon"
            :version-id="versionName"
            :subtitle="info.category === 'vanilla' ? '原版' : info.categoryTitle + ' ' + info.loaderVersion"
          >
            <template #action>
              <button class="setup-icon-btn" title="重命名版本" @click="openRename">
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M17 3a2.8 2.8 0 114 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
                </svg>
              </button>
            </template>
          </VersionItem>
        </div>

        <!-- 快捷方式卡片 -->
        <div class="setup-card">
          <div class="setup-card-title">快捷方式</div>
          <div class="setup-btn-row">
            <button class="setup-btn" @click="openFolder('version')">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
              </svg>
              <span>版本文件夹</span>
            </button>
            <button class="setup-btn" @click="openFolder('saves')">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M19 21H5a2 2 0 01-2-2V5a2 2 0 012-2h11l5 5v11a2 2 0 01-2 2z" />
                <path d="M17 21v-8H7v8M7 3v5h8" />
              </svg>
              <span>存档文件夹</span>
            </button>
            <button class="setup-btn" @click="openFolder('mods')">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M10 13a5 5 0 007.5.5l3-3a5 5 0 00-7-7l-1.7 1.7" />
                <path d="M14 11a5 5 0 00-7.5-.5l-3 3a5 5 0 007 7l1.7-1.7" />
              </svg>
              <span>Mod 文件夹</span>
            </button>
          </div>
        </div>

        <!-- 高级管理卡片 -->
        <div class="setup-card">
          <div class="setup-card-title">高级管理</div>
          <div class="setup-btn-row">
            <button class="setup-btn" @click="exportScript">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="16 18 22 12 16 6" />
                <polyline points="8 6 2 12 8 18" />
              </svg>
              <span>导出启动脚本</span>
            </button>
            <button class="setup-btn" @click="repairFiles">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M20 14.7A7 7 0 0012 3a7 7 0 00-6.9 5.4A5 5 0 005 18h11a4 4 0 004-3.3z" />
                <path d="M12 12v5M9.5 14.5L12 17l2.5-2.5" />
              </svg>
              <span>补全文件</span>
            </button>
            <button class="setup-btn danger" @click="deleteDialog = true">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6" />
                <path d="M10 11v6M14 11v6" />
                <path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2" />
              </svg>
              <span>删除版本</span>
            </button>
          </div>
        </div>
      </div>

      <!-- 设置 -->
      <div v-else-if="activeTab === 'settings'" class="vs-page">
        <div class="vs-hint">版本设置功能开发中</div>
      </div>

      <!-- Mod 管理 -->
      <div v-else-if="activeTab === 'mods'" class="vs-page">
        <div class="vs-hint">Mod 管理功能开发中</div>
      </div>

      <!-- 导出 -->
      <div v-else class="vs-page">
        <div class="vs-hint">导出功能开发中</div>
      </div>
    </div>

    <!-- 重命名版本 dialog -->
    <AlertDialog v-model="renameDialog" title="重命名版本" :divider="false">
      <div class="setup-dialog-body">
        <input
          v-model="renameInput"
          class="vs-input"
          type="text"
          maxlength="64"
          @keydown.enter="confirmRename"
        />
        <div v-if="renameError" class="vs-err">{{ renameError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmRename">确定</button>
        <button class="dialog-btn" @click="renameDialog = false">取消</button>
      </template>
    </AlertDialog>

    <!-- 删除版本确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除版本" :divider="false">
      <div class="setup-dialog-body">
        <div class="setup-dialog-text">
          确定要删除版本「{{ versionName }}」吗？该操作会删除版本文件夹及其全部文件，且无法恢复。
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn danger" @click="confirmDelete">删除</button>
        <button class="dialog-btn" @click="deleteDialog = false">取消</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vs {
  display: flex;
  height: 100%;
}

/* ===== 左侧导航 ===== */
.vs-left {
  flex: 0 0 240px;
  min-width: 220px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
}

.vs-nav-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.vs-nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.12s;
  user-select: none;
}

.vs-nav-item:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.vs-nav-item.active {
  background: var(--color-brand-bg);
  color: var(--color-brand);
  font-weight: 600;
}

/* ===== 右侧 ===== */
.vs-right {
  flex: 1;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface-page);
}

.vs-hint {
  padding: 24px;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}

.vs-err {
  font-size: 12px;
  color: #e74c3c;
  padding: 6px 8px;
}

/* 卡片 */
.setup-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  max-width: 560px;
}

.setup-card + .setup-card {
  margin-top: 12px;
}

.setup-card :deep(.ver-item) {
  border-radius: var(--radius-md);
}

.setup-card-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  padding: 12px 16px 4px;
  letter-spacing: 0.5px;
}

.setup-btn-row {
  display: flex;
  gap: 8px;
  padding: 8px 16px 14px;
  flex-wrap: wrap;
}

.setup-btn {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  cursor: pointer;
  transition: all 0.15s;
}

.setup-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.setup-btn.danger {
  color: #e74c3c;
  border-color: rgba(231, 76, 60, 0.4);
}

.setup-btn.danger:hover {
  background: rgba(231, 76, 60, 0.08);
  border-color: #e74c3c;
  color: #e74c3c;
}

/* hover 重命名按钮 */
.setup-icon-btn {
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

.setup-icon-btn:hover {
  background-color: var(--color-brand-bg);
  color: var(--color-brand);
}

/* dialog */
.setup-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.setup-dialog-text {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
}

.vs-input {
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

.vs-input:focus {
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

.dialog-btn.danger {
  background: #e74c3c;
  border-color: #e74c3c;
  color: #fff;
}

.dialog-btn.danger:hover {
  opacity: 0.88;
}
</style>
