<script setup lang="ts">
// Mod 搜索结果列表：薄适配层 → CommunityResultList（详情回填当前加载器）。
import { useI18n } from "vue-i18n";
import { communitySearch, communityTotalPages } from "../../store/community/communitySearch";
import { Puzzle } from "@lucide/vue";
import CommunityResultList from "./CommunityResultList.vue";

const { t } = useI18n();
</script>

<template>
  <CommunityResultList
    :search="communitySearch"
    :total-pages="communityTotalPages"
    :label="t('download.categories.mod')"
    :empty-icon="Puzzle"
    :detail-loader="communitySearch.loader"
  />
</template>
