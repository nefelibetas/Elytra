<script setup lang="ts">
import { datapackSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";
</script>

<template>
  <CommunitySearchBar :search="datapackSearch" title="搜索数据包" id-prefix="da" />
</template>
