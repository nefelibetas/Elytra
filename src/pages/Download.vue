<script setup lang="ts">
// 下载页容器：导航 + 各 tab 内容编排（具体逻辑在 store/组件中，本文件不做业务）
import { ref, watch, onMounted } from "vue";
import DownloadTab from "../component/DownloadTab.vue";
import VanillaList from "../component/VanillaList.vue";
import VanillaDetail from "../component/VanillaDetail.vue";
import ModSearchBar from "../component/ModSearchBar.vue";
import ModResultList from "../component/ModResultList.vue";
import ScrollTopFab from "../component/ScrollTopFab.vue";
import { vdState } from "../store/versionDownload";
import { modSearch } from "../store/modSearch";

type Tab = "vanilla" | "mod" | "modpack" | "datapack" | "resourcepack" | "shader";

const currentTab = ref<Tab>("vanilla");

onMounted(() => {
  vdState.fetchManifest();
});

// 首次进入 Mod 页自动搜索热门（对齐 PCL2；重置后不重复自动触发）
watch(currentTab, (t) => {
  if (t === "mod") modSearch.ensureAutoSearch();
});
</script>

<template>
  <div class="dl">
    <nav class="dl-nav">
      <DownloadTab :active="currentTab === 'vanilla'" @click="currentTab = 'vanilla'" @refresh="vdState.refresh()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z" /></svg>
        </template>
        原版游戏
      </DownloadTab>
      <div class="nav-divider"><span>社区资源</span></div>
      <DownloadTab :active="currentTab === 'mod'" @click="currentTab = 'mod'">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M3 2a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1V3a1 1 0 00-1-1H3zm8 0a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1V3a1 1 0 00-1-1h-2zM3 10a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 00-1-1H3zm8 0a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 00-1-1h-2z" /></svg></template>Mod
      </DownloadTab>
      <DownloadTab :active="currentTab === 'modpack'" @click="currentTab = 'modpack'">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M2 3a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H3a1 1 0 01-1-1V3zm6 0a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V3zM2 9a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H3a1 1 0 01-1-1V9zm6 0a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V9z" /></svg></template>整合包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'datapack'" @click="currentTab = 'datapack'">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M4 2h8a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1zm0 4h8a1 1 0 011 1v6a1 1 0 01-1 1H4a1 1 0 01-1-1V7a1 1 0 011-1zm1 2v4h6V8H5z" /></svg></template>数据包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'resourcepack'" @click="currentTab = 'resourcepack'">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="8" cy="8" r="2" /></svg></template>资源包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'shader'" @click="currentTab = 'shader'">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 1a1 1 0 011 1v.5a1 1 0 01-2 0V2a1 1 0 011-1zM4.5 4.5a1 1 0 011.4 0l.4.3a1 1 0 01-1.4 1.4l-.4-.3a1 1 0 010-1.4zm7 0a1 1 0 010 1.4l-.4.3a1 1 0 11-1.4-1.4l.4-.3a1 1 0 011.4 0zM2 7a1 1 0 011 1v.5a1 1 0 01-2 0V8a1 1 0 011-1zm12 0a1 1 0 011 1v.5a1 1 0 01-2 0V8a1 1 0 011-1zm-6 2a1 1 0 011 1v.5a1 1 0 01-2 0V10a1 1 0 011-1z" /></svg></template>光影包
      </DownloadTab>
    </nav>

    <div class="dl-body">
      <Transition name="page" mode="out-in">
        <div v-if="currentTab === 'vanilla' && !vdState.selectedVersion" key="list" class="dl-page">
          <VanillaList />
        </div>

        <div v-else-if="currentTab === 'vanilla' && vdState.selectedVersion" key="detail" class="dl-page">
          <VanillaDetail />
        </div>

        <div v-else-if="currentTab === 'mod'" key="mod" class="dl-page mod-dl-page">
          <ModSearchBar />
          <ModResultList />
        </div>

        <div v-else key="other" class="dl-page">
          <h2>{{ currentTab }}</h2>
        </div>
      </Transition>
    </div>
    <ScrollTopFab />
  </div>
</template>

<style scoped>
.dl { display: flex; height: 100%; }
.dl-nav {
  flex: 0 0 180px; min-width: 160px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  display: flex; flex-direction: column;
  padding: 16px 12px; gap: 2px;
}
.nav-divider {
  display: flex; align-items: center;
  padding: 12px 12px 6px; margin-top: 8px; user-select: none;
}
.nav-divider span {
  font-size: 11px; font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase; letter-spacing: 0.8px;
}
.dl-body {
  flex: 1; position: relative; padding: 24px; overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable;
  background: var(--surface-page);
}
.dl-page {
  display: flex; flex-direction: column; gap: 16px; min-height: 100%;
  background: var(--surface-page);
}
.mod-dl-page { gap: 8px; }

/* page transition - fast opacity fade */
.page-enter-active, .page-leave-active { transition: opacity 0.12s ease; }
.page-enter-from { opacity: 0; }
.page-leave-to { opacity: 0; }
</style>
