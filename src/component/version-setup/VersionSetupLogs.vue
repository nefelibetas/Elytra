<script setup lang="ts">
// 版本设置-游戏日志：logs/latest.log（游戏 cwd 为版本目录）与 .minecraft/logs/elytra-launch.log，尾部 200 行。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { loadGameLog } from "../../store/app/launch";
import { openFolder } from "../../utils/fs";
import { RefreshCw, FolderOpen } from "@lucide/vue";
import { activePath } from "../../store/config/gameDirs";

const route = useRoute();
const { t } = useI18n();
const versionName = computed(() => String(route.query.name ?? ""));

const logText = ref("");
const loading = ref(false);

async function loadLog() {
  if (!activePath.value) {
    logText.value = t("settings.version.setup.logs.noDir");
    return;
  }
  loading.value = true;
  try {
    const text = await loadGameLog(activePath.value, versionName.value);
    logText.value = text || t("settings.version.setup.logs.empty");
  } catch (e) {
    logText.value = String(e);
  }
  loading.value = false;
}

onMounted(loadLog);

/** 打开该版本的 logs 目录（latest.log 所在）；目录未创建时回退打开版本目录。 */
async function openLogDir() {
  if (!activePath.value) return;
  const vdir = activePath.value.replace(/\\+$/, "") + "/versions/" + versionName.value;
  try {
    await openFolder(vdir + "/logs");
  } catch {
    await openFolder(vdir);
  }
}
</script>

<template>
  <div class="vs-page">
    <div class="setup-card">
      <div class="setup-card-hdr">
        <div class="setup-card-titles">
          <div class="setup-card-title">{{ t("settings.version.setup.logs.title") }}</div>
          <div class="setup-desc">{{ t("settings.version.setup.logs.subtitle") }}</div>
        </div>
        <div class="vslog-actions">
          <button class="vslog-refresh" @click="openLogDir">
            <FolderOpen :size="13" />
            {{ t("settings.version.setup.logs.openDir") }}
          </button>
          <button class="vslog-refresh" :disabled="loading" @click="loadLog">
            <RefreshCw :size="13" :class="{ spin: loading }" />
            {{ t("settings.version.setup.logs.refresh") }}
          </button>
        </div>
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
  padding: 14px 16px;
}
.vslog-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
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
