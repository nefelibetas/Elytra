<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { resourcepackSearch, resourcepackTotalPages } from "../../store/community/categorySearch";
import { Palette } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";

const { t } = useI18n();
</script>

<template>
  <CommunityResultList
    :search="resourcepackSearch"
    :total-pages="resourcepackTotalPages"
    :label="t('download.categories.resourcepack')"
    :empty-icon="Palette"
  />
</template>
