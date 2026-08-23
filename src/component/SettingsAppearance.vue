<script setup lang="ts">
// 外观设置：多亮色主题选择（Grass / End / Nether / Ice）。
// 每卡片展示该主题的色板预览（品牌渐变 + 页面底色 + 文字色）；切换即时生效并持久化 localStorage。
import { THEMES, themeId } from "../store/theme";
</script>

<template>
  <section class="stg-card">
    <div class="stg-card-hdr">
      <h3>外观</h3>
      <p class="stg-card-sub">选择主题（均为亮色，accent 取自 Minecraft 生态）</p>
    </div>
    <div class="appearance-grid">
      <button
        v-for="t in THEMES"
        :key="t.id"
        class="appearance-item"
        :class="{ active: themeId === t.id }"
        @click="themeId = t.id"
      >
        <span class="swatch-row">
          <span
            v-for="(c, i) in t.swatch"
            :key="i"
            class="swatch-dot"
            :style="{ background: c }"
          ></span>
        </span>
        <span class="appearance-name">{{ t.label }}</span>
        <span class="appearance-desc">{{ t.desc }}</span>
      </button>
    </div>
  </section>
</template>

<style>
@import "../styles/settings-shared.css";
</style>

<style scoped>
.appearance-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 12px;
}

.appearance-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 16px 12px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-elevated);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}

.appearance-item:hover {
  border-color: var(--color-brand);
  color: var(--text-primary);
  transform: translateY(-1px);
}

.appearance-item.active {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
  box-shadow: 0 0 0 1px var(--color-brand) inset;
}

.swatch-row {
  display: flex;
  gap: 5px;
}

.swatch-dot {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 1px solid var(--border-subtle);
}

.appearance-name {
  font-size: 13px;
  font-weight: 600;
}

.appearance-desc {
  font-size: 11px;
  color: var(--text-tertiary);
  text-align: center;
  line-height: 1.4;
}
</style>
