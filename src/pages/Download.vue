<script setup lang="ts">
// 资源中心页容器：导航 + 各 tab 内容编排（具体逻辑在 store/组件中，本文件不做业务）
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from "vue";
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
import { loadDownloadConfig } from "../store/downloaderConfig";
import { downloadTabState } from "../store/downloadTab";
import { Box, Puzzle, Archive, CodeXml, Layers, Sun } from "@lucide/vue";
import {
  shaderSearch,
  resourcepackSearch,
  datapackSearch,
  modpackSearch,
} from "../store/categorySearch";

const tab = computed(() => downloadTabState.current);
/** 右侧滚动容器（切 tab 时复位滚动位置） */
const bodyEl = ref<HTMLElement>();

onMounted(async () => {
  vdState.fetchManifest();
  // 加载下载配置（翻译显示模式等）：保证社区搜索前配置已从持久化读入
  loadDownloadConfig();
  // 预加载社区 tab 热门（对齐 PCL2）：进入下载页即并发请求，切 tab 秒开；
  // ensureAutoSearch 幂等（模块级标志），watch 切 tab 时不会重复请求
  modSearch.ensureAutoSearch();
  shaderSearch.ensureAutoSearch();
  resourcepackSearch.ensureAutoSearch();
  datapackSearch.ensureAutoSearch();
  modpackSearch.ensureAutoSearch();
  // 从详情页返回时恢复该 tab 的滚动位置（组件重建，DOM 需先渲染完成）
  await nextTick();
  const el = bodyEl.value;
  if (el) el.scrollTop = downloadTabState.scrollTop[downloadTabState.current];
});

onBeforeUnmount(() => {
  const el = bodyEl.value;
  if (el) downloadTabState.scrollTop[downloadTabState.current] = el.scrollTop;
});

// 首次进入社区 tab 自动搜索热门（预加载已触发时直接跳过）；切 tab 滚动复位到最上方
watch(tab, (t) => {
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
      <DownloadTab :active="tab === 'vanilla'" @click="downloadTabState.current = 'vanilla'" @refresh="vdState.refresh()">
        <template #icon>
          <Box :size="16" :stroke-width="1.4" />
        </template>
        原版游戏
      </DownloadTab>
      <div class="nav-divider"><span>社区资源</span></div>
      <DownloadTab :active="tab === 'mod'" @click="downloadTabState.current = 'mod'" @refresh="modSearch.reset()">
        <template #icon>
          <Puzzle :size="16" :stroke-width="1.4" />
        </template>Mod
      </DownloadTab>
      <DownloadTab :active="tab === 'modpack'" @click="downloadTabState.current = 'modpack'" @refresh="modpackSearch.reset()">
        <template #icon>
          <Archive :size="16" :stroke-width="1.4" />
        </template>整合包
      </DownloadTab>
      <DownloadTab :active="tab === 'datapack'" @click="downloadTabState.current = 'datapack'" @refresh="datapackSearch.reset()">
        <template #icon>
          <CodeXml :size="16" :stroke-width="1.4" />
        </template>数据包
      </DownloadTab>
      <DownloadTab :active="tab === 'resourcepack'" @click="downloadTabState.current = 'resourcepack'" @refresh="resourcepackSearch.reset()">
        <template #icon>
          <Layers :size="16" :stroke-width="1.4" />
        </template>资源包
      </DownloadTab>
      <DownloadTab :active="tab === 'shader'" @click="downloadTabState.current = 'shader'" @refresh="shaderSearch.reset()">
        <template #icon>
          <Sun :size="16" :stroke-width="1.4" />
        </template>光影包
      </DownloadTab>
    </nav>

    <div ref="bodyEl" class="dl-body">
      <Transition name="page" mode="out-in">
        <div v-if="tab === 'vanilla' && !vdState.selectedVersion" key="list" class="dl-page">
          <VanillaList />
        </div>

        <div v-else-if="tab === 'vanilla' && vdState.selectedVersion" key="detail" class="dl-page">
          <VanillaDetail />
        </div>

        <div v-else-if="tab === 'mod'" key="mod" class="dl-page mod-dl-page">
          <ModSearchBar />
          <ModResultList />
        </div>

        <div v-else-if="tab === 'shader'" key="shader" class="dl-page mod-dl-page">
          <ShaderSearchBar />
          <ShaderResultList />
        </div>

        <div v-else-if="tab === 'resourcepack'" key="resourcepack" class="dl-page mod-dl-page">
          <ResourcepackSearchBar />
          <ResourcepackResultList />
        </div>

        <div v-else-if="tab === 'datapack'" key="datapack" class="dl-page mod-dl-page">
          <DatapackSearchBar />
          <DatapackResultList />
        </div>

        <div v-else-if="tab === 'modpack'" key="modpack" class="dl-page mod-dl-page">
          <ModpackSearchBar />
          <ModpackResultList />
        </div>

        <div v-else key="other" class="dl-page">
          <h2>{{ tab }}</h2>
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
