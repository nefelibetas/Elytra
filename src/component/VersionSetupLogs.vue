<script setup lang="ts">
// 版本设置-游戏日志：读取该版本 logs/latest.log（游戏自身日志，游戏 cwd 是版本目录）
// 与 .minecraft/logs/elytra-launch.log（启动器捕获的启动输出），尾部 200 行。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "@lucide/vue";
import { activePath } from "../store/gameDirs";

const route = useRoute();
/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

const logText = ref("");
const loading = ref(false);

async function loadLog() {
  if (!activePath.value) {
    logText.value = "未选择游戏目录（设置 → 游戏目录）";
    return;
  }
  loading.value = true;
  try {
    const text = await invoke<string>("get_game_log", {
      gameDir: activePath.value,
      versionName: versionName.value,
    });
    logText.value = text || "暂无日志（尚未启动过该版本）";
  } catch (e) {
    logText.value = String(e);
  }
  loading.value = false;
}

onMounted(loadLog);
</script>

<template>
  <div class="vs-page">
    <div class="setup-card">
      <div class="setup-card-hdr">
        <div class="setup-card-titles">
          <div class="setup-card-title">游戏日志</div>
          <div class="setup-desc">该版本 logs/latest.log 与启动器捕获日志的尾部 200 行</div>
        </div>
        <button class="vslog-refresh" :disabled="loading" @click="loadLog">
          <RefreshCw :size="13" :class="{ spin: loading }" />
          刷新
        </button>
      </div>
      <pre class="vslog-pre">{{ logText }}</pre>
    </div>
  </div>
</template>

<style scoped>
.vs-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}
.setup-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  max-width: 720px;
  padding: 14px 16px;
}
.setup-card-hdr {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}
.setup-card-titles { min-width: 0; }
.setup-card-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
}
.setup-desc {
  margin-top: 2px;
  font-size: 12px;
  color: var(--text-tertiary);
}
.vslog-refresh {
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
.vslog-refresh:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.vslog-pre {
  margin: 0;
  padding: 12px;
  max-height: 60vh;
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
  animation: vslog-spin 0.8s linear infinite;
}
@keyframes vslog-spin {
  to { transform: rotate(360deg); }
}
</style>
</style>
