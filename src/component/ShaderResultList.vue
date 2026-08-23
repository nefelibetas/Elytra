<script setup lang="ts">
// 光影包搜索结果列表：薄适配层 → CommunityResultList。
import { shaderSearch, shaderTotalPages } from "../store/categorySearch";
import { Sun } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";
</script>

<template>
  <CommunityResultList
    :search="shaderSearch"
    :total-pages="shaderTotalPages"
    label="光影包"
    :empty-icon="Sun"
  />
</template>
