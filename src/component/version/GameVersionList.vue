<script setup lang="ts">
// 版本选择页右侧：当前目录版本列表（按 ModLoader/原版分类卡片）+ 删除版本 dialog
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { Settings2, Trash2 } from "@lucide/vue";
import AlertDialog from "../common/AlertDialog.vue";
import VersionCard from "../download/VersionCard.vue";
import VersionItem from "../download/VersionItem.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";
import {
  versions, versionsLoading, versionsError, expanded, allVersions, searchQuery,
  pickVersion, openDelete,
  deleteDialog, deleteTarget, confirmDeleteVersion,
} from "../../store/config/gameDirs";
import { loadVersionIcon, versionIcons } from "../../store/task/versionIcon";
import { watch } from "vue";

// 整合包安装的版本用平台项目图标（懒加载，缺失回退 loader 图标）。
watch(
  () => versions.value.map(v => v.name),
  names => names.forEach(loadVersionIcon),
  { immediate: true },
);

const router = useRouter();
const { t } = useI18n();

function selectAndBack(v: string) {
  pickVersion(v);
  router.push("/");
}

function goSetup(v: string) {
  router.push({ path: "/version-setup", query: { name: v } });
}

function goDownload() {
  router.push("/download");
}
</script>

<template>
  <div class="vs-right">
    <LoadingSpinner v-if="versionsLoading" :text="t('common.status.loading')" />
    <ErrorState v-else-if="versionsError" :text="versionsError" />
    <EmptyState v-else-if="versions.length === 0">
      <p>{{ t("settings.version.select.noVersions") }}</p>
      <button class="vs-empty-btn" @click="goDownload">{{ t("launch.action.downloadGame") }}</button>
    </EmptyState>
    <template v-else>
      <div class="vs-search">
        <input
          v-model="searchQuery"
          class="vs-search-input"
          type="text"
          :placeholder="t('settings.version.select.searchPlaceholder')"
          autocomplete="off"
        />
      </div>
      <div v-if="allVersions.length === 0" class="vs-empty">
        <p>{{ t("settings.version.select.noMatch", { query: searchQuery.trim() }) }}</p>
      </div>
      <div v-else class="vs-cards">
        <VersionCard
          :title="t('settings.version.select.allVersions')"
          :count="allVersions.length"
          :expanded="expanded['all']"
          @toggle="expanded['all'] = !expanded['all']"
        >
          <VersionItem
            v-for="v in allVersions"
            :key="v.name"
            :icon-src="versionIcons[v.name] ?? v.icon"
            :version-id="v.name"
            :subtitle="v.subtitle"
            @select="selectAndBack(v.name)"
          >
            <template #action>
              <button
                class="vs-item-setup"
                v-tooltip="t('settings.version.select.versionSetup')"
                @click.stop="goSetup(v.name)"
              >
                <Settings2 :size="15" />
              </button>
              <button
                class="vs-item-del"
                v-tooltip="t('settings.version.select.deleteVersion')"
                @click.stop="openDelete(v.name)"
              >
                <Trash2 :size="15" />
              </button>
            </template>
          </VersionItem>
        </VersionCard>
      </div>
    </template>

    <!-- 删除版本确认 dialog -->
    <AlertDialog v-model="deleteDialog" :title="t('settings.version.select.deleteVersion')">
      <div class="vs-dialog-body">
        <div class="vs-dialog-text">
          {{ t("settings.version.select.deleteConfirm", { name: deleteTarget }) }}
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn danger" @click="confirmDeleteVersion">{{ t("settings.version.setup.common.delete") }}</button>
        <button class="dialog-btn" @click="deleteDialog = false">{{ t("settings.version.setup.common.cancel") }}</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vs-right {
  flex: 1;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface-page);
}

.vs-cards {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.vs-search {
  margin-bottom: 12px;
}

.vs-search-input {
  width: 100%;
  padding: 7px 12px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}

.vs-search-input:focus {
  border-color: var(--color-brand);
}

.vs-search-input::placeholder {
  color: var(--text-tertiary);
}

/* 版本选择页：item 间无分割线 */
.vs-cards :deep(.ver-item + .ver-item) {
  border-top: none;
}

.vs-item-setup {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.vs-item-setup:hover {
  background-color: var(--color-brand-bg);
  color: var(--color-brand);
}

.vs-item-del {
  flex: none;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.vs-item-del:hover {
  background-color: var(--color-danger-bg);
  color: var(--color-danger);
}

.vs-empty {
  min-height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  padding: 24px;
}

.vs-empty p {
  margin: 0;
  font-size: 13px;
  color: var(--text-tertiary);
}

.vs-empty-btn {
  padding: 8px 22px;
  border: none;
  border-radius: var(--radius-md);
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
  font-size: 13.5px;
  font-weight: 600;
  cursor: pointer;
  /* 渐变色切换不插值（会闪变），只过渡 transform/阴影 */
  transition: transform 0.15s, box-shadow 0.15s;
}

.vs-empty-btn:hover {
  background: var(--color-brand-hover);
  box-shadow: var(--shadow-md);
}

.vs-empty-btn:active {
  transform: scale(0.97);
}



.vs-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.vs-dialog-text {
  font-size: 13px;
  color: var(--text-secondary);
  line-height: 1.6;
}


</style>
