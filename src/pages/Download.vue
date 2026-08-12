<script setup lang="ts">
// 下载页容器：导航 + 各 tab 内容编排（具体逻辑在 store/组件中，本文件不做业务）
import { ref, watch, onMounted } from "vue";
import DownloadTab from "../component/DownloadTab.vue";
import VanillaList from "../component/VanillaList.vue";
import VanillaDetail from "../component/VanillaDetail.vue";
import ModSearchBar from "../component/ModSearchBar.vue";
import ModResultList from "../component/ModResultList.vue";
import ShaderSearchBar from "../component/ShaderSearchBar.vue";
import ShaderResultList from "../component/ShaderResultList.vue";
import ResourcepackSearchBar from "../component/ResourcepackSearchBar.vue";
import ResourcepackResultList from "../component/ResourcepackResultList.vue";
import DatapackSearchBar from "../component/DatapackSearchBar.vue";
import DatapackResultList from "../component/DatapackResultList.vue";
import ModpackSearchBar from "../component/ModpackSearchBar.vue";
import ModpackResultList from "../component/ModpackResultList.vue";
import ScrollTopFab from "../component/ScrollTopFab.vue";
import { vdState } from "../store/versionDownload";
import { modSearch } from "../store/modSearch";
import {
  shaderSearch,
  resourcepackSearch,
  datapackSearch,
  modpackSearch,
} from "../store/categorySearch";

type Tab = "vanilla" | "mod" | "modpack" | "datapack" | "resourcepack" | "shader";

const currentTab = ref<Tab>("vanilla");
/** 右侧滚动容器（切 tab 时复位滚动位置） */
const bodyEl = ref<HTMLElement>();

onMounted(() => {
  vdState.fetchManifest();
  // 预加载社区 tab 热门（对齐 PCL2）：进入下载页即并发请求，切 tab 秒开；
  // ensureAutoSearch 幂等（模块级标志），watch 切 tab 时不会重复请求
  modSearch.ensureAutoSearch();
  shaderSearch.ensureAutoSearch();
  resourcepackSearch.ensureAutoSearch();
  datapackSearch.ensureAutoSearch();
  modpackSearch.ensureAutoSearch();
});

// 首次进入社区 tab 自动搜索热门（预加载已触发时直接跳过）；切 tab 滚动复位到最上方
watch(currentTab, (t) => {
  bodyEl.value?.scrollTo({ top: 0, behavior: "smooth" });
  if (t === "mod") modSearch.ensureAutoSearch();
  if (t === "shader") shaderSearch.ensureAutoSearch();
  if (t === "resourcepack") resourcepackSearch.ensureAutoSearch();
  if (t === "datapack") datapackSearch.ensureAutoSearch();
  if (t === "modpack") modpackSearch.ensureAutoSearch();
});
</script>

<template>
  <div class="dl">
    <nav class="dl-nav">
      <DownloadTab :active="currentTab === 'vanilla'" @click="currentTab = 'vanilla'" @refresh="vdState.refresh()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <path d="M8 1.5L14 5l-6 3.5L2 5z" />
            <path d="M2 5v6l6 3.5V8.5" />
            <path d="M14 5v6l-6 3.5V8.5" />
          </svg>
        </template>
        原版游戏
      </DownloadTab>
      <div class="nav-divider"><span>社区资源</span></div>
      <DownloadTab :active="currentTab === 'mod'" @click="currentTab = 'mod'" @refresh="modSearch.reset()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4" y="3" width="9" height="9" rx="1.5" />
            <path d="M11.5 7.5a2 2 0 01-2-2V3" />
            <path d="M8 12v-4" />
          </svg>
        </template>Mod
      </DownloadTab>
      <DownloadTab :active="currentTab === 'modpack'" @click="currentTab = 'modpack'" @refresh="modpackSearch.reset()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2.5" y="5" width="11" height="8.5" rx="1" />
            <path d="M2.5 8.5h11" />
            <path d="M8 5V3.5a1.5 1.5 0 00-1.5-1.5H4" />
          </svg>
        </template>整合包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'datapack'" @click="currentTab = 'datapack'" @refresh="datapackSearch.reset()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <path d="M6 3.5L2.5 7 6 10.5" />
            <path d="M10 3.5L13.5 7 10 10.5" />
            <path d="M9.2 2.5l-2.4 9" />
          </svg>
        </template>数据包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'resourcepack'" @click="currentTab = 'resourcepack'" @refresh="resourcepackSearch.reset()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <path d="M8 2l6 3-6 3-6-3z" />
            <path d="M2 9l6 3 6-3" />
            <path d="M2 12l6 3 6-3" />
          </svg>
        </template>资源包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'shader'" @click="currentTab = 'shader'" @refresh="shaderSearch.reset()">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="8" cy="8" r="2.5" />
            <path d="M8 1.5v2" />
            <path d="M8 12.5v2" />
            <path d="M1.5 8h2" />
            <path d="M12.5 8h2" />
            <path d="M3.5 3.5l1.4 1.4" />
            <path d="M11.1 11.1l1.4 1.4" />
            <path d="M12.5 3.5l-1.4 1.4" />
            <path d="M4.9 11.1l-1.4 1.4" />
          </svg>
        </template>光影包
      </DownloadTab>
    </nav>

    <div ref="bodyEl" class="dl-body">
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

        <div v-else-if="currentTab === 'shader'" key="shader" class="dl-page mod-dl-page">
          <ShaderSearchBar />
          <ShaderResultList />
        </div>

        <div v-else-if="currentTab === 'resourcepack'" key="resourcepack" class="dl-page mod-dl-page">
          <ResourcepackSearchBar />
          <ResourcepackResultList />
        </div>

        <div v-else-if="currentTab === 'datapack'" key="datapack" class="dl-page mod-dl-page">
          <DatapackSearchBar />
          <DatapackResultList />
        </div>

        <div v-else-if="currentTab === 'modpack'" key="modpack" class="dl-page mod-dl-page">
          <ModpackSearchBar />
          <ModpackResultList />
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
