<script setup lang="ts">
// 数据包搜索结果列表：薄适配层 → CommunityResultList。
import { datapackSearch, datapackTotalPages } from "../../store/categorySearch";
import { Database } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";
</script>

<template>
  <CommunityResultList
    :search="datapackSearch"
    :total-pages="datapackTotalPages"
    :empty-icon="Database"
    label="数据包"
  />
</template>
