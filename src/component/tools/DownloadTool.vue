<script setup lang="ts">
// 工具箱「多线程下载器」：粘贴多个链接 → 队列并发 + 单文件 HTTP Range 分段。
// 任务列表与进度完全独立于启动器下载任务（自己的 store / 事件 / 卡片布局）。
// 服务端不支持 Range（或链路吃掉 Range 头）时后端自动回退单连接。
import { onMounted, ref } from "vue";
import { Download, FolderOpen, Pause, Play, Trash2, X } from "@lucide/vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  toolDlState,
  ensureToolDlListen,
  loadToolDlTasks,
  loadToolDlConfig,
  saveToolDlConfig,
  enqueueToolDl,
  pauseToolDl,
  resumeToolDl,
  cancelToolDl,
  removeToolDl,
  clearFinishedToolDl,
  openToolDlFolder,
  toolDlEta,
} from "../../store/toolDownload";
import type { ToolDlSnapshot } from "../../store/toolDownload";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { fmtEta, fmtSize, fmtSpeed } from "../../utils/format";
import DropSelect from "../common/DropSelect.vue";
import EmptyState from "../common/EmptyState.vue";

const urlsText = ref("");
const saveDir = ref("");
const maxFiles = ref("3");
const segments = ref("8");
const busy = ref(false);

const maxFilesOptions = [
  { value: "1", label: "1", desc: "串行，逐个下载" },
  { value: "2", label: "2" },
  { value: "3", label: "3", desc: "默认" },
  { value: "5", label: "5" },
  { value: "8", label: "8", desc: "带宽充足时可提高" },
];

const segmentOptions = [
  { value: "1", label: "1", desc: "单连接，兼容性最好" },
  { value: "2", label: "2" },
  { value: "4", label: "4" },
  { value: "8", label: "8", desc: "默认" },
  { value: "16", label: "16" },
  { value: "32", label: "32", desc: "上限" },
];

onMounted(async () => {
  ensureToolDlListen();
  try {
    const [config] = await Promise.all([loadToolDlConfig(), loadToolDlTasks()]);
    saveDir.value = config.save_dir;
    maxFiles.value = String(config.max_files);
    segments.value = String(config.segments);
  } catch (e) {
    showToast(errMsg(e));
  }
});

async function persistConfig() {
  await saveToolDlConfig({
    save_dir: saveDir.value.trim(),
    max_files: Number(maxFiles.value),
    segments: Number(segments.value),
  });
}

async function handlePickDir() {
  const picked = await open({ directory: true, multiple: false });
  if (!picked || Array.isArray(picked)) return;
  saveDir.value = picked;
  try {
    await persistConfig();
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function handleStart() {
  const urls = urlsText.value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (urls.length === 0) {
    showToast("请先粘贴下载链接（每行一个）");
    return;
  }
  if (!saveDir.value.trim()) {
    showToast("请先选择保存目录");
    return;
  }
  busy.value = true;
  try {
    await enqueueToolDl(urls, saveDir.value.trim(), Number(segments.value));
    await persistConfig();
    urlsText.value = "";
    showToast(`已加入 ${urls.length} 个下载任务`);
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    busy.value = false;
  }
}

async function handlePause(task: ToolDlSnapshot) {
  try {
    await pauseToolDl(task.id);
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function handleResume(task: ToolDlSnapshot) {
  try {
    await resumeToolDl(task.id);
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function handleCancel(task: ToolDlSnapshot) {
  try {
    await cancelToolDl(task.id);
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 移除记录后重新拉全量，避免前端与后端列表不一致。 */
async function handleRemove(task: ToolDlSnapshot) {
  try {
    await removeToolDl(task.id);
    await loadToolDlTasks();
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function handleClearFinished() {
  try {
    await clearFinishedToolDl();
    await loadToolDlTasks();
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function handleOpenFolder(task: ToolDlSnapshot) {
  try {
    await openToolDlFolder(task.dest);
  } catch (e) {
    showToast(errMsg(e));
  }
}

function isFinished(task: ToolDlSnapshot): boolean {
  return task.status === "done" || task.status === "error" || task.status === "cancelled";
}

function percentOf(task: ToolDlSnapshot): number {
  if (task.total <= 0) return 0;
  return Math.min(100, Math.floor((task.downloaded / task.total) * 100));
}

function sizeText(task: ToolDlSnapshot): string {
  if (task.total > 0) return `${fmtSize(task.downloaded)} / ${fmtSize(task.total)}`;
  return task.downloaded > 0 ? fmtSize(task.downloaded) : "—";
}

function statusText(task: ToolDlSnapshot): string {
  switch (task.status) {
    case "queued":
      return "排队中";
    case "downloading":
      return "下载中";
    case "paused":
      return "已暂停";
    case "done":
      return "已完成";
    case "error":
      return "失败";
    case "cancelled":
      return "已取消";
  }
}

function modeText(task: ToolDlSnapshot): string {
  if (task.status === "queued") return "等待线程";
  // 传输发起前还没探测：ranged 未定但 segments 仍是请求值 → 显示"探测中"，
  // 避免用户看到"配了 8 线程却短暂显示单连接"（探测一完成内核就回写真实方案）
  if (!task.ranged && task.segments > 1) return "探测中…";
  return task.ranged ? `${task.segments} 线程分段` : "单连接";
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>多线程下载器</h3>
      <div class="tool-card-sub">
        粘贴链接即可多线程下载：队列内多个文件并发，单个大文件再按 HTTP Range 分段
      </div>
    </div>

    <!-- 新建任务 -->
    <div class="tdl-sec">
      <textarea
        v-model="urlsText"
        class="tdl-urls"
        rows="3"
        placeholder="每行一个下载链接，例如：&#10;https://example.com/file.jar"
        autocomplete="off"
        spellcheck="false"
      />

      <div class="tdl-row">
        <span class="tdl-label">保存目录</span>
        <input
          v-model="saveDir"
          class="tdl-input"
          placeholder="选择下载文件的保存位置"
          autocomplete="off"
        />
        <button class="tdl-icon-btn" v-tooltip="'选择目录'" @click="handlePickDir">
          <FolderOpen :size="14" />
        </button>
      </div>

      <div class="tdl-row">
        <span class="tdl-label">同时下载</span>
        <DropSelect v-model="maxFiles" :options="maxFilesOptions" width="150px" />
        <span class="tdl-label">单文件线程</span>
        <DropSelect v-model="segments" :options="segmentOptions" width="150px" />
      </div>

      <div class="tdl-actions">
        <button class="tdl-btn tdl-btn-primary" :disabled="busy" @click="handleStart">
          <Download :size="14" />
          {{ busy ? "提交中…" : "开始下载" }}
        </button>
      </div>
    </div>

    <!-- 任务列表（占满剩余高度，列表在卡内滚动） -->
    <div class="tdl-sec tdl-tasks">
      <div class="tdl-list-hdr">
        <span class="tdl-sec-title">任务（{{ toolDlState.tasks.length }}）</span>
        <span v-if="toolDlState.totalSpeed > 0" class="tdl-speed tdl-mono">
          {{ fmtSpeed(toolDlState.totalSpeed) }}
        </span>
        <span class="tdl-spacer" />
        <button
          v-if="toolDlState.finishedCount > 0"
          class="tdl-btn tdl-btn-ghost"
          @click="handleClearFinished"
        >
          <Trash2 :size="13" />
          清除已完成
        </button>
      </div>

      <div v-if="toolDlState.tasks.length === 0" class="tdl-empty">
        <EmptyState>
          <Download :size="40" :stroke-width="1.5" opacity="0.5" />
          <p>暂无下载任务</p>
        </EmptyState>
      </div>

      <TransitionGroup v-else name="tdl" tag="div" class="tdl-list">
        <div v-for="task in toolDlState.tasks" :key="task.id" class="tdl-item">
          <div class="tdl-item-hdr">
            <span class="tdl-name" :title="task.file_name">{{ task.file_name }}</span>
            <span class="tdl-chip" :class="'is-' + task.status">{{ statusText(task) }}</span>
            <span class="tdl-mode">{{ modeText(task) }}</span>
            <span class="tdl-spacer" />

            <button
              v-if="task.status === 'downloading'"
              class="tdl-icon-btn"
              v-tooltip="'暂停（保留断点）'"
              @click="handlePause(task)"
            >
              <Pause :size="13" />
            </button>
            <button
              v-if="task.status === 'paused' || task.status === 'error'"
              class="tdl-icon-btn"
              v-tooltip="'继续下载'"
              @click="handleResume(task)"
            >
              <Play :size="13" />
            </button>
            <button
              v-if="task.status === 'downloading' || task.status === 'queued'"
              class="tdl-icon-btn"
              v-tooltip="'取消并清理'"
              @click="handleCancel(task)"
            >
              <X :size="13" />
            </button>
            <button
              v-if="task.status === 'done'"
              class="tdl-icon-btn"
              v-tooltip="'打开所在目录'"
              @click="handleOpenFolder(task)"
            >
              <FolderOpen :size="13" />
            </button>
            <button
              v-if="isFinished(task)"
              class="tdl-icon-btn"
              v-tooltip="'移除记录'"
              @click="handleRemove(task)"
            >
              <Trash2 :size="13" />
            </button>
          </div>

          <div
            v-if="!isFinished(task)"
            class="tdl-bar"
          >
            <div class="tdl-bar-fill" :style="{ width: percentOf(task) + '%' }" />
          </div>

          <div class="tdl-item-sub">
            <span class="tdl-mono">{{ sizeText(task) }}</span>
            <span v-if="task.status === 'downloading' && task.speed > 0" class="tdl-mono">
              {{ fmtSpeed(task.speed) }}
            </span>
            <span v-if="toolDlEta(task) !== null" class="tdl-mono">
              剩余 {{ fmtEta(toolDlEta(task)) }}
            </span>
            <span v-if="task.total > 0" class="tdl-mono">{{ percentOf(task) }}%</span>
          </div>

          <div v-if="task.status === 'error' && task.error" class="tdl-err" v-tooltip="task.error">
            {{ errMsg(task.error) }}
          </div>
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>

<style scoped>
.tool-card {
  /* 撑满内容区：任务列表在卡内滚动，页面本身不出现外部滚动条 */
  box-sizing: border-box;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 16px 18px;
}

.tool-card-hdr {
  flex: none;
  margin-bottom: 14px;
}

.tool-card-hdr h3 {
  margin: 0 0 4px;
  font-size: 15px;
  color: var(--text-primary);
}

.tool-card-sub {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.5;
}

.tdl-sec {
  flex: none;
  padding: 12px 0;
  border-top: 1px solid var(--border-default);
}

/* 任务区：吃掉剩余高度，列表在内部滚动（页面不整体滚动） */
.tdl-tasks {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.tdl-sec-title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
}

.tdl-urls {
  width: 100%;
  box-sizing: border-box;
  /* 固定 3 行高度、禁止手动拉伸（行高与字号同步收窄，整体更紧凑） */
  resize: none;
  height: 62px;
  padding: 6px 10px;
  font-size: 12px;
  font-family: "Cascadia Mono", "Consolas", monospace;
  line-height: 1.45;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}

.tdl-urls:focus {
  border-color: var(--color-brand);
  outline: none;
}

.tdl-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 0 0;
}

.tdl-label {
  flex: 0 0 64px;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.tdl-input {
  flex: 1;
  min-width: 0;
  height: 32px;
  padding: 0 10px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}

.tdl-input:focus {
  border-color: var(--color-brand);
  outline: none;
}

.tdl-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding-top: 10px;
}

.tdl-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  font-size: 13px;
  color: var(--text-secondary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.tdl-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.tdl-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.tdl-btn-primary {
  color: var(--text-on-brand, #fff);
  background: var(--color-brand-gradient, var(--color-brand));
  border-color: transparent;
}

.tdl-btn-primary:hover:not(:disabled) {
  color: var(--text-on-brand, #fff);
  opacity: 0.9;
}

.tdl-btn-ghost {
  height: 26px;
  padding: 0 10px;
  font-size: 12px;
}

.tdl-icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  color: var(--text-tertiary);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.tdl-icon-btn:hover {
  color: var(--color-brand);
  border-color: var(--border-default);
}

.tdl-list-hdr {
  flex: none;
  display: flex;
  align-items: center;
  gap: 10px;
  padding-bottom: 8px;
}

.tdl-spacer {
  flex: 1;
}

.tdl-speed {
  font-size: 12.5px;
  color: var(--color-brand);
}

.tdl-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 4px 0;
  scrollbar-gutter: stable;
}

/* 空态在任务区剩余空间里垂直居中 */
.tdl-empty {
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.tdl-item {
  padding: 10px 12px;
  background-color: var(--surface-inset, var(--surface-hover));
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}

.tdl-item-hdr {
  display: flex;
  align-items: center;
  gap: 8px;
}

.tdl-name {
  max-width: 45%;
  overflow: hidden;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  text-overflow: ellipsis;
}

.tdl-chip {
  flex: 0 0 auto;
  padding: 1px 7px;
  font-size: 11.5px;
  border-radius: 999px;
  color: var(--text-secondary);
  background-color: var(--surface-hover);
}

.tdl-chip.is-downloading {
  color: var(--color-brand);
}

.tdl-chip.is-done {
  color: var(--color-success, var(--color-brand));
}

.tdl-chip.is-error {
  color: var(--color-error, #e5484d);
}

.tdl-mode {
  flex: 0 0 auto;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.tdl-bar {
  height: 6px;
  margin: 8px 0 6px;
  border-radius: 3px;
  background-color: var(--surface-hover);
  overflow: hidden;
}

.tdl-bar-fill {
  height: 100%;
  border-radius: 3px;
  background: var(--color-brand-gradient, var(--color-brand));
  transition: width 0.25s ease;
}

.tdl-item-sub {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.tdl-mono {
  font-variant-numeric: tabular-nums;
}

.tdl-err {
  margin-top: 6px;
  font-size: 12px;
  color: var(--color-error, #e5484d);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.tdl-enter-active,
.tdl-leave-active {
  transition: opacity 0.2s ease;
}

.tdl-enter-from,
.tdl-leave-to {
  opacity: 0;
}
</style>
