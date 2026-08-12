<script setup lang="ts">
// 设置页容器：导航 + 各 tab 内容编排（业务在 store/组件，本文件不做配置逻辑）
import { ref } from "vue";
import { SlidersHorizontal, Download } from "@lucide/vue";
import SettingsOfflineSkin from "../component/SettingsOfflineSkin.vue";
import SettingsLaunchOptions from "../component/SettingsLaunchOptions.vue";
import SettingsJdk from "../component/SettingsJdk.vue";
import SettingsDownload from "../component/SettingsDownload.vue";
import ScrollTopFab from "../component/ScrollTopFab.vue";

type Tab = "launch" | "download";

const currentTab = ref<Tab>("launch");
</script>

<template>
  <div class="stg">
    <nav class="stg-nav">
      <div class="nav-divider"><span>设置</span></div>
      <button
        class="stg-tab"
        :class="{ active: currentTab === 'launch' }"
        @click="currentTab = 'launch'"
      >
        <SlidersHorizontal :size="16" />
        启动设置
      </button>
      <button
        class="stg-tab"
        :class="{ active: currentTab === 'download' }"
        @click="currentTab = 'download'"
      >
        <Download :size="16" />
        下载设置
      </button>
    </nav>

    <div class="stg-body">
      <Transition name="page" mode="out-in">
        <!-- 启动设置：离线皮肤 / 启动选项 / 内存 / Java -->
        <div v-if="currentTab === 'launch'" key="launch" class="stg-page">
          <SettingsOfflineSkin />
          <SettingsLaunchOptions />
          <SettingsJdk />
        </div>

        <!-- 下载设置：管理游戏下载源 -->
        <div v-else key="download" class="stg-page">
          <SettingsDownload />
        </div>
      </Transition>
    </div>

    <ScrollTopFab />
  </div>
</template>

<style scoped>
.stg { display: flex; height: 100%; }

/* ===== 左侧导航 ===== */
.stg-nav {
  flex: 0 0 180px; min-width: 160px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 12px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-divider {
  display: flex; align-items: center;
  padding: 8px 12px 6px; user-select: none;
}

.nav-divider span {
  font-size: 11px; font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase; letter-spacing: 0.8px;
}

.stg-tab {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 12px;
  border: none; border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px; font-weight: 500;
  cursor: pointer; transition: all 0.15s;
  text-align: left;
}

.stg-tab:hover {
  color: var(--text-primary);
  background-color: var(--surface-hover);
}

.stg-tab.active {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* ===== 右侧内容 ===== */
.stg-body {
  flex: 1; position: relative;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  background: var(--surface-page);
}

/* 页面过渡 */
.page-enter-active, .page-leave-active { transition: all 0.15s ease-out; }
.page-enter-from { opacity: 0; transform: translateY(8px); }
.page-leave-to { opacity: 0; transform: translateY(-8px); }
</style>
