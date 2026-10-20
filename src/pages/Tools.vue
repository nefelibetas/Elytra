<script lang="ts">
// 工具 tab 状态：模块级持久（路由切换不丢）；须放普通 <script>（<script setup> 不允许 export）
import { ref } from "vue";

export type ToolsTab =
  | "status"
  | "motd"
  | "command"
  | "give"
  | "entity"
  | "skinpaint"
  | "download"
  | "online";

export const toolsTab = ref<ToolsTab>("status");
</script>

<script setup lang="ts">
// 页面容器：SideNav + 各工具编排（业务在组件内）
import { useI18n } from "vue-i18n";
import { Server, Type, Link2, Sword, Download, Paintbrush, Terminal, PawPrint } from "@lucide/vue";
import ServerStatusTool from "../component/tools/ServerStatusTool.vue";
import MotdGeneratorTool from "../component/tools/MotdGeneratorTool.vue";
import CommandGeneratorTool from "../component/tools/CommandGeneratorTool.vue";
import EntityNbtTool from "../component/tools/EntityNbtTool.vue";
import GiveGeneratorTool from "../component/tools/GiveGeneratorTool.vue";
import SkinPainterTool from "../component/tools/SkinPainterTool.vue";
import DownloadTool from "../component/tools/DownloadTool.vue";
import OnlineTool from "../component/tools/OnlineTool.vue";
import ScrollTopFab from "../component/common/ScrollTopFab.vue";
import SideNav from "../component/common/SideNav.vue";

const { t } = useI18n();
</script>

<template>
  <div class="tools">
    <SideNav
      v-model="toolsTab"
      :title="t('tools.tabs.title')"
      :tabs="[
        { key: 'status', label: t('tools.tabs.status'), icon: Server },
        { key: 'motd', label: t('tools.tabs.motd'), icon: Type },
        { key: 'give', label: t('tools.tabs.give'), icon: Sword },
        { key: 'entity', label: t('tools.tabs.entity'), icon: PawPrint },
        { key: 'command', label: t('tools.tabs.command'), icon: Terminal },
        { key: 'skinpaint', label: t('tools.tabs.skinpaint'), icon: Paintbrush },
        { key: 'download', label: t('tools.tabs.download'), icon: Download },
        { key: 'online', label: t('tools.tabs.online'), icon: Link2 },
      ]"
    >
      <ServerStatusTool v-if="toolsTab === 'status'" key="status" />
      <MotdGeneratorTool v-else-if="toolsTab === 'motd'" key="motd" />
      <GiveGeneratorTool v-else-if="toolsTab === 'give'" key="give" />
      <EntityNbtTool v-else-if="toolsTab === 'entity'" key="entity" />
      <CommandGeneratorTool v-else-if="toolsTab === 'command'" key="command" />
      <SkinPainterTool v-else-if="toolsTab === 'skinpaint'" key="skinpaint" />
      <DownloadTool v-else-if="toolsTab === 'download'" key="download" />
      <OnlineTool v-else key="online" />
    </SideNav>

    <ScrollTopFab />
  </div>
</template>

<style scoped>
.tools {
  height: 100%;
}
</style>
