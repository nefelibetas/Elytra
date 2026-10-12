<script setup lang="ts">
// 版本设置页容器：左侧导航 + 各 tab 编排（业务在管理组件内）
import { computed, ref } from "vue";
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
  loadMods, loadModsQuick,
  loadWorlds, loadResourceDetailed, loadResourceQuick,
} from "../store/task/versionDetail";

type TabKey = "mods" | "saves" | "resourcepacks" | "shaderpacks" | "datapacks" | "export" | "logs" | "server" | "auto_tasks" | "misc";

const tabs: { key: TabKey; label: string; icon: Component }[] = [
  { key: "mods", label: "Mod 管理", icon: Puzzle },
  { key: "saves", label: "存档管理", icon: Map },
  { key: "resourcepacks", label: "资源包管理", icon: Palette },
  { key: "shaderpacks", label: "光影包管理", icon: Sun },
  { key: "datapacks", label: "数据包管理", icon: Database },
  { key: "server", label: "服务器", icon: Server },
  { key: "auto_tasks", label: "自动任务", icon: Play },
  { key: "misc", label: "图标", icon: ImagePlus },
  { key: "logs", label: "游戏日志", icon: FileText },
  { key: "export", label: "导出", icon: Upload },
];

const activeTab = ref<TabKey>("mods");

const route = useRoute();

/** 游戏 id（版本目录名），由路由 query 传入。 */
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
      <!-- 右侧：当前 tab 内容（切换淡入淡出） -->
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
