<script setup lang="ts">
// 设置页容器：SideNav 导航 + 各 tab 内容编排（业务在 store/组件，本文件不做配置逻辑）
import { SlidersHorizontal, Download, Palette, Link2, FileText, KeyRound } from "@lucide/vue";
import SettingsLaunchOptions from "../component/settings/SettingsLaunchOptions.vue";
import SettingsJdk from "../component/settings/SettingsJdk.vue";
import SettingsExternal from "../component/settings/SettingsExternal.vue";
import SettingsDownload from "../component/settings/SettingsDownload.vue";
import SettingsAppearance from "../component/settings/SettingsAppearance.vue";
import SettingsOnline from "../component/settings/SettingsOnline.vue";
import SettingsLog from "../component/settings/SettingsLog.vue";
import ScrollTopFab from "../component/common/ScrollTopFab.vue";
import SideNav from "../component/common/SideNav.vue";
import { settingsTab } from "../store/app/settingsTab";
</script>

<template>
  <div class="stg">
    <SideNav
      v-model="settingsTab"
      title="设置"
      :tabs="[
        { key: 'launch', label: '启动设置', icon: SlidersHorizontal },
        { key: 'external', label: '第三方登录', icon: KeyRound },
        { key: 'download', label: '下载设置', icon: Download },
        { key: 'appearance', label: '外观', icon: Palette },
        { key: 'online', label: '联机设置', icon: Link2 },
        { key: 'log', label: '启动器日志', icon: FileText },
      ]"
    >
      <!-- 启动设置：启动选项 / 内存 / Java -->
      <div v-if="settingsTab === 'launch'" key="launch" class="stg-page">
        <SettingsLaunchOptions />
        <SettingsJdk />
      </div>

      <!-- 下载设置：管理游戏下载源 -->
      <div v-else-if="settingsTab === 'external'" key="external" class="stg-page">
        <SettingsExternal />
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
    </SideNav>

    <ScrollTopFab />
  </div>
</template>

<style scoped>
.stg {
  height: 100%;
}

.stg-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
</style>
