<script setup lang="ts">
// 资源中心页容器：导航 + 各 tab 内容编排（具体逻辑在 store/组件中，本文件不做业务）
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import DownloadTab from "../component/download/DownloadTab.vue";
import VanillaList from "../component/download/VanillaList.vue";
import VanillaDetail from "../component/download/VanillaDetail.vue";
import ModSearchBar from "../component/download/ModSearchBar.vue";
import ModResultList from "../component/download/ModResultList.vue";
import ShaderSearchBar from "../component/download/ShaderSearchBar.vue";
import ShaderResultList from "../component/download/ShaderResultList.vue";
import WorldsSearchBar from "../component/download/WorldsSearchBar.vue";
import WorldsResultList from "../component/download/WorldsResultList.vue";
import ResourcepackSearchBar from "../component/download/ResourcepackSearchBar.vue";
import ResourcepackResultList from "../component/download/ResourcepackResultList.vue";
import DatapackSearchBar from "../component/download/DatapackSearchBar.vue";
import DatapackResultList from "../component/download/DatapackResultList.vue";
import ModpackSearchBar from "../component/download/ModpackSearchBar.vue";
import ModpackResultList from "../component/download/ModpackResultList.vue";
import ScrollTopFab from "../component/common/ScrollTopFab.vue";
import { vdState } from "../store/versionDownload";
import { communitySearch } from "../store/communitySearch";
import { loadDownloadConfig } from "../store/downloaderConfig";
import { downloadTabState } from "../store/downloadTab";
import { Box, Puzzle, Archive, CodeXml, Layers, Sun, Map } from "@lucide/vue";
import {
  shaderSearch,
  resourcepackSearch,
  datapackSearch,
  modpackSearch,
  worldsSearch,
} from "../store/categorySearch";

const tab = computed(() => downloadTabState.current);
/** 右侧滚动容器（切 tab 时复位滚动位置） */
const bodyEl = ref<HTMLElement>();
const route = useRoute();
const router = useRouter();

// 原版游戏详情态 ↔ 路由 query：进入详情 push ?v=<id>（产生历史项），
// 浏览器原生后退 → query 消失 → 回列表（侧键由 nav.ts goBack 规则处理，
// 此处兜底原生历史/URL 直达场景）
watch(
  () => vdState.selectedVersion?.id,
  (id) => {
    const cur = route.query.v;
    if (id && cur !== id) {
      router.push({ path: "/download", query: { v: id } });
    } else if (!id && cur) {
      router.replace({ path: "/download" });
    }
  },
);

watch(
  () => route.query.v,
  (v) => {
    if (!v && vdState.selectedVersion) {
      vdState.backToList();
    } else if (v && !vdState.selectedVersion) {
      // goForward/URL 直达：恢复详情态
      const meta = vdState.manifest?.versions.find((x) => x.id === v);
      if (meta) vdState.selectVersion(meta);
    }
  },
);

onMounted(async () => {
  vdState.fetchManifest();
  // 加载下载配置（翻译显示模式等）：社区搜索的翻译开关依赖 dlTranslateMode，
  // 必须等配置读入后再发起搜索（否则 en 模式首帧仍请求翻译）
  await loadDownloadConfig();
  // 预加载社区 tab 热门（对齐 PCL2）：进入下载页即并发请求，切 tab 秒开；
  // ensureAutoSearch 幂等（模块级标志），watch 切 tab 时不会重复请求
  communitySearch.ensureAutoSearch();
  shaderSearch.ensureAutoSearch();
  worldsSearch.ensureAutoSearch();
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
  if (t === "mod") communitySearch.ensureAutoSearch();
  if (t === "shader") shaderSearch.ensureAutoSearch();
  if (t === "worlds") worldsSearch.ensureAutoSearch();
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
      <DownloadTab :active="tab === 'mod'" @click="downloadTabState.current = 'mod'" @refresh="communitySearch.reset()">
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
      <DownloadTab :active="tab === 'worlds'" @click="downloadTabState.current = 'worlds'" @refresh="worldsSearch.reset()">
        <template #icon>
          <Map :size="16" :stroke-width="1.4" />
        </template>地图存档
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

        <div v-else-if="tab === 'worlds'" key="worlds" class="dl-page mod-dl-page">
          <WorldsSearchBar />
          <WorldsResultList />
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
