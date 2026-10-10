<script setup lang="ts">
import { ref } from "vue"
import { X, Check, LoaderCircle, RotateCcw, HardDriveDownload, Pause } from "@lucide/vue"
import { dlState, TaskItem, interruptedJobs, resumeAllInterrupted, dismissAllInterrupted } from "../store/task/download"
import { errMsg } from "../utils/error"
import { fmtSpeed, fmtEta } from "../utils/format"
import { showToast } from "../utils/toast"
import ScrollTopFab from "../component/common/ScrollTopFab.vue"

/** 批量文件任务步骤可能很多，默认折叠到前 N 个。 */
const MAX_VISIBLE_STEPS = 8
const expandedSteps = ref<Set<string>>(new Set())

function visibleSteps(task: TaskItem) {
  if (expandedSteps.value.has(task.taskId) || task.steps.length <= MAX_VISIBLE_STEPS) {
    return task.steps
  }
  return task.steps.slice(0, MAX_VISIBLE_STEPS)
}

function toggleSteps(taskId: string) {
  const s = new Set(expandedSteps.value)
  if (s.has(taskId)) s.delete(taskId)
  else s.add(taskId)
  expandedSteps.value = s
}

/** 状态徽标文案与色调。 */
function statusChip(task: TaskItem): { text: string; cls: string } {
  if (task.status === "error") return { text: "失败", cls: "err" }
  if (task.status === "pending") return { text: "等待中", cls: "pending" }
  if (task.status === "paused") return { text: "已暂停", cls: "pending" }
  return { text: "下载中", cls: "running" }
}

async function handleCancel(task: TaskItem) {
  const name = task.name || "下载任务";
  try {
    await dlState.cancelTask(task.taskId)
  } catch {
    // 任务已结束（后端无此任务）：不阻塞本地清理
  } finally {
    // 无论 cancel 成败都移除卡片：cancel 抛错（任务已终态）时卡片不得残留
    dlState.removeTask(task.taskId)
  }
  showToast(`${name} 已取消`)
  // 不自动退出任务中心：与任务完成一致（顶级 tab，停留本页查看状态），
  // 取消后列表空则显示"暂无下载任务"
}

async function handleResume(task: TaskItem) {
  try {
    await dlState.resumePausedTask(task.taskId)
    showToast(`${task.name || "下载任务"} 已继续下载`)
  } catch (e) {
    showToast(`继续失败：${errMsg(String(e))}`)
  }
}

async function handleDismissPaused(task: TaskItem) {
  await dlState.dismissPausedTask(task.taskId)
  showToast(`${task.name || "下载任务"} 已放弃`)
}
</script>

<template>
  <div class="dd">
    <!-- 顶部总览条：任务存在时展示聚合进度 -->
    <section v-if="dlState.tasks.length > 0" class="dd-overview">
      <div class="ov-main">
        <div class="ov-pct mono">{{ Math.floor(dlState.totalProgress) }}<span class="ov-unit">%</span></div>
        <div class="ov-bar">
          <div class="ov-bar-fill" :style="{ width: dlState.totalProgress + '%' }"></div>
        </div>
        <div class="ov-sub">
          {{ dlState.tasks.length }} 个任务
          <span v-if="dlState.remainingFiles > 0"> · 剩余 {{ dlState.remainingFiles }} 个文件</span>
        </div>
      </div>
      <div class="ov-stats">
        <div class="ov-stat">
          <span class="ov-label">下载速度</span>
          <span class="ov-value mono">{{ fmtSpeed(dlState.totalSpeedBps) }}</span>
        </div>
        <div class="ov-stat">
          <span class="ov-label">预计剩余</span>
          <span class="ov-value mono">{{ fmtEta(dlState.totalEtaSeconds) }}</span>
        </div>
        <div class="ov-stat">
          <span class="ov-label">剩余文件</span>
          <span class="ov-value mono">{{ dlState.remainingFiles }}</span>
        </div>
      </div>
    </section>

    <main class="dd-body">
      <!-- 崩溃/重启恢复横幅：上次未完成的任务可一键恢复（幂等重跑） -->
      <div v-if="interruptedJobs.length" class="resume-banner">
        <span class="rb-text">
          检测到 {{ interruptedJobs.length }} 个上次未完成的任务
          <span class="rb-sub">{{ interruptedJobs.map(j => j.name).join("、") }}</span>
        </span>
        <button class="rb-btn rb-primary" @click="resumeAllInterrupted">
          <RotateCcw :size="12" />
          恢复
        </button>
        <button
          class="rb-btn"
          v-tooltip="'放弃全部任务，不再提示'"
          @click="dismissAllInterrupted"
        >
          全部忽略
        </button>
      </div>

      <div v-if="dlState.tasks.length === 0" class="empty">
        <HardDriveDownload :size="44" class="empty-icon" />
        <p>暂无下载任务</p>
        <p class="empty-sub">在资源中心选择版本后，任务进度会显示在这里</p>
      </div>

      <TransitionGroup name="task" class="task-list">
        <div v-for="task in dlState.tasks" :key="task.taskId" class="task-card">
          <div class="tc-hdr">
            <span class="tc-name" :title="task.name">{{ task.name || "下载任务" }}</span>
            <span class="tc-chip" :class="statusChip(task).cls">{{ statusChip(task).text }}</span>
            <template v-if="task.status === 'running'">
              <button class="tc-cancel" v-tooltip="'暂停'" @click="dlState.pauseTask(task.taskId)">
                <Pause :size="13" />
              </button>
              <button class="tc-cancel" v-tooltip="'取消并清理'" @click="handleCancel(task)">
                <X :size="13" />
              </button>
            </template>
            <template v-else-if="task.status === 'paused'">
              <button class="tc-cancel" v-tooltip="'继续下载'" @click="handleResume(task)">
                <RotateCcw :size="13" />
              </button>
              <button class="tc-cancel" v-tooltip="'放弃（删除快照）'" @click="handleDismissPaused(task)">
                <X :size="13" />
              </button>
            </template>
          </div>

          <!-- 任务级总进度条 -->
          <div v-if="task.status === 'running'" class="tc-progress">
            <div class="tc-progress-bar">
              <div class="tc-progress-fill" :style="{ width: task.progress + '%' }"></div>
            </div>
            <span class="tc-progress-pct mono">{{ Math.floor(task.progress) }}%</span>
            <span v-if="task.speedBps > 0" class="tc-progress-speed mono">{{ fmtSpeed(task.speedBps) }}</span>
          </div>

          <div v-if="task.status === 'error' && task.error" class="tc-err" v-tooltip="task.error">
            {{ errMsg(task.error) }}
          </div>

          <div class="tc-steps">
            <div v-for="st in visibleSteps(task)" :key="st.name" class="tc-step">
              <span class="tc-st-icon" :class="st.status">
                <Check v-if="st.status === 'done'" :size="10" :stroke-width="2" />
                <LoaderCircle v-else-if="st.status === 'running'" class="tc-spin" :size="10" :stroke-width="2" />
                <span v-else class="tc-dot"></span>
              </span>
              <span class="tc-st-name">{{ st.name }}</span>
              <div v-if="st.status === 'running'" class="tc-st-bar">
                <div class="tc-st-bar-fill" :style="{ width: st.progress + '%' }"></div>
              </div>
              <span v-if="st.status !== 'pending'" class="tc-st-pct mono">{{ Math.floor(st.progress) }}%</span>
            </div>
            <button v-if="task.steps.length > MAX_VISIBLE_STEPS" class="tc-more" @click="toggleSteps(task.taskId)">
              {{ expandedSteps.has(task.taskId) ? "收起" : `显示全部 ${task.steps.length} 项` }}
            </button>
          </div>
        </div>
      </TransitionGroup>
    </main>
    <ScrollTopFab />
  </div>
</template>

<style scoped>
.dd {
  display: flex;
  flex-direction: column;
  height: 100%;
  gap: 14px;
  background-color: var(--surface-page);
  padding: 16px 20px 20px;
  overflow: hidden;
}

/* ── 顶部总览条 ── */
.dd-overview {
  flex: none;
  display: flex;
  align-items: center;
  gap: 28px;
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 16px 20px;
}

.ov-main {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}

.ov-pct {
  font-size: 26px;
  font-weight: 800;
  color: var(--color-brand);
  flex: none;
  font-variant-numeric: tabular-nums;
}

.ov-unit { font-size: 14px; font-weight: 700; }

.ov-bar {
  flex: 1;
  height: 8px;
  border-radius: 4px;
  background-color: var(--surface-inset, var(--surface-hover));
  overflow: hidden;
  min-width: 60px;
}

.ov-bar-fill {
  height: 100%;
  border-radius: 4px;
  background: var(--color-brand-gradient, var(--color-brand));
  transition: width 0.25s ease;
}

.ov-sub {
  flex: none;
  font-size: 12px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.ov-stats {
  flex: none;
  display: flex;
  gap: 24px;
}

.ov-stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 76px;
}

.ov-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.3px;
}

.ov-value {
  font-size: 15px;
  font-weight: 700;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}

/* ── 主体 ── */
.dd-body {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  display: flex;
  flex-direction: column;
}

.mono { font-variant-numeric: tabular-nums; }

.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-tertiary);
  font-size: 14px;
}

.empty-icon {
  opacity: 0.5;
  color: var(--text-tertiary);
  margin-bottom: 4px;
}

.empty-sub {
  font-size: 12px;
  opacity: 0.8;
}

/* 崩溃恢复横幅 */
.resume-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 10px 14px;
  margin-bottom: 12px;
  background: var(--color-brand-bg);
  border: 1px solid var(--color-brand);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--text-primary);
}

.rb-text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
.rb-sub {
  font-size: 12px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.rb-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface-card);
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.rb-btn:hover { border-color: var(--color-brand); }
.rb-primary { background: var(--color-brand); border-color: var(--color-brand); color: var(--text-inverse); }
.rb-primary:hover { opacity: 0.9; }

/* ── 任务卡片 ── */
.task-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.task-card {
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 11px 14px;
}

.tc-hdr {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 28px;
}

.tc-name {
  flex: 1;
  min-width: 0;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
  line-height: 1.2;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tc-chip {
  flex: none;
  padding: 2px 8px;
  border-radius: 9px;
  font-size: 11px;
  font-weight: 600;
  line-height: 1.4;
}

.tc-chip.running { background: var(--color-brand-bg); color: var(--color-brand); }
.tc-chip.pending { background: var(--surface-hover); color: var(--text-tertiary); }
.tc-chip.err { background: var(--color-danger-bg); color: var(--color-danger); }

.tc-cancel {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
  color: var(--text-tertiary);
  transition: all 0.15s;
}

.tc-cancel:hover {
  background-color: var(--color-danger-bg);
  color: var(--color-danger);
}

/* 任务级进度条 */
.tc-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
}

.tc-progress-bar {
  flex: 1;
  height: 6px;
  border-radius: 3px;
  background-color: var(--surface-hover);
  overflow: hidden;
}

.tc-progress-fill {
  height: 100%;
  border-radius: 3px;
  background: var(--color-brand-gradient, var(--color-brand));
  transition: width 0.25s ease;
}

.tc-progress-pct {
  flex: none;
  font-size: 12px;
  font-weight: 700;
  color: var(--color-brand);
  min-width: 36px;
  text-align: right;
}

.tc-progress-speed {
  flex: none;
  font-size: 12px;
  color: var(--text-tertiary);
  min-width: 64px;
  text-align: right;
}

.tc-err {
  margin-top: 8px;
  font-size: 12px;
  color: var(--color-danger);
  line-height: 1.5;
  word-break: break-all;
  cursor: help;
}

.tc-steps {
  margin-top: 7px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.tc-step {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 18px;
}

.tc-st-icon {
  flex: none;
  width: 12px;
  height: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
}

.tc-st-icon.running { color: var(--color-brand); }
.tc-st-icon.done { color: var(--color-success); }

.tc-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background-color: var(--text-tertiary);
  opacity: 0.6;
}

.tc-spin {
  animation: tc-spin 0.9s linear infinite;
}

@keyframes tc-spin {
  to { transform: rotate(360deg); }
}

.tc-st-name {
  flex: none;
  font-size: 12px;
  color: var(--text-secondary);
}

.tc-st-bar {
  flex: 1;
  max-width: 120px;
  height: 4px;
  border-radius: 2px;
  background-color: var(--surface-hover);
  overflow: hidden;
}

.tc-st-bar-fill {
  height: 100%;
  border-radius: 2px;
  background: var(--color-brand);
  transition: width 0.2s ease;
}

.tc-st-pct {
  flex: none;
  font-size: 11px;
  color: var(--text-tertiary);
  min-width: 34px;
  text-align: right;
}

.tc-more {
  align-self: flex-start;
  margin-top: 2px;
  padding: 2px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: var(--surface-hover);
  color: var(--text-secondary);
  font-size: 11.5px;
  cursor: pointer;
  transition: all 0.15s;
}

.tc-more:hover { color: var(--color-brand); }

/* TransitionGroup 卡片增删动画 */
.task-enter-active { transition: all 0.22s ease; }
.task-leave-active { transition: all 0.18s ease; position: absolute; }
.task-enter-from { opacity: 0; transform: translateY(10px); }
.task-leave-to { opacity: 0; transform: translateY(-6px); }
.task-move { transition: transform 0.2s ease; }
</style>
