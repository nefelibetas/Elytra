<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { shaderSearch, shaderTotalPages } from "../../store/community/categorySearch";
import { Sun } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";

const { t } = useI18n();
</script>

<template>
  <CommunityResultList
    :search="shaderSearch"
    :total-pages="shaderTotalPages"
    :label="t('download.categories.shader')"
    :empty-icon="Sun"
  />
</template>
