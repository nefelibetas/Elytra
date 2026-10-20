<script setup lang="ts">
// 版本选择页左侧：游戏目录列表 + 右键菜单 + 添加游戏目录 + 重命名 dialog
import { onMounted, onUnmounted } from "vue";
import { useI18n } from "vue-i18n";
import { Ellipsis, FolderPlus } from "@lucide/vue";
import MenuPopover from "../common/MenuPopover.vue";
import AlertDialog from "../common/AlertDialog.vue";
import { errMsg } from "../../utils/error";
import { importLocalModpack } from "../../store/task/modpack";
import {
  dirs, activePath, dirsError, itemMenu,
  selectDir, openItemMenu, onWindowClick, closeItemMenu,
  menuOpen, menuRefresh, menuRemove, menuRename, confirmRename,
  renameDialog, renameInput, renameError,
  addFolder, confirmAdd, addDialog, addInput, addError,
} from "../../store/config/gameDirs";

onMounted(() => window.addEventListener("click", onWindowClick));
onUnmounted(() => window.removeEventListener("click", onWindowClick));

const { t } = useI18n();

/** 导入整合包：先切到该目录（整合包装进「当前游戏目录」），再选文件提交安装。 */
async function menuImportPack() {
  const d = itemMenu.value.dir;
  closeItemMenu();
  if (!d) return;
  try {
    await selectDir(d);
  } catch (e) {
    dirsError.value = errMsg(e);
    return;
  }
  await importLocalModpack();
}
</script>

<template>
  <div class="vs-left">
    <div class="vs-group-label">{{ t("settings.version.select.gameList") }}</div>
    <div class="vs-dir-list">
      <!-- 无目录：macOS/Linux 无平台默认，Windows 仅只读安装位置为空 -->
      <div v-if="!dirs.length" class="vs-dir-empty">
        {{ t("settings.version.select.emptyTitle") }}<br />
        <span class="vs-dir-empty-hint">{{ t("settings.version.select.emptyHint") }}</span>
      </div>
      <div
        v-for="d in dirs"
        :key="d.path"
        class="vs-dir-item"
        :class="{ active: d.path === activePath, 'menu-open': itemMenu.visible && itemMenu.dir?.path === d.path }"
        @click="selectDir(d)"
        @contextmenu.prevent="openItemMenu($event, d)"
      >
        <div class="vs-dir-main">
          <div class="vs-dir-title">{{ d.name }}</div>
          <div class="vs-dir-sub" v-tooltip="d.path">{{ d.path }}</div>
        </div>
        <button
          class="vs-dir-more"
          v-tooltip="t('settings.version.select.moreActions')"
          @click.stop="openItemMenu($event, d)"
        >
          <Ellipsis :size="15" :fill="'currentColor'" :stroke="'none'" />
        </button>
      </div>
    </div>

    <div class="vs-group-label">{{ t("settings.version.select.addOrImport") }}</div>
    <div class="vs-add-list">
      <div class="vs-add-item" @click="addFolder">
        <FolderPlus :size="14" />
        <span>{{ t("settings.version.select.addExistingDir") }}</span>
      </div>
    </div>
    <div v-if="dirsError" class="vs-err">{{ dirsError }}</div>

    <!-- 目录项菜单 -->
    <MenuPopover
      :visible="itemMenu.visible"
      :x="itemMenu.x"
      :y="itemMenu.y"
      min-width="120px"
    >
      <div class="menu-item" @click="menuRename">{{ t("settings.version.select.rename") }}</div>
      <div class="menu-item" @click="menuOpen">{{ t("settings.version.select.open") }}</div>
      <div class="menu-item" @click="menuRemove">{{ t("settings.version.select.delete") }}</div>
      <div class="menu-item" @click="menuRefresh">{{ t("settings.version.select.refresh") }}</div>
      <div class="menu-item" @click="menuImportPack">{{ t("settings.version.select.importModpack") }}</div>
    </MenuPopover>

    <!-- 重命名 dialog -->
    <AlertDialog v-model="renameDialog" :title="t('settings.version.select.renameTitle')">
      <div class="vs-dialog-body">
        <div class="vs-dialog-label">{{ t("settings.version.select.renameLabel") }}</div>
        <input
          v-model="renameInput"
          class="vs-input"
          type="text"
          maxlength="32"
          @keydown.enter="confirmRename"
          autocomplete="off"
        />
        <div v-if="renameError" class="vs-err">{{ renameError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmRename">{{ t("settings.version.setup.common.confirm") }}</button>
        <button class="dialog-btn" @click="renameDialog = false">{{ t("settings.version.setup.common.cancel") }}</button>
      </template>
    </AlertDialog>

    <!-- 添加已有文件夹 dialog -->
    <AlertDialog v-model="addDialog" :title="t('settings.version.select.addTitle')">
      <div class="vs-dialog-body">
        <div class="vs-dialog-label">{{ t("settings.version.select.renameLabel") }}</div>
        <input
          v-model="addInput"
          class="vs-input"
          type="text"
          maxlength="32"
          @keydown.enter="confirmAdd"
          autocomplete="off"
        />
        <div v-if="addError" class="vs-err">{{ addError }}</div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmAdd">{{ t("settings.version.setup.common.confirm") }}</button>
        <button class="dialog-btn" @click="addDialog = false">{{ t("settings.version.setup.common.cancel") }}</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.vs-left {
  flex: 0 0 240px;
  min-width: 220px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
}

.vs-group-label {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.8px;
  padding: 8px 8px 6px;
}

.vs-dir-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.vs-dir-empty {
  padding: 12px;
  font-size: 12px;
  line-height: 1.7;
  color: var(--text-secondary);
}

.vs-dir-empty-hint {
  font-size: 11px;
  color: var(--text-tertiary);
}

.vs-dir-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background 0.12s;
}

.vs-dir-item:hover,
.vs-dir-item.menu-open {
  background: var(--surface-hover);
}

.vs-dir-item.active {
  background: var(--color-brand-bg);
}

.vs-dir-more {
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
  opacity: 0;
  visibility: hidden; /* 隐藏态移出命中测试（WebView2 pointer-events 过渡不可靠） */
  transition: opacity 0.12s, visibility 0.12s;
}

.vs-dir-item:hover .vs-dir-more,
.vs-dir-item.menu-open .vs-dir-more {
  opacity: 1;
  visibility: visible;
}

.vs-dir-more:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.vs-dir-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vs-dir-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.vs-dir-sub {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.vs-add-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.vs-add-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.12s;
}

.vs-add-item:hover {
  background: var(--surface-hover);
  color: var(--color-brand);
}

.vs-err {
  font-size: 12px;
  color: var(--color-danger);
  padding: 6px 8px;
}

/* 目录项菜单：壳样式在 MenuPopover */


.vs-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.vs-dialog-label {
  font-size: 13px;
  color: var(--text-secondary);
}

.vs-input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}

.vs-input:focus {
  border-color: var(--color-brand);
}


</style>
