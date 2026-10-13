<script setup lang="ts">
import { modpackSearch, modpackTotalPages } from "../../store/community/categorySearch";
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
