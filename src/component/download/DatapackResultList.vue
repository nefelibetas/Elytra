<script setup lang="ts">
import { datapackSearch, datapackTotalPages } from "../../store/community/categorySearch";
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
