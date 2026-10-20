<script setup lang="ts">
// Mod 搜索表单卡片：薄适配层 → CommunitySearchBar（Mod 特有：来源与名称同行 + 加载器筛选）。
import { useI18n } from "vue-i18n";
import { communitySearch } from "../../store/community/communitySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";

const { t } = useI18n();
</script>

<template>
  <CommunitySearchBar
    :search="communitySearch"
    :title="t('download.search.title', { category: t('download.categories.mod') })"
    show-source
    show-loader
    id-prefix="mod"
  />
</template>
