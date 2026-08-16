<script setup lang="ts">
// 版本设置-概览 tab：版本信息 + 快捷方式 + 高级管理（补全/导出/删除）+ 重命名/删除 dialog
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Pencil, Folder, Save, Puzzle, CodeXml, CloudDownload, Trash2 } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { classifyVersion } from "../utils/versionClassify";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { CATEGORY_ICONS } from "../utils/versionIcons";
import VersionItem from "./VersionItem.vue";
import AlertDialog from "./AlertDialog.vue";
import { dlState } from "../store/download";
import { authMode, msAccount, loadMsAccount, username as offlineUsername } from "../store/auth";
import { selectedVersion, setSelected, clearSelected } from "../store/selectedVersion";

const route = useRoute();
const router = useRouter();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));
const info = computed(() => classifyVersion(versionName.value));
const icon = computed(() => CATEGORY_ICONS[info.value.category] ?? CATEGORY_ICONS.vanilla);

/** 当前激活游戏目录（用于拼接文件夹路径）。 */
const gameDir = ref("");
const dirError = ref("");

onMounted(async () => {
  try {
    const d = await invoke<{ name: string; path: string }>("get_active_game_dir");
    gameDir.value = d.path;
  } catch (e) {
    dirError.value = errMsg(e);
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
    // 重命名后保持选中（启动页按钮仍指向该版本新名）
    setSelected(newName);
    // 回到版本选择页并刷新列表（进入页面 onMounted 自动刷新）
    router.push({ path: "/version-select" });
  } catch (e) {
    renameError.value = errMsg(e);
  }
}

// ── 删除版本 dialog ──
const deleteDialog = ref(false);

async function confirmDelete() {
  // 先关 dialog 并提示删除中（delete_version_dir 递归删除可能耗时数秒）
  deleteDialog.value = false;
  showToast("正在删除版本…");
  try {
    await invoke("delete_version_dir", { versionName: versionName.value });
    showToast(`版本「${versionName.value}」已经删除`);
    // 删除的是当前选中版本 → 清理持久化选择（启动页按钮回到"下载游戏"入口）
    if (selectedVersion.value === versionName.value) {
      clearSelected();
    }
    router.push({ path: "/version-select" });
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 补全文件：先提示检测，扫描缺失 → 提交到任务中心。 */
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
      (report.client_jar ? 1 : 0) +
      report.libraries +
      report.assets +
      report.natives;
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
    // 注意：auth store 的 AuthMode 是 "premium" | "offline"，后端命令参数约定是 "online" | "offline"
    let username = "";
    let accessToken = "";
    let uuid = "";
    let xuid = "";
    if (authMode.value === "premium") {
      if (!msAccount.value) await loadMsAccount();
      const acc = msAccount.value;
      if (!acc) {
        showToast("未登录正版账号，请先在启动页登录");
        return;
      }
      username = acc.name;
      accessToken = acc.access_token;
      uuid = acc.uuid;
      xuid = acc.xuid;
    } else {
      // 优先当前输入框用户名（未启动过也正确），回退上次保存，最后 Steve
      username =
        offlineUsername.value.trim() ||
        (await invoke<string>("get_last_offline_username")) ||
        "Steve";
    }

    await invoke("export_launch_script", {
      savePath: path,
      versionDirName: versionName.value,
      authMode: authMode.value === "premium" ? "online" : "offline",
      username,
      accessToken,
      uuid,
      xuid,
    });
    showToast("启动脚本已导出");
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vs-page">
    <div v-if="dirError" class="vs-err">{{ dirError }}</div>

    <!-- 版本信息卡片 -->
    <div class="setup-card">
      <VersionItem
        :icon-src="icon"
        :version-id="versionName"
        :subtitle="info.category === 'vanilla' ? '原版' : info.categoryTitle + ' ' + info.loaderVersion"
      >
        <template #action>
          <button class="setup-icon-btn" v-tooltip="'重命名版本'" @click="openRename">
            <Pencil :size="15" />
          </button>
        </template>
      </VersionItem>
    </div>

    <!-- 快捷方式卡片 -->
    <div class="setup-card">
      <div class="setup-card-title">快捷方式</div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="openFolder('version')">
          <Folder :size="15" />
          <span>版本文件夹</span>
        </button>
        <button class="setup-btn" @click="openFolder('saves')">
          <Save :size="15" />
          <span>存档文件夹</span>
        </button>
        <button class="setup-btn" @click="openFolder('mods')">
          <Puzzle :size="15" />
          <span>Mod 文件夹</span>
        </button>
      </div>
    </div>

    <!-- 高级管理卡片 -->
    <div class="setup-card">
      <div class="setup-card-title">高级管理</div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="exportScript">
          <CodeXml :size="15" />
          <span>导出启动脚本</span>
        </button>
        <button class="setup-btn" @click="repairFiles">
          <CloudDownload :size="15" />
          <span>补全文件</span>
        </button>
        <button class="setup-btn danger" @click="deleteDialog = true">
          <Trash2 :size="15" />
          <span>删除版本</span>
        </button>
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
.vs-err {
  font-size: 12px;
  color: var(--color-danger);
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
  color: var(--color-danger);
  border-color: rgba(231, 76, 60, 0.4);
}

.setup-btn.danger:hover {
  background: rgba(231, 76, 60, 0.08);
  border-color: var(--color-danger);
  color: var(--color-danger);
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
  background: var(--color-danger);
  border-color: var(--color-danger);
  color: #fff;
}

.dialog-btn.danger:hover {
  opacity: 0.88;
}
</style>
