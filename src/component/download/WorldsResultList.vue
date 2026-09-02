<script setup lang="ts">
// 地图存档搜索结果列表：薄适配层 → CommunityResultList。
import { worldsSearch, worldsTotalPages } from "../../store/categorySearch";
import { Map } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";
</script>

<template>
  <CommunityResultList
    :search="worldsSearch"
    :total-pages="worldsTotalPages"
    label="地图存档"
    :empty-icon="Map"
  />
</template>
