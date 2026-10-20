<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { datapackSearch, datapackTotalPages } from "../../store/community/categorySearch";
import { Database } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";

const { t } = useI18n();
</script>

<template>
  <CommunityResultList
    :search="datapackSearch"
    :total-pages="datapackTotalPages"
    :empty-icon="Database"
    :label="t('download.categories.datapack')"
  />
</template>
