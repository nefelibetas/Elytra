<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { shaderSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";

const { t } = useI18n();
</script>

<template>
  <CommunitySearchBar
    :search="shaderSearch"
    :title="t('download.search.title', { category: t('download.categories.shader') })"
    id-prefix="sh"
  />
</template>
