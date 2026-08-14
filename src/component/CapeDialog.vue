<script setup lang="ts">
// 披风选择 dialog：checkbox 单选列表 + 无披风选项，无分割线
import AlertDialog from "./AlertDialog.vue";
import { accountUi } from "../store/accountUi";
</script>

<template>
  <AlertDialog v-model="accountUi.capeDialogVisible" title="选择披风" :divider="false">
    <div class="cape-dialog-list">
      <div
        class="cape-opt"
        :class="{ checked: accountUi.selectedCapeId === '' }"
        @click="accountUi.selectedCapeId = ''"
      >
        <span class="cape-cb"></span>
        <span class="cape-opt-name">无披风</span>
      </div>
      <div
        v-for="c in accountUi.capes"
        :key="c.id"
        class="cape-opt"
        :class="{ checked: accountUi.selectedCapeId === c.id }"
        @click="accountUi.selectedCapeId = c.id"
      >
        <span class="cape-cb"></span>
        <span class="cape-opt-name">{{ c.alias || c.id.slice(0, 8) }}</span>
      </div>
    </div>
    <template #actions>
      <button class="dialog-btn primary" @click="accountUi.confirmCape">确定</button>
      <button class="dialog-btn" @click="accountUi.closeCapeDialog">取消</button>
    </template>
  </AlertDialog>
</template>

<style scoped>
/* 选择披风 dialog：checkbox 单选列表（可滚动） */
.cape-dialog-list {
  max-height: 220px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.cape-opt {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background 0.12s;
}

.cape-opt:hover {
  background: var(--color-brand-bg);
}

/* checkbox 外观：品牌渐变填充 + glow + 回弹勾选动画 */
.cape-cb {
  flex: none;
  width: 16px;
  height: 16px;
  border: 1.5px solid var(--border-strong);
  border-radius: 5px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface-card);
  transition: border-color 0.15s, background 0.15s, box-shadow 0.15s;
}

.cape-opt:hover .cape-cb {
  border-color: var(--color-brand);
}

.cape-cb::after {
  content: "";
  width: 8px;
  height: 4.5px;
  border-left: 2px solid #fff;
  border-bottom: 2px solid #fff;
  transform: rotate(-45deg) translate(0.5px, -0.5px) scale(0);
  transition: transform 0.18s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.cape-opt.checked .cape-cb {
  background: var(--color-brand-gradient);
  border-color: transparent;
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

.cape-opt.checked .cape-cb::after {
  transform: rotate(-45deg) translate(0.5px, -0.5px) scale(1);
}

.cape-opt-name {
  font-size: 13px;
  color: var(--text-primary);
  user-select: none;
}

/* dialog 操作按钮 */
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
</style>
