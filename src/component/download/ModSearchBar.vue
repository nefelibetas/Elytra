<script setup lang="ts">
// Mod 搜索表单卡片：薄适配层 → CommunitySearchBar（Mod 特有：来源与名称同行 + 加载器筛选）。
import { communitySearch } from "../../store/communitySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar
    :search="communitySearch"
    title="搜索 Mod"
    show-source
    show-loader
    id-prefix="mod"
  />
</template>
