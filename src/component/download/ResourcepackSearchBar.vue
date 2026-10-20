<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { resourcepackSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";

const { t } = useI18n();
</script>

<template>
  <CommunitySearchBar
    :search="resourcepackSearch"
    :title="t('download.search.title', { category: t('download.categories.resourcepack') })"
    id-prefix="rp"
  />
</template>
