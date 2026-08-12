<script setup lang="ts">
// 版本设置页容器：左侧导航 + 各 tab 编排（业务在 VersionSetupOverview 组件）
import { ref } from "vue";
import type { Component } from "vue";
import { Info, Settings, Puzzle, Upload } from "@lucide/vue";
import VersionSetupOverview from "../component/VersionSetupOverview.vue";

type TabKey = "overview" | "settings" | "mods" | "export";

const tabs: { key: TabKey; label: string; icon: Component }[] = [
  { key: "overview", label: "概览", icon: Info },
  { key: "settings", label: "设置", icon: Settings },
  { key: "mods", label: "Mod 管理", icon: Puzzle },
  { key: "export", label: "导出", icon: Upload },
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
          <component :is="t.icon" :size="15" />
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
