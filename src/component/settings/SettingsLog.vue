<script setup lang="ts">
// 启动器日志：查看 launcher.log（stdout/stderr 重定向到 exe 旁 Elytra/）。
import { onMounted, ref } from "vue";
import { loadLauncherLog, openLauncherLogDir } from "../../store/app/appLog";
import { RefreshCw, FolderOpen } from "@lucide/vue";

const logText = ref("");
const loading = ref(false);

async function loadLog() {
  loading.value = true;
  try {
    const text = await loadLauncherLog();
    logText.value = text || "暂无启动器日志（launcher.log 尚未产生内容）";
  } catch (e) {
    logText.value = String(e);
  }
  loading.value = false;
}

onMounted(loadLog);

async function openLogDir() {
  try {
    await openLauncherLogDir();
  } catch (e) {
    /* 目录不存在时忽略 */
  }
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr slog-hdr">
      <div class="slog-titles">
        <h3>启动器日志</h3>
        <div class="stg-card-sub">启动器自身输出（exe 旁 Elytra/launcher.log 尾部 200 行）</div>
      </div>
      <div class="slog-actions">
        <button class="slog-refresh" @click="openLogDir">
          <FolderOpen :size="13" />
          打开目录
        </button>
        <button class="slog-refresh" :disabled="loading" @click="loadLog">
          <RefreshCw :size="13" :class="{ spin: loading }" />
          刷新
        </button>
      </div>
    </div>
    <pre class="slog-pre">{{ logText }}</pre>
  </div>
</template>

<style scoped>
.slog-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}
.slog-hdr {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
}
.slog-titles {
  min-width: 0;
}
.slog-refresh {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 10px;
  font-size: 12px;
  border: 1px solid var(--border-default);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.slog-refresh:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.slog-pre {
  margin: 12px 0 0;
  padding: 12px;
  max-height: 55vh;
  overflow: auto;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 1.55;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 8px;
  white-space: pre-wrap;
  word-break: break-all;
  user-select: text;
}
.spin {
  animation: slog-spin 0.8s linear infinite;
}
@keyframes slog-spin {
  to { transform: rotate(360deg); }
}
</style>
