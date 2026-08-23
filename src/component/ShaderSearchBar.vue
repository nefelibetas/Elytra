<script setup lang="ts">
// 光影包搜索表单卡片：薄适配层 → CommunitySearchBar。
import { shaderSearch } from "../store/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="shaderSearch" title="搜索光影包" id-prefix="sh" />
</template>
