<script lang="ts">
// 工具 tab 状态：模块级持久（路由切换不丢），对齐 settingsTab 先例。
// 放普通 <script> 块：<script setup> 不允许 export。
import { ref } from "vue";

export type ToolsTab = "status" | "motd" | "give" | "potion" | "firework" | "villager" | "online";

export const toolsTab = ref<ToolsTab>("status");
</script>

<script setup lang="ts">
// 工具页容器：SideNav 导航 + 各工具编排（业务在组件内，本文件不做工具逻辑）。
// 工具：服务器状态 / MOTD / /give / 药水 / 烟花 / 村民交易 / 联机；后续工具加条目即可。
import { Server, Type, Link2, Sword, FlaskConical, Rocket, Store } from "@lucide/vue";
import ServerStatusTool from "../component/tools/ServerStatusTool.vue";
import MotdGeneratorTool from "../component/tools/MotdGeneratorTool.vue";
import GiveGeneratorTool from "../component/tools/GiveGeneratorTool.vue";
import PotionGeneratorTool from "../component/tools/PotionGeneratorTool.vue";
import FireworkGeneratorTool from "../component/tools/FireworkGeneratorTool.vue";
import VillagerGeneratorTool from "../component/tools/VillagerGeneratorTool.vue";
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
        { key: 'give', label: '/give 生成器', icon: Sword },
        { key: 'potion', label: '药水', icon: FlaskConical },
        { key: 'firework', label: '烟花', icon: Rocket },
        { key: 'villager', label: '村民交易', icon: Store },
        { key: 'online', label: '联机', icon: Link2 },
      ]"
    >
      <ServerStatusTool v-if="toolsTab === 'status'" key="status" />
      <MotdGeneratorTool v-else-if="toolsTab === 'motd'" key="motd" />
      <GiveGeneratorTool v-else-if="toolsTab === 'give'" key="give" />
      <PotionGeneratorTool v-else-if="toolsTab === 'potion'" key="potion" />
      <FireworkGeneratorTool v-else-if="toolsTab === 'firework'" key="firework" />
      <VillagerGeneratorTool v-else-if="toolsTab === 'villager'" key="villager" />
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
