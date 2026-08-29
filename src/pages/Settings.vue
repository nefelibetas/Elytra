<script setup lang="ts">
// 设置页容器：导航 + 各 tab 内容编排（业务在 store/组件，本文件不做配置逻辑）
import { SlidersHorizontal, Download, Palette, Link2, FileText, KeyRound } from "@lucide/vue";
import SettingsLaunchOptions from "../component/SettingsLaunchOptions.vue";
import SettingsJdk from "../component/SettingsJdk.vue";
import SettingsThirdParty from "../component/SettingsThirdParty.vue";
import SettingsDownload from "../component/SettingsDownload.vue";
import SettingsAppearance from "../component/SettingsAppearance.vue";
import SettingsOnline from "../component/SettingsOnline.vue";
import SettingsLog from "../component/SettingsLog.vue";
import ScrollTopFab from "../component/ScrollTopFab.vue";
import { settingsTab } from "../store/settingsTab";
</script>

<template>
  <div class="stg">
    <nav class="stg-nav">
      <div class="nav-divider"><span>设置</span></div>
      <button
        class="stg-tab"
        :class="{ active: settingsTab === 'launch' }"
        @click="settingsTab = 'launch'"
      >
        <SlidersHorizontal :size="16" />
        启动设置
      </button>
      <button
        class="stg-tab"
        :class="{ active: settingsTab === 'thirdparty' }"
        @click="settingsTab = 'thirdparty'"
      >
        <KeyRound :size="16" />
        第三方登录
      </button>
      <button
        class="stg-tab"
        :class="{ active: settingsTab === 'download' }"
        @click="settingsTab = 'download'"
      >
        <Download :size="16" />
        下载设置
      </button>
      <button
        class="stg-tab"
        :class="{ active: settingsTab === 'appearance' }"
        @click="settingsTab = 'appearance'"
      >
        <Palette :size="16" />
        外观
      </button>
      <button
        class="stg-tab"
        :class="{ active: settingsTab === 'online' }"
        @click="settingsTab = 'online'"
      >
        <Link2 :size="16" />
        联机
      </button>
      <button
        class="stg-tab"
        :class="{ active: settingsTab === 'log' }"
        @click="settingsTab = 'log'"
      >
        <FileText :size="16" />
        启动器日志
      </button>
    </nav>

    <div class="stg-body">
      <Transition name="page" mode="out-in">
        <!-- 启动设置：启动选项 / 内存 / Java -->
        <div v-if="settingsTab === 'launch'" key="launch" class="stg-page">
          <SettingsLaunchOptions />
          <SettingsJdk />
        </div>

        <!-- 下载设置：管理游戏下载源 -->
        <div v-else-if="settingsTab === 'thirdparty'" key="thirdparty" class="stg-page">
          <SettingsThirdParty />
        </div>

        <div v-else-if="settingsTab === 'download'" key="download" class="stg-page">
          <SettingsDownload />
        </div>

        <!-- 外观：亮色 / 暗色 / 跟随系统 -->
        <div v-else-if="settingsTab === 'appearance'" key="appearance" class="stg-page">
          <SettingsAppearance />
        </div>

        <!-- 联机：EasyTier 虚拟局域网 -->
        <div v-else-if="settingsTab === 'online'" key="online" class="stg-page">
          <SettingsOnline />
        </div>

        <!-- 启动器日志 -->
        <div v-else key="log" class="stg-page">
          <SettingsLog />
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