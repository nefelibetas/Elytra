<script setup lang="ts">
// 整合包搜索表单卡片：薄适配层 → CommunitySearchBar；「导入本地」为本类特例，经插槽注入。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { modpackSearch } from "../store/categorySearch";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { registerTaskName } from "../store/download";
import CommunitySearchBar from "./CommunitySearchBar.vue";

/** 本地导入：选 .mrpack/.zip → install_modpack（后端自动嗅探 Modrinth/CurseForge 清单）。 */
const importing = ref(false);
async function importLocal() {
  const path = await open({
    multiple: false,
    filters: [{ name: "整合包", extensions: ["mrpack", "zip"] }],
  });
  if (!path) return;
  importing.value = true;
  try {
    // 展示名取文件名去扩展名（后端计划/任务卡均用该名称）
    const fileName = String(path).split(/[\\/]/).pop() ?? "整合包";
    const name = fileName.replace(/\.(mrpack|zip)$/i, "") || "整合包";
    const taskId = await invoke<string>("install_modpack", {
      request: { name, source: { type: "local", path } },
    });
    registerTaskName(taskId, name);
    showToast("已提交整合包安装（准备 → 游戏本体 → 内容）");
  } catch (e) {
    showToast(errMsg(e));
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
/* 插槽内容编译于本组件作用域，通用组件的 scoped .cs-btn 不命中，此处复制按钮样式保持视觉一致 */
.cs-btn {
  padding: 6px 24px;
  border: 1px solid var(--border-default); border-radius: var(--radius-md);
  background: var(--surface-card); color: var(--text-primary); font-size: 13px; font-weight: 600;
  cursor: pointer; transition: all 0.15s; display: inline-flex; align-items: center; gap: 6px;
}
.cs-btn:hover { border-color: var(--color-brand); }
.cs-btn:disabled { opacity: 0.6; cursor: not-allowed; }
</style>
