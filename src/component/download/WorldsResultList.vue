<script setup lang="ts">
import { worldsSearch, worldsTotalPages } from "../../store/community/categorySearch";
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
