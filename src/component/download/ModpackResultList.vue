<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { modpackSearch, modpackTotalPages } from "../../store/community/categorySearch";
import { Archive } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";

const { t } = useI18n();
</script>

<template>
  <CommunityResultList
    :search="modpackSearch"
    :total-pages="modpackTotalPages"
    :label="t('download.categories.modpack')"
    :empty-icon="Archive"
  />
</template>
