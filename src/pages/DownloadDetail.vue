<script setup lang="ts">
import { ref } from "vue"
import { X, Check, LoaderCircle, RotateCcw } from "@lucide/vue"
import { dlState, TaskItem, interruptedJobs, resumeAllInterrupted, dismissInterrupted } from "../store/download"
import { showToast } from "../utils/toast"
import { useRouter } from "vue-router"
import ScrollTopFab from "../component/ScrollTopFab.vue"

const router = useRouter()

/** 批量文件任务步骤可能很多，默认折叠到前 N 个。 */
const MAX_VISIBLE_STEPS = 8
const expandedSteps = ref<Set<string>>(new Set())

function fmtSpeed(bps: number): string {
  if (bps >= 1_000_000) return (bps / 1_000_000).toFixed(2) + " MB/s"
  if (bps >= 1_000) return (bps / 1_000).toFixed(2) + " KB/s"
  return bps.toFixed(2) + " B/s"
}

/** 预计剩余时间格式化；null（停滞/速度过低）显示 —。 */
function fmtEta(secs: number | null): string {
  if (secs == null) return "—"
  if (secs < 60) return `${secs} 秒`
  const m = Math.floor(secs / 60)
  if (m < 60) return `${m} 分 ${secs % 60} 秒`
  return `${Math.floor(m / 60)} 时 ${m % 60} 分`
}

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

async function handleCancel(task: TaskItem) {
  const name = task.name || "下载任务";
  await dlState.cancelTask(task.taskId)
  dlState.removeTask(task.taskId)
  ;showToast(`${name} 已取消`)
  if (dlState.tasks.length === 0) {
    router.push(task.previousRoute || "/")
  }
}
</script>

<template>
  <div class="dd">
    <aside class="dd-left">
      <div class="summary">
        <div class="summary-row">
          <span class="summary-label">下载速度</span>
          <span class="summary-value mono">{{ fmtSpeed(dlState.totalSpeedBps) }}</span>
        </div>
        <div class="summary-row">
          <span class="summary-label">预计剩余</span>
          <span class="summary-value mono">{{ fmtEta(dlState.totalEtaSeconds) }}</span>
        </div>
        <div class="summary-row">
          <span class="summary-label">剩余文件</span>
          <span class="summary-value mono">{{ dlState.remainingFiles }}</span>
        </div>
        <div class="summary-row">
          <span class="summary-label">总进度</span>
          <span class="summary-value mono">{{ Math.floor(dlState.totalProgress) }}%</span>
        </div>
      </div>
    </aside>

    <main class="dd-right">
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
          v-for="j in interruptedJobs"
          :key="j.id"
          class="rb-btn"
          v-tooltip="'放弃该任务，不再提示'"
          @click="dismissInterrupted(j.id)"
        >
          忽略
        </button>
      </div>

      <div v-if="dlState.tasks.length === 0" class="empty">
        <p>暂无下载任务</p>
      </div>

      <TransitionGroup name="task" class="task-list">
        <div v-for="task in dlState.tasks" :key="task.taskId" class="task-card">
          <div class="tc-hdr">
            <span class="tc-name">{{ task.name || "下载任务" }}</span>
            <button class="tc-cancel" v-tooltip="'取消并清理'" @click="handleCancel(task)">
              <X :size="12" />
            </button>
          </div>
          <div v-if="task.status === 'pending'" class="tc-phase">等待中</div>
          <div v-if="task.status === 'error' && task.error" class="tc-err">{{ task.error }}</div>

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
              <span v-if="st.status !== 'pending'" class="tc-st-pct">{{ Math.floor(st.progress) }}%</span>
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
  display: flex; height: 100%; gap: 20px;
  background-color: var(--surface-page);
}

.dd-left {
  flex: 0 0 280px;
  display: flex; flex-direction: column;
  justify-content: center;
  background-color: var(--surface-card);
  border-right: 1px solid var(--border-default);
  padding: 28px 24px;
}

.summary {
  display: flex; flex-direction: column; gap: 20px;
}

.summary-row {
  display: flex; flex-direction: column; gap: 4px;
}

.summary-label {
  font-size: 12px; font-weight: 600; color: var(--text-tertiary);
  text-transform: uppercase; letter-spacing: 0.5px;
}

.summary-value {
  font-size: 18px; font-weight: 700; color: var(--text-primary);
  text-align: right;
  font-variant-numeric: tabular-nums;
}



.mono { font-variant-numeric: tabular-nums; }

.dd-right {
  flex: 1; overflow-y: auto; overflow-x: hidden; padding: 20px 20px 20px 0;
  scrollbar-gutter: stable;
}

.empty {
  display: flex; align-items: center; justify-content: center;
  height: 100%; color: var(--text-tertiary); font-size: 14px;
}

/* 崩溃恢复横幅 */
.resume-banner {
  display: flex; align-items: center; gap: 10px; flex-wrap: wrap;
  padding: 10px 14px; margin-bottom: 12px;
  background: var(--color-brand-bg);
  border: 1px solid var(--color-brand);
  border-radius: var(--radius-md);
  font-size: 13px; color: var(--text-primary);
}

.rb-text { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
.rb-sub {
  font-size: 12px; color: var(--text-tertiary);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}

.rb-btn {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 5px 12px; border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: var(--surface-card); color: var(--text-primary);
  font-size: 12px; cursor: pointer; transition: all 0.15s;
}

.rb-btn:hover { border-color: var(--color-brand); }
.rb-primary { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.rb-primary:hover { opacity: 0.9; }

.task-list {
  display: flex; flex-direction: column; gap: 12px;
}

/* TransitionGroup 容器差异兜底：相邻卡片显式间距 */
.task-card + .task-card {
  margin-top: 12px;
}

.task-card {
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 9px 12px;
}

.tc-hdr {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 30px;
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

.tc-cancel {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex; align-items: center; justify-content: center;
  border: none; border-radius: var(--radius-sm);
  background: transparent; cursor: pointer;
  color: var(--text-tertiary); transition: all 0.15s;
}
.tc-cancel:hover {
  background-color: var(--color-danger-bg);
  color: var(--color-danger);
}

.tc-err {
  margin-top: 8px;
  font-size: 12px;
  color: var(--color-danger);
  line-height: 1.5;
  word-break: break-all;
}

.tc-phase {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--text-tertiary);
  line-height: 1.4;
}

.tc-steps {
  margin-top: 6px;
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
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  color: var(--text-primary);
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tc-st-bar {
  flex: 0 0 64px;
  height: 3px;
  border-radius: 2px;
  background-color: var(--surface-hover);
  overflow: hidden;
}
.tc-st-bar-fill {
  height: 100%;
  border-radius: 2px;
  background-color: var(--color-brand);
  transition: width 0.15s ease;
}

.tc-st-pct {
  flex: none;
  width: 34px;
  text-align: right;
  font-size: 10.5px;
  font-variant-numeric: tabular-nums;
  color: var(--text-tertiary);
}

.tc-more {
  align-self: flex-start;
  margin-top: 2px;
  border: none;
  background: none;
  padding: 1px 0;
  font-size: 11px;
  color: var(--color-brand);
  cursor: pointer;
}
.tc-more:hover { text-decoration: underline; }

/* transition */
.task-enter-active, .task-leave-active { transition: all 0.25s ease; }
.task-enter-from { opacity: 0; transform: translateY(-12px); }
.task-leave-to { opacity: 0; transform: translateX(40px); }
.task-move { transition: transform 0.2s ease; }
</style>
