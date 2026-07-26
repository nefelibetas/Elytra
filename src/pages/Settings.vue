<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface JavaEnv { path: string; version: string; vendor: string; arch: string; }

const jdkList = ref<JavaEnv[]>([]);
const jdkError = ref("");

async function scanAllJdk() {
  jdkList.value = []; jdkError.value = "";
  try { jdkList.value = await invoke<JavaEnv[]>("auto_scan_jdk"); }
  catch (e) { jdkError.value = String(e); }
}

async function selectCustomDir() {
  jdkList.value = []; jdkError.value = "";
  try {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;
    jdkList.value = await invoke<JavaEnv[]>("scan_custom_jdk", { path: dir });
  } catch (e) { jdkError.value = String(e); }
}
</script>

<template>
  <div class="page">
    <h2>设置</h2>

    <section class="section">
      <h3>Java 运行时</h3>
      <div class="actions">
        <button @click="scanAllJdk">自动扫描</button>
        <button @click="selectCustomDir">选择目录</button>
      </div>
      <div v-if="jdkList.length" class="info">
        <p>找到 {{ jdkList.length }} 个 JDK：</p>
        <div v-for="(jdk, i) in jdkList" :key="i" class="item">
          <p>#{{ i + 1 }} 路径：{{ jdk.path }}</p>
          <p>版本：{{ jdk.version }}（{{ jdk.arch }}）</p>
          <p>厂商：{{ jdk.vendor }}</p>
        </div>
      </div>
      <div v-else-if="jdkError" class="err">{{ jdkError }}</div>
      <div v-else class="hint">点击按钮扫描系统中已安装的 JDK</div>
    </section>
  </div>
</template>

<style scoped>
.page {
  padding: 24px;
  max-width: 640px;
}

.page h2 {
  font-size: 20px;
  font-weight: 600;
  margin-bottom: 24px;
  color: var(--text-primary);
}

.section { margin-bottom: 32px; }

.section h3 {
  font-size: 15px;
  font-weight: 500;
  color: var(--text-secondary);
  margin-bottom: 12px;
}

.actions {
  display: flex;
  gap: 8px;
  margin-bottom: 16px;
}

.actions button {
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  padding: 0.5em 1.2em;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  background-color: var(--surface-card);
  cursor: pointer;
  transition: 0.15s;
}

.actions button:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.info {
  background-color: var(--surface-card);
  padding: 12px 16px;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-default);
}

.info p {
  margin: 4px 0;
  color: var(--text-secondary);
  font-size: 13px;
}

.item {
  border-top: 1px solid var(--border-default);
  padding: 4px 0;
}

.err { color: #e74c3c; }
.hint { color: var(--text-tertiary); font-size: 13px; }
</style>
