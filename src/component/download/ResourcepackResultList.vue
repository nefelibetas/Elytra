<script setup lang="ts">
// 资源包搜索结果列表：薄适配层 → CommunityResultList。
import { resourcepackSearch, resourcepackTotalPages } from "../../store/categorySearch";
import { Palette } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";
</script>

<template>
  <CommunityResultList
    :search="resourcepackSearch"
    :total-pages="resourcepackTotalPages"
    label="资源包"
    :empty-icon="Palette"
  />
</template>
