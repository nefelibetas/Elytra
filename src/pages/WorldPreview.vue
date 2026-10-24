<script setup lang="ts">
// 世界预览页面：解析路由中的版本/存档名，视图实现见 WorldPreviewView。
import { computed } from "vue";
import { useRoute } from "vue-router";

import WorldPreviewView from "../component/map/WorldPreviewView.vue";

const route = useRoute();
const versionName = computed(() => String(route.query.version ?? ""));
const worldName = computed(() => String(route.query.world ?? ""));
</script>

<template>
  <div class="world-preview-page">
    <WorldPreviewView :version-name="versionName" :world-name="worldName" />
  </div>
</template>

<style scoped>
.world-preview-page {
  height: 100%;
  min-height: 0;
}
</style>
