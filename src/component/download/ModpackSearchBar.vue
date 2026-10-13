<script setup lang="ts">
import { ref } from "vue";
import { modpackSearch } from "../../store/community/categorySearch";
import { importLocalModpack } from "../../store/task/modpack";
import CommunitySearchBar from "./CommunitySearchBar.vue";

/** 本地导入：选 .mrpack/.zip → install_modpack（后端自动嗅探 Modrinth/CurseForge 清单）。 */
const importing = ref(false);
async function importLocal() {
  importing.value = true;
  try {
    await importLocalModpack();
  } finally {
    importing.value = false;
  }
}
</script>

<template>
  <CommunitySearchBar :search="modpackSearch" title="搜索整合包" id-prefix="modpack">
    <template #prepend-actions>
      <button class="cs-btn" :disabled="importing" @click="importLocal">导入本地</button>
    </template>
  </CommunitySearchBar>
</template>

<style scoped>
/* 插槽内容编译于本组件作用域，通用组件的 scoped .cs-btn 不命中，故复制样式保持一致 */
.cs-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.cs-btn:hover { border-color: var(--color-brand); }
.cs-btn:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
