<script setup lang="ts">
import { resourcepackSearch, resourcepackTotalPages } from "../../store/community/categorySearch";
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
