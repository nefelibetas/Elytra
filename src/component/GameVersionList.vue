<script setup lang="ts">
// 版本选择页右侧：当前目录版本列表（按 ModLoader/原版分类卡片）+ 删除版本 dialog
import { useRouter } from "vue-router";
import AlertDialog from "./AlertDialog.vue";
import VersionCard from "./VersionCard.vue";
import VersionItem from "./VersionItem.vue";
import {
  versions, versionsLoading, versionsError, expanded, categories,
  pickVersion, openDelete,
  deleteDialog, deleteTarget, confirmDeleteVersion,
} from "../store/gameDirs";

const router = useRouter();

/** 选择版本 → 记录（含所属目录）并返回启动页。 */
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
    <div v-if="versionsLoading" class="vs-hint">加载中…</div>
    <div v-else-if="versionsError" class="vs-err">{{ versionsError }}</div>
    <div v-else-if="versions.length === 0" class="vs-empty">
      <p>该目录下没有已安装的游戏版本</p>
      <button class="vs-empty-btn" @click="goDownload">下载游戏</button>
    </div>
    <div v-else class="vs-cards">
      <VersionCard
        v-for="cat in categories"
        :key="cat.key"
        :title="cat.title"
        :count="cat.items.length"
        :expanded="expanded[cat.key]"
        @toggle="expanded[cat.key] = !expanded[cat.key]"
      >
        <VersionItem
          v-for="v in cat.items"
          :key="v.name"
          :icon-src="v.icon"
          :version-id="v.name"
          :subtitle="v.loaderVersion || '原版'"
          @select="selectAndBack(v.name)"
        >
          <template #action>
            <button
              class="vs-item-setup"
              title="版本设置"
              @click.stop="goSetup(v.name)"
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <line x1="21" y1="4" x2="14" y2="4"/>
                <line x1="10" y1="4" x2="3" y2="4"/>
                <line x1="21" y1="12" x2="12" y2="12"/>
                <line x1="8" y1="12" x2="3" y2="12"/>
                <line x1="21" y1="20" x2="16" y2="20"/>
                <line x1="12" y1="20" x2="3" y2="20"/>
                <line x1="14" y1="2" x2="14" y2="6"/>
                <line x1="8" y1="10" x2="8" y2="14"/>
                <line x1="16" y1="18" x2="16" y2="22"/>
              </svg>
            </button>
            <button
              class="vs-item-del"
              title="删除版本"
              @click.stop="openDelete(v.name)"
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="3 6 5 6 21 6" />
                <path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6" />
                <path d="M10 11v6M14 11v6" />
                <path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2" />
              </svg>
            </button>
          </template>
        </VersionItem>
      </VersionCard>
    </div>

    <!-- 删除版本确认 dialog -->
    <AlertDialog v-model="deleteDialog" title="删除版本" :divider="false">
      <div class="vs-dialog-body">
        <div class="vs-dialog-text">
          确定要删除版本「{{ deleteTarget }}」吗？该操作会删除版本文件夹及其全部文件，且无法恢复。
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn danger" @click="confirmDeleteVersion">删除</button>
        <button class="dialog-btn" @click="deleteDialog = false">取消</button>
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

/* 版本选择场景：item 之间不显示分割线（下载页保持原样） */
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
  background-color: rgba(231, 76, 60, 0.1);
  color: #e74c3c;
}

.vs-hint {
  padding: 24px;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
}

/* 空状态：右侧容器正中提示 + 下载入口 */
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
  /* 渐变 ↔ 纯色背景切换不做插值（会闪变），只过渡 transform/阴影 */
  transition: transform 0.15s, box-shadow 0.15s;
}

.vs-empty-btn:hover {
  background: var(--color-brand-hover);
  box-shadow: var(--shadow-md);
}

.vs-empty-btn:active {
  transform: scale(0.97);
}

.vs-err {
  font-size: 12px;
  color: #e74c3c;
  padding: 6px 8px;
}

/* dialog 内容 */
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

.dialog-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.dialog-btn.primary {
  background: var(--color-brand);
  border-color: var(--color-brand);
  color: #fff;
}

.dialog-btn.primary:hover {
  opacity: 0.88;
}

.dialog-btn.danger {
  background: #e74c3c;
  border-color: #e74c3c;
  color: #fff;
}

.dialog-btn.danger:hover {
  opacity: 0.88;
}
</style>
