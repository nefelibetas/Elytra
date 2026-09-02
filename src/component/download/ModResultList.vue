<script setup lang="ts">
// Mod 搜索结果列表：薄适配层 → CommunityResultList（详情回填当前加载器）。
import { communitySearch, communityTotalPages } from "../../store/communitySearch";
import { Puzzle } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";
</script>

<template>
  <CommunityResultList
    :search="communitySearch"
    :total-pages="communityTotalPages"
    label="Mod"
    :empty-icon="Puzzle"
    :detail-loader="communitySearch.loader"
  />
</template>
