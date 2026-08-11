<script setup lang="ts">
// 版本选择页容器：左列目录管理 + 右列版本列表（业务在 store/gameDirs 与两个组件中）
import { onMounted } from "vue";
import GameDirList from "../component/GameDirList.vue";
import GameVersionList from "../component/GameVersionList.vue";
import ScrollTopFab from "../component/ScrollTopFab.vue";
import { loadDirs, loadVersions } from "../store/gameDirs";

onMounted(async () => {
  await loadDirs();
  await loadVersions();
});
</script>

<template>
  <div class="vs">
    <GameDirList />
    <GameVersionList />
    <ScrollTopFab />
  </div>
</template>

<style scoped>
.vs {
  display: flex;
  height: 100%;
}
</style>
