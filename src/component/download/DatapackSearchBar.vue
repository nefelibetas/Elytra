<script setup lang="ts">
// 数据包搜索表单卡片：薄适配层 → CommunitySearchBar。
import { datapackSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="datapackSearch" title="搜索数据包" id-prefix="da" />
</template>
