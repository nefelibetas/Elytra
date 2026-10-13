<script setup lang="ts">
import { shaderSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="shaderSearch" title="搜索光影包" id-prefix="sh" />
</template>
