<script setup lang="ts">
import { resourcepackSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="resourcepackSearch" title="搜索资源包" id-prefix="rp" />
</template>
