<script setup lang="ts">
import { ref } from "vue";

const props = withDefaults(defineProps<{
  title?: string;
  placeholder?: string;
  confirmText?: string;
  cancelText?: string;
}>(), {
  title: "选择文件夹",
  placeholder: "输入或粘贴文件夹路径",
  confirmText: "确认",
  cancelText: "取消",
});

const emit = defineEmits<{
  confirm: [path: string];
  cancel: [];
}>();

const pathInput = ref("");

function onConfirm() {
  const trimmed = pathInput.value.trim();
  if (trimmed) {
    emit("confirm", trimmed);
  }
}

function onCancel() {
  emit("cancel");
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") onCancel();
  if (e.key === "Enter") onConfirm();
}
</script>

<template>
  <Teleport to="body">
    <div class="dialog-overlay" @click.self="onCancel">
      <div class="dialog-card" @keydown="onKeydown">
        <div class="dialog-header">
          <span class="dialog-title">{{ title }}</span>
          <button class="dialog-close" @click="onCancel">
            <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
              <polygon points="1,1 5,5 9,1 11,3 7,7 11,11 9,13 5,9 1,13 -1,11 3,7 -1,3" />
            </svg>
          </button>
        </div>

        <div class="dialog-body">
          <input
            v-model="pathInput"
            class="dialog-input"
            type="text"
            :placeholder="placeholder"
            spellcheck="false"
            autofocus
          />
        </div>

        <div class="dialog-footer">
          <button class="btn btn-cancel" @click="onCancel">{{ cancelText }}</button>
          <button class="btn btn-confirm" :disabled="!pathInput.trim()" @click="onConfirm">{{ confirmText }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(30, 27, 75, 0.35);
  backdrop-filter: blur(2px);
}

.dialog-card {
  width: 420px;
  background: var(--surface-card);
  border-radius: var(--radius-lg);
  box-shadow: 0 8px 32px rgba(30, 27, 75, 0.15);
  overflow: hidden;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border-default);
}

.dialog-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-close {
  width: 28px;
  height: 28px;
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

.dialog-close:hover {
  background: rgba(231, 76, 60, 0.1);
  color: #e74c3c;
}

.dialog-body {
  padding: 20px;
}

.dialog-input {
  width: 100%;
  padding: 10px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 14px;
  outline: none;
  transition: border-color 0.15s;
}

.dialog-input:focus {
  border-color: var(--color-brand);
  box-shadow: 0 0 0 3px var(--color-brand-bg);
}

.dialog-input::placeholder {
  color: var(--text-tertiary);
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--border-default);
}

.btn {
  padding: 7px 18px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
  border: 1px solid transparent;
}

.btn-cancel {
  border-color: var(--border-strong);
  background: transparent;
  color: var(--text-secondary);
}

.btn-cancel:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.btn-confirm {
  border-color: var(--color-brand);
  background: var(--color-brand);
  color: var(--text-inverse);
}

.btn-confirm:hover {
  background: var(--color-brand-dark);
}

.btn-confirm:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
