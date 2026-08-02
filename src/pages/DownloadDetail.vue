<script setup lang="ts">
import { dlState, TaskItem } from "../store/download"
import { useRouter } from "vue-router"

const router = useRouter()

function fmtSpeed(bps: number): string {
  if (bps >= 1_000_000) return (bps / 1_000_000).toFixed(2) + " MB/s"
  if (bps >= 1_000) return (bps / 1_000).toFixed(2) + " KB/s"
  return bps.toFixed(2) + " B/s"
}

async function handleCancel(task: TaskItem) {
  const name = task.name || "下载任务";
  await dlState.cancelTask(task.taskId)
  dlState.removeTask(task.taskId)
  ;(window as any).__showToast?.(`${name} 已取消`)
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
          <span class="summary-value mono">{{ Math.round(dlState.totalProgress) }}%</span>
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
          <div v-if="task.status === 'error' && task.error" class="tc-err">{{ task.error }}</div>
        </div>
      </TransitionGroup>
    </main>
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

/* transition */
.task-enter-active, .task-leave-active { transition: all 0.25s ease; }
.task-enter-from { opacity: 0; transform: translateY(-12px); }
.task-leave-to { opacity: 0; transform: translateX(40px); }
.task-move { transition: transform 0.2s ease; }
</style>
