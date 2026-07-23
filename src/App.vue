<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface JavaEnv {
  path: string;
  version: string;
  vendor: string;
  arch: string;
}

const jdkList = ref<JavaEnv[]>([]);
const jdkError = ref("");

const customJdkList = ref<JavaEnv[]>([]);
const customJdkError = ref("");

async function scanAllJdk() {
  jdkList.value = [];
  jdkError.value = "";
  try {
    jdkList.value = await invoke<JavaEnv[]>("auto_scan_jdk");
  } catch (e) {
    jdkError.value = String(e);
  }
}

async function selectCustomDir() {
  customJdkList.value = [];
  customJdkError.value = "";
  try {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;
    customJdkList.value = await invoke<JavaEnv[]>("scan_custom_jdk", { path: dir });
  } catch (e) {
    customJdkError.value = String(e);
  }
}
</script>

<template>
  <main class="container">
    <h1>Elytra Launcher</h1>

    <div class="section">
      <button @click="scanAllJdk">自动扫描 JDK</button>
      <div v-if="jdkList.length > 0" class="jdk-info">
        <p>找到 {{ jdkList.length }} 个 JDK：</p>
        <div v-for="(jdk, index) in jdkList" :key="index" class="jdk-item">
          <p>#{{ index + 1 }} 路径：{{ jdk.path }}</p>
          <p>版本：{{ jdk.version }} （{{ jdk.arch }}）</p>
          <p>厂商：{{ jdk.vendor }}</p>
        </div>
      </div>
      <div v-else-if="jdkError" class="jdk-error">{{ jdkError }}</div>
      <div v-else class="jdk-hint">点击按钮自动扫描 JDK</div>
    </div>

    <div class="section">
      <button @click="selectCustomDir">选择目录</button>
      <div v-if="customJdkList.length > 0" class="jdk-info">
        <p>找到 {{ customJdkList.length }} 个 JDK：</p>
        <div v-for="(jdk, index) in customJdkList" :key="index" class="jdk-item">
          <p>#{{ index + 1 }} 路径：{{ jdk.path }}</p>
          <p>版本：{{ jdk.version }} （{{ jdk.arch }}）</p>
          <p>厂商：{{ jdk.vendor }}</p>
        </div>
      </div>
      <div v-else-if="customJdkError" class="jdk-error">{{ customJdkError }}</div>
    </div>
  </main>
</template>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;
  color: #0f0f0f;
  background-color: #f6f6f6;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}

.section {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}

button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  cursor: pointer;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
  outline: none;
}

button:hover {
  border-color: #396cd8;
}

button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

.jdk-info {
  text-align: left;
  background: #ffffff;
  padding: 16px 24px;
  border-radius: 8px;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.1);
}

.jdk-info p {
  margin: 4px 0;
}

.jdk-item {
  border-top: 1px solid #e0e0e0;
  padding: 4px 0;
}

.jdk-error {
  color: #d32f2f;
}

.jdk-hint {
  color: #888;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }

  button:active {
    background-color: #0f0f0f69;
  }

  .jdk-info {
    background: #1e1e1e;
  }

  .jdk-item {
    border-top-color: #333;
  }

  .jdk-hint {
    color: #888;
  }
}
</style>