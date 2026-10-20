<script setup lang="ts">
// 页面容器：左侧导航 + 各 tab 编排；业务在管理组件内
import { computed, onMounted, ref } from "vue";
import type { Component } from "vue";
import { useRoute } from "vue-router";
import { Puzzle, Map, Palette, Sun, Database, Upload, ImagePlus, FileText, Server, Play } from "@lucide/vue";
import SideNav from "../component/common/SideNav.vue";
import VersionSetupMisc from "../component/version-setup/VersionSetupMisc.vue";
import AutoTasksEditor from "../component/settings/AutoTasksEditor.vue";
import VersionSetupLogs from "../component/version-setup/VersionSetupLogs.vue";
import VersionSetupServer from "../component/version-setup/VersionSetupServer.vue";
import VersionSetupExport from "../component/version-setup/VersionSetupExport.vue";
import VersionSetupMods from "../component/version-setup/VersionSetupMods.vue";
import VersionSetupSaves from "../component/version-setup/VersionSetupSaves.vue";
import VersionSetupResource from "../component/version-setup/VersionSetupResource.vue";
import {
  watchVersionDir,
  loadMods, loadModsQuick,
  loadWorlds, loadResourceDetailed, loadResourceQuick,
} from "../store/task/versionDetail";
import { useI18n } from "vue-i18n";

type TabKey = "mods" | "saves" | "resourcepacks" | "shaderpacks" | "datapacks" | "export" | "logs" | "server" | "auto_tasks" | "misc";

const { t } = useI18n();

const tabs = computed<{ key: TabKey; label: string; icon: Component }[]>(() => [
  { key: "mods", label: t("settings.version.setup.tabs.mods"), icon: Puzzle },
  { key: "saves", label: t("settings.version.setup.tabs.saves"), icon: Map },
  { key: "resourcepacks", label: t("settings.version.setup.tabs.resourcepacks"), icon: Palette },
  { key: "shaderpacks", label: t("settings.version.setup.tabs.shaderpacks"), icon: Sun },
  { key: "datapacks", label: t("settings.version.setup.tabs.datapacks"), icon: Database },
  { key: "server", label: t("settings.version.setup.tabs.server"), icon: Server },
  { key: "auto_tasks", label: t("settings.version.setup.tabs.autoTasks"), icon: Play },
  { key: "misc", label: t("settings.version.setup.tabs.misc"), icon: ImagePlus },
  { key: "logs", label: t("settings.version.setup.tabs.logs"), icon: FileText },
  { key: "export", label: t("settings.version.setup.tabs.export"), icon: Upload },
]);

const activeTab = ref<TabKey>("mods");

const route = useRoute();

/** 版本目录名，由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

/** 各管理 tab 的刷新动作（侧边 hover 刷新按钮，强制重载并更新缓存标记）。 */
const REFRESHERS: Record<TabKey, () => void> = {
  mods: () => {
    void loadMods(versionName.value, true);
    void loadModsQuick(versionName.value);
  },
  saves: () => {
    void loadWorlds(versionName.value);
  },
  resourcepacks: () => {
    void loadResourceDetailed(versionName.value, "resourcepacks");
    void loadResourceQuick(versionName.value, "resourcepacks");
  },
  shaderpacks: () => {
    void loadResourceDetailed(versionName.value, "shaderpacks");
    void loadResourceQuick(versionName.value, "shaderpacks");
  },
  datapacks: () => {
    void loadResourceDetailed(versionName.value, "datapacks");
    void loadResourceQuick(versionName.value, "datapacks");
  },
  export: () => {},
  logs: () => {},
  server: () => {},
  auto_tasks: () => {},
  misc: () => {},
};

onMounted(() => {
  // 聚焦当前版本：后端据此监听 versions/{name} 的一级资源子目录
  if (versionName.value) watchVersionDir(versionName.value);
});
</script>

<template>
  <div class="vs">
    <SideNav
      v-model="activeTab"
      :tabs="tabs"
      :width="240"
      refreshable
      internal-scroll
      :no-refresh-keys="['export']"
      @refresh="REFRESHERS[$event as TabKey]()"
    >
      <VersionSetupMods v-if="activeTab === 'mods'" key="mods" />

      <VersionSetupSaves v-else-if="activeTab === 'saves'" key="saves" />

      <VersionSetupResource
        v-else-if="activeTab === 'resourcepacks'"
        key="resourcepacks"
        kind="resourcepacks"
      />

      <VersionSetupResource
        v-else-if="activeTab === 'shaderpacks'"
        key="shaderpacks"
        kind="shaderpacks"
      />

      <VersionSetupResource
        v-else-if="activeTab === 'datapacks'"
        key="datapacks"
        kind="datapacks"
      />

      <VersionSetupExport v-else-if="activeTab === 'export'" key="export" />

      <VersionSetupLogs v-else-if="activeTab === 'logs'" key="logs" />

      <VersionSetupServer v-else-if="activeTab === 'server'" key="server" />

      <AutoTasksEditor v-else-if="activeTab === 'auto_tasks'" key="auto_tasks" />

      <VersionSetupMisc v-else key="misc" />
    </SideNav>
  </div>
</template>

<style scoped>
.vs {
  height: 100%;
}
</style>
