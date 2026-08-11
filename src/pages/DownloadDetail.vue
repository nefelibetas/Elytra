<script setup lang="ts">
import { ref } from "vue"
import { dlState, TaskItem } from "../store/download"
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
      <div v-if="dlState.tasks.length === 0" class="empty">
        <p>暂无下载任务</p>
      </div>

      <TransitionGroup name="task" class="task-list">
        <div v-for="task in dlState.tasks" :key="task.taskId" class="task-card">
          <div class="tc-hdr">
            <span class="tc-name">{{ task.name || "下载任务" }}</span>
            <button class="tc-cancel" title="取消并清理" @click="handleCancel(task)">
              <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
                <polygon points="1,1 5,5 9,1 11,3 7,7 11,11 9,13 5,9 1,13 -1,11 3,7 -1,3" />
              </svg>
            </button>
          </div>
          <div v-if="task.status === 'pending'" class="tc-phase">等待中</div>
          <div v-if="task.status === 'error' && task.error" class="tc-err">{{ task.error }}</div>

          <div class="tc-steps">
            <div v-for="st in visibleSteps(task)" :key="st.name" class="tc-step">
              <span class="tc-st-icon" :class="st.status">
                <svg v-if="st.status === 'done'" viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 6.5 5 9.5 10 3.5" /></svg>
                <svg v-else-if="st.status === 'running'" class="tc-spin" viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 1a5 5 0 1 1-4.95 4" /></svg>
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
  background-color: rgba(231,76,60,0.1);
  color: #e74c3c;
}

.tc-err {
  margin-top: 8px;
  font-size: 12px;
  color: #e74c3c;
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
.tc-st-icon.done { color: #22c55e; }

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
