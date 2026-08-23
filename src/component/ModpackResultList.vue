<script setup lang="ts">
// 整合包搜索结果列表：薄适配层 → CommunityResultList。
import { modpackSearch, modpackTotalPages } from "../store/categorySearch";
import { Archive } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";
</script>

<template>
  <CommunityResultList
    :search="modpackSearch"
    :total-pages="modpackTotalPages"
    label="整合包"
    :empty-icon="Archive"
  />
</template>
