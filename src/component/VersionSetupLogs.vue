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
  <div class="vslog">
    <div class="vslog-hdr">
      <span class="vslog-title">游戏日志</span>
      <button class="vslog-refresh" :disabled="loading" @click="loadLog">
        <RefreshCw :size="13" :class="{ spin: loading }" />
        刷新
      </button>
    </div>
    <pre class="vslog-pre">{{ logText }}</pre>
  </div>
</template>

<style scoped>
.vslog {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: 100%;
}
.vslog-hdr {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.vslog-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
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
  flex: 1;
  margin: 0;
  padding: 12px;
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
