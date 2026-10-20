<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { worldsSearch, worldsTotalPages } from "../../store/community/categorySearch";
import { Map } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";

const { t } = useI18n();
</script>

<template>
  <CommunityResultList
    :search="worldsSearch"
    :total-pages="worldsTotalPages"
    :label="t('download.categories.worlds')"
    :empty-icon="Map"
  />
</template>
