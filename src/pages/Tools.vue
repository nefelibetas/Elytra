<script lang="ts">
// 工具 tab 状态：模块级持久（路由切换不丢）；须放普通 <script>（<script setup> 不允许 export）
import { ref } from "vue";

export type ToolsTab =
  | "status"
  | "motd"
  | "command"
  | "give"
  | "potion"
  | "firework"
  | "villager"
  | "skinpaint"
  | "download"
  | "online";

export const toolsTab = ref<ToolsTab>("status");
</script>

<script setup lang="ts">
// 页面容器：SideNav + 各工具编排（业务在组件内）
import { Server, Type, Link2, Sword, FlaskConical, Rocket, Store, Download, Paintbrush, Terminal } from "@lucide/vue";
import ServerStatusTool from "../component/tools/ServerStatusTool.vue";
import MotdGeneratorTool from "../component/tools/MotdGeneratorTool.vue";
import CommandGeneratorTool from "../component/tools/CommandGeneratorTool.vue";
import GiveGeneratorTool from "../component/tools/GiveGeneratorTool.vue";
import PotionGeneratorTool from "../component/tools/PotionGeneratorTool.vue";
import FireworkGeneratorTool from "../component/tools/FireworkGeneratorTool.vue";
import VillagerGeneratorTool from "../component/tools/VillagerGeneratorTool.vue";
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
        { key: 'command', label: '指令生成器', icon: Terminal },
        { key: 'give', label: '物品 NBT 生成', icon: Sword },
        { key: 'potion', label: '药水', icon: FlaskConical },
        { key: 'firework', label: '烟花', icon: Rocket },
        { key: 'villager', label: '村民交易', icon: Store },
        { key: 'skinpaint', label: '皮肤绘画', icon: Paintbrush },
        { key: 'download', label: '多线程下载', icon: Download },
        { key: 'online', label: '联机', icon: Link2 },
      ]"
    >
      <ServerStatusTool v-if="toolsTab === 'status'" key="status" />
      <MotdGeneratorTool v-else-if="toolsTab === 'motd'" key="motd" />
      <CommandGeneratorTool v-else-if="toolsTab === 'command'" key="command" />
      <GiveGeneratorTool v-else-if="toolsTab === 'give'" key="give" />
      <PotionGeneratorTool v-else-if="toolsTab === 'potion'" key="potion" />
      <FireworkGeneratorTool v-else-if="toolsTab === 'firework'" key="firework" />
      <VillagerGeneratorTool v-else-if="toolsTab === 'villager'" key="villager" />
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
