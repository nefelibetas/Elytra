<script setup lang="ts">
// 外观设置：内置亮色主题（Grass / End / Nether / Ice）+ 用户自定义外观（核心色板编辑器）。
// 内置卡片点击切换；自定义卡片点击激活、hover 显示编辑/删除；末尾「添加自定义」新建。
// 全部即时生效并持久化 localStorage（theme 入选 + 自定义 theme 列表）。
import { computed, onMounted } from "vue";
import { useRouter } from "vue-router";
import {
  THEMES, themeId, customThemes, isCustom,
  deleteCustomTheme, activateCustomTheme, ensureCustomThemesApplied,
  type ThemeId,
} from "../../store/app/theme";
import { Trash2, Pencil, Plus } from "@lucide/vue";

// 挂载时注入全部已保存自定义主题样式（页面刷新后 style 不持久），并高亮当前选中
onMounted(() => ensureCustomThemesApplied());

/** 展示用主题列表：内置 + 自定义。自定义主题合成 swatch（品牌/文本/底色）与 desc。 */
const allThemes = computed(() => {
  const customs = Object.values(customThemes.value).map((t) => ({
    id: t.id,
    label: t.name,
    desc: t.desc || "自定义外观",
    swatch: [
      t.vars["--color-brand"],
      t.vars["--text-primary"],
      t.vars["--surface-page"],
    ] as string[],
  }));
  return [...THEMES, ...customs];
});


function isActive(id: string) {
  return themeId.value === id;
}

/** 卡片图标：swatch 首色为主、次色为渐变（内置 4 色 / 自定义 3 色取前两）。 */
function iconStyle(swatch: string[]) {
  const a = swatch[0] ?? "var(--color-brand)";
  const b = swatch[1] ?? swatch[0] ?? "var(--color-brand)";
  return { background: `linear-gradient(135deg, ${a}, ${b})` };
}

/** 点击内置主题：直接切换（调用方已确认 id 为内置主题）。 */
function selectBuiltin(id: string) {
  themeId.value = id as ThemeId;
}

/** 主题卡片点击：自定义 → 激活；内置 → 切换。 */
function onPick(id: string) {
  if (isCustom(id)) selectCustom(id);
  else selectBuiltin(id);
}

/** 点击自定义主题：激活（切换 + 注入）。 */
function selectCustom(id: string) {
  activateCustomTheme(id);
}

// ── 新建自定义 ──









/** 进入自定义外观编辑器（子路由）：新建从当前主题复制起点。 */
const router = useRouter();

function openNew(fromThemeId: string) {
  void router.push({ path: "/custom-theme", query: { from: fromThemeId } });
}

/** 进入编辑器编辑已有自定义主题。 */
function openEdit(id: string) {
  void router.push({ path: "/custom-theme", query: { id } });
}

/** 删除确认：直接删（简易确认放回调）。 */
function removeCustom(id: string) {
  const t = customThemes.value[id];
  if (!t) return;
  if (window.confirm(`删除自定义外观「${t.name}」？`)) {
    deleteCustomTheme(id);
  }
}

</script>

<template>
  <section class="stg-card">
    <div class="stg-card-hdr">
      <h3>外观</h3>
      <p class="stg-card-sub">选择主题（内置均为亮色，accent 取自 Minecraft 生态；可添加自定义外观）</p>
    </div>
    <div class="appearance-grid">
      <button
        v-for="t in allThemes"
        :key="t.id"
        class="appearance-item"
        :class="{ active: isActive(t.id), 'appearance-item--custom': isCustom(t.id) }"
        @click="onPick(t.id)"
      >
        <span class="appearance-icon" :style="iconStyle(t.swatch)"></span>
        <span class="appearance-name">{{ t.label }}</span>
        <span class="appearance-desc">{{ t.desc }}</span>
        <!-- 自定义卡片的 hover 操作：编辑 / 删除 -->
        <span v-if="isCustom(t.id)" class="appearance-custom-ops" @click.stop>
          <button class="appearance-op" v-tooltip="'编辑'" @click="openEdit(t.id)">
            <Pencil :size="13" />
          </button>
          <button class="appearance-op danger" v-tooltip="'删除'" @click="removeCustom(t.id)">
            <Trash2 :size="13" />
          </button>
        </span>
      </button>

      <!-- 添加自定义 -->
      <button class="appearance-item appearance-add" @click="openNew(themeId)">
        <span class="appearance-icon appearance-add-icon"><Plus :size="26" /></span>
        <span class="appearance-name">添加自定义</span>
        <span class="appearance-desc">自定义品牌色与文本配色</span>
      </button>
    </div>

  </section>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
.appearance-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 12px;
}

.appearance-item {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  aspect-ratio: 1 / 1;
  width: 100%;
  padding: 16px 12px;
  border: 1px solid var(--border-default);
  border-radius: 16px;
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

/* 自定义卡片：顶部预留 ops 空间 */
.appearance-item--custom {
  padding-top: 40px;
}

/* 自定义卡片 ops：右上角固定，hover 时从透明淡入 */
.appearance-custom-ops {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  gap: 4px;
  opacity: 0;
  transition: opacity 0.15s;
}

.appearance-item:hover .appearance-custom-ops {
  opacity: 1;
}

.appearance-op {
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface-elevated);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}

.appearance-op:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.appearance-op.danger:hover {
  border-color: var(--color-danger);
  color: var(--color-danger);
}

/* 添加卡片：虚线边框整卡 */
.appearance-add {
  border-style: dashed;
}

.appearance-icon {
  width: 64px;
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 18px;
  box-shadow: var(--shadow-md);
  color: #FFFFFF;
  flex-shrink: 0;
}

.appearance-add-icon {
  background: var(--surface-page);
  color: var(--text-tertiary);
  border: 1.5px dashed var(--border-strong);
  box-shadow: none;
}

.appearance-add:hover .appearance-add-icon {
  color: var(--color-brand);
  border-color: var(--color-brand);
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