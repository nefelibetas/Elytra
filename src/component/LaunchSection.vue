<script setup lang="ts">
// 启动区：启动按钮（下载游戏入口）+ 版本选择 / 版本设置
import { useRouter } from "vue-router";
import { isLaunching, doLaunch } from "../store/launch";
import { selectedVersion } from "../store/selectedVersion";

const router = useRouter();

function goDownload() {
  router.push("/download");
}
</script>

<template>
  <div class="section s-launch">
    <div class="launch-area">
      <button
        class="launch-btn"
        :class="{ launching: isLaunching }"
        :disabled="isLaunching"
        @click="doLaunch(goDownload)"
      >
        <template v-if="!selectedVersion">
          <span class="launch-label">下载游戏</span>
          <span class="launch-version">未找到可用的游戏版本</span>
        </template>
        <template v-else-if="isLaunching">
          <span class="launch-label">启动中...</span>
          <span class="launch-version">{{ selectedVersion }}</span>
        </template>
        <template v-else>
          <span class="launch-label">启动游戏</span>
          <span class="launch-version">{{ selectedVersion }}</span>
        </template>
      </button>

      <div class="launch-actions">
        <button class="secondary-btn" @click="router.push('/version-select')">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
            <path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z" />
          </svg>
          <span>版本选择</span>
        </button>
        <button v-if="selectedVersion" class="secondary-btn" @click="router.push({ path: '/version-setup', query: { name: selectedVersion } })">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <line x1="21" y1="4" x2="14" y2="4"/>
            <line x1="10" y1="4" x2="3" y2="4"/>
            <line x1="21" y1="12" x2="12" y2="12"/>
            <line x1="8" y1="12" x2="3" y2="12"/>
            <line x1="21" y1="20" x2="16" y2="20"/>
            <line x1="12" y1="20" x2="3" y2="20"/>
            <line x1="14" y1="2" x2="14" y2="6"/>
            <line x1="8" y1="10" x2="8" y2="14"/>
            <line x1="16" y1="18" x2="16" y2="22"/>
          </svg>
          <span>版本设置</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.section {
  display: flex;
  align-items: center;
}

.s-launch { align-items: flex-start; }

.launch-area {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.launch-btn {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  padding: 16px;
  border-radius: var(--radius-lg);
  border: none;
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 4px 16px var(--color-brand-glow);
}

.launch-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 6px 20px var(--color-brand-glow);
}

.launch-btn:active:not(:disabled) { transform: translateY(0); }
.launch-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  box-shadow: none;
}

.launch-btn.launching {
  opacity: 0.7;
  pointer-events: none;
}

.launch-label {
  font-size: 16px;
  font-weight: 600;
  line-height: 1.4;
}

.launch-version {
  font-size: 12px;
  opacity: 0.8;
  line-height: 1.3;
}

.launch-actions {
  display: flex;
  gap: 8px;
}

.secondary-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 0;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background-color: var(--surface-elevated);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.secondary-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}
</style>
