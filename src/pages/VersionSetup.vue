<script setup lang="ts">
// 版本设置页容器：左侧导航 + 各 tab 编排（业务在 VersionSetupOverview 组件）
import { ref } from "vue";
import VersionSetupOverview from "../component/VersionSetupOverview.vue";

type TabKey = "overview" | "settings" | "mods" | "export";

const tabs: { key: TabKey; label: string; icon: string }[] = [
  { key: "overview", label: "概览", icon: "info" },
  { key: "settings", label: "设置", icon: "settings" },
  { key: "mods", label: "Mod 管理", icon: "puzzle" },
  { key: "export", label: "导出", icon: "export" },
];

const activeTab = ref<TabKey>("overview");
</script>

<template>
  <div class="vs">
    <!-- 左侧导航（标题栏由窗口 TitleBar 提供） -->
    <div class="vs-left">
      <div class="vs-nav-list">
        <div
          v-for="t in tabs"
          :key="t.key"
          class="vs-nav-item"
          :class="{ active: activeTab === t.key }"
          @click="activeTab = t.key"
        >
          <svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor">
            <path v-if="t.icon === 'info'" d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 3.4a1.1 1.1 0 110 2.2 1.1 1.1 0 010-2.2zm1 7.8H7V6.8h2v5.4z" />
            <g v-if="t.icon === 'settings'" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" transform="translate(4 4) scale(0.667)">
              <line x1="21" y1="4" x2="14" y2="4"/>
              <line x1="10" y1="4" x2="3" y2="4"/>
              <line x1="21" y1="12" x2="12" y2="12"/>
              <line x1="8" y1="12" x2="3" y2="12"/>
              <line x1="21" y1="20" x2="16" y2="20"/>
              <line x1="12" y1="20" x2="3" y2="20"/>
              <line x1="14" y1="2" x2="14" y2="6"/>
              <line x1="8" y1="10" x2="8" y2="14"/>
              <line x1="16" y1="18" x2="16" y2="22"/>
            </g>
            <path v-if="t.icon === 'puzzle'" d="M9.5 1h1.4a1.4 1.4 0 011.4 1.4v1.4h.7a1.5 1.5 0 010 3h-.7v3.3a1.4 1.4 0 01-1.4 1.4H7.6v-.7a1.5 1.5 0 00-3 0v.7H2.9a1.4 1.4 0 01-1.4-1.4V9.9h.7a1.5 1.5 0 000-3h-.7V2.4A1.4 1.4 0 012.9 1h2.4v.7a1.5 1.5 0 003 0V1z" />
            <path v-if="t.icon === 'export'" d="M8 1.5a1 1 0 01.7.3l3.5 3.5a1 1 0 01-1.4 1.4L9 4.4V11a1 1 0 01-2 0V4.4L5.2 6.7a1 1 0 01-1.4-1.4l3.5-3.5a1 1 0 01.7-.3zM2 12.5a1 1 0 011 1V14h10v-.5a1 1 0 112 0V15a1 1 0 01-1 1H2a1 1 0 01-1-1v-1.5a1 1 0 011-1z" />
          </svg>
          <span>{{ t.label }}</span>
        </div>
      </div>
    </div>

    <!-- 右侧：当前 tab 内容 -->
    <div class="vs-right">
      <VersionSetupOverview v-if="activeTab === 'overview'" />

      <div v-else-if="activeTab === 'settings'" class="vs-page">
        <div class="vs-hint">版本设置功能开发中</div>
      </div>

      <div v-else-if="activeTab === 'mods'" class="vs-page">
        <div class="vs-hint">Mod 管理功能开发中</div>
      </div>

      <div v-else class="vs-page">
        <div class="vs-hint">导出功能开发中</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.vs {
  display: flex;
  height: 100%;
}

/* ===== 左侧导航 ===== */
.vs-left {
  flex: 0 0 240px;
  min-width: 220px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
}

.vs-nav-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.vs-nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.12s;
  user-select: none;
}

.vs-nav-item:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.vs-nav-item.active {
  background: var(--color-brand-bg);
  color: var(--color-brand);
  font-weight: 600;
}

/* ===== 右侧 ===== */
.vs-right {
  flex: 1;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface-page);
}

.vs-hint {
  padding: 24px;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}
</style>
