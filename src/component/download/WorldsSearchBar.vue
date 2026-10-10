<script setup lang="ts">
// 地图存档搜索表单卡片：薄适配层 → CommunitySearchBar。
// CurseForge 单源（Modrinth 无 worlds project_type），不显示来源筛选。
import { worldsSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="worldsSearch" title="搜索地图存档" :show-source="false" id-prefix="w" />
</template>
