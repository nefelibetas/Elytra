<script setup lang="ts">
// 页面容器：SideNav + 各 tab 编排；工具 tab 的内容是工具 Grid（SettingsTools）。
import { SlidersHorizontal, Download, Palette, Link2, FileText, KeyRound, Wrench, Info } from "@lucide/vue";
import SettingsLaunchOptions from "../component/settings/SettingsLaunchOptions.vue";
import SettingsJdk from "../component/settings/SettingsJdk.vue";
import SettingsExternal from "../component/settings/SettingsExternal.vue";
import SettingsDownload from "../component/settings/SettingsDownload.vue";
import SettingsAppearance from "../component/settings/SettingsAppearance.vue";
import SettingsOnline from "../component/settings/SettingsOnline.vue";
import SettingsLog from "../component/settings/SettingsLog.vue";
import SettingsTools from "../component/settings/SettingsTools.vue";
import SettingsAbout from "../component/settings/SettingsAbout.vue";
import ScrollTopFab from "../component/common/ScrollTopFab.vue";
import SideNav from "../component/common/SideNav.vue";
import { settingsTab } from "../store/app/settingsTab";
import { useI18n } from "vue-i18n";

const { t } = useI18n();
</script>

<template>
  <div class="stg">
    <SideNav
      v-model="settingsTab"
      :title="t('nav.settings')"
      :tabs="[
        { key: 'launch', label: t('settings.tabs.launch'), icon: SlidersHorizontal },
        { key: 'external', label: t('settings.tabs.external'), icon: KeyRound },
        { key: 'download', label: t('settings.tabs.download'), icon: Download },
        { key: 'appearance', label: t('settings.tabs.appearance'), icon: Palette },
        { key: 'online', label: t('settings.tabs.online'), icon: Link2 },
        { key: 'log', label: t('settings.tabs.log'), icon: FileText },
        { key: 'tools', label: t('settings.tabs.tools'), icon: Wrench },
        { key: 'about', label: t('settings.tabs.about'), icon: Info },
      ]"
    >
      <div v-if="settingsTab === 'launch'" key="launch" class="stg-page">
        <SettingsLaunchOptions />
        <SettingsJdk />
      </div>

      <div v-else-if="settingsTab === 'external'" key="external" class="stg-page">
        <SettingsExternal />
      </div>

      <div v-else-if="settingsTab === 'download'" key="download" class="stg-page">
        <SettingsDownload />
      </div>

      <div v-else-if="settingsTab === 'appearance'" key="appearance" class="stg-page">
        <SettingsAppearance />
      </div>

      <div v-else-if="settingsTab === 'online'" key="online" class="stg-page">
        <SettingsOnline />
      </div>

      <div v-else-if="settingsTab === 'tools'" key="tools" class="stg-page stg-page-fill">
        <SettingsTools />
      </div>

      <div v-else-if="settingsTab === 'about'" key="about" class="stg-page">
        <SettingsAbout />
      </div>

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

/* 工具详情需要确定高度：SkinPainter 的 canvas 依赖父级 flex 高度 */
.stg-page-fill {
  height: 100%;
  min-height: 0;
}
</style>
