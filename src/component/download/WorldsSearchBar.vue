<script setup lang="ts">
// 地图存档搜索表单卡片 → CommunitySearchBar；CurseForge 单源（Modrinth 无 worlds project_type），不显示来源筛选。
import { useI18n } from "vue-i18n";
import { worldsSearch } from "../../store/community/categorySearch";
import CommunitySearchBar from "./CommunitySearchBar.vue";

const { t } = useI18n();
</script>

<template>
  <CommunitySearchBar
    :search="worldsSearch"
    :title="t('download.search.title', { category: t('download.categories.worlds') })"
    :show-source="false"
    id-prefix="w"
  />
</template>
