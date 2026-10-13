<script setup lang="ts">
import { shaderSearch, shaderTotalPages } from "../../store/community/categorySearch";
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
