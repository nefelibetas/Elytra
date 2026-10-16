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
</script>

<template>
  <div class="tools">
    <SideNav
      v-model="toolsTab"
      title="工具"
      :tabs="[
        { key: 'status', label: '服务器状态', icon: Server },
        { key: 'motd', label: 'MOTD 生成器', icon: Type },
        { key: 'give', label: '物品 NBT 生成', icon: Sword },
        { key: 'entity', label: '生物 NBT 生成', icon: PawPrint },
        { key: 'command', label: '指令生成器', icon: Terminal },
        { key: 'skinpaint', label: '皮肤绘画', icon: Paintbrush },
        { key: 'download', label: '多线程下载', icon: Download },
        { key: 'online', label: '联机', icon: Link2 },
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
