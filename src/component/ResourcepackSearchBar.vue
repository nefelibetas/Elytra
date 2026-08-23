<script setup lang="ts">
// 资源包搜索表单卡片：薄适配层 → CommunitySearchBar。
import { resourcepackSearch } from "../store/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="resourcepackSearch" title="搜索资源包" id-prefix="rp" />
</template>
