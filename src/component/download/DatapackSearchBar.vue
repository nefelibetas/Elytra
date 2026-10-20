<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { datapackSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";

const { t } = useI18n();
</script>

<template>
  <CommunitySearchBar
    :search="datapackSearch"
    :title="t('download.search.title', { category: t('download.categories.datapack') })"
    id-prefix="da"
  />
</template>
