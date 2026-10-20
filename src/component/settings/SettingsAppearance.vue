<script setup lang="ts">
// 外观设置：内置亮色主题（Grass/End/Nether/Ice）+ 自定义色板。
// 即时生效并持久化 localStorage；自定义卡片 hover 显示编辑/删除。
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import DropSelect from "../common/DropSelect.vue";
import { currentLocale, languageSetting, setLanguage, type LanguageSetting } from "../../i18n";
import {
  THEMES, themeId, customThemes, isCustom,
  deleteCustomTheme, activateCustomTheme, ensureCustomThemesApplied,
  type ThemeId,
} from "../../store/app/theme";
import { Trash2, Pencil, Plus } from "@lucide/vue";

const { t } = useI18n();

// 挂载时注入已保存自定义主题样式（刷新后 style 不持久）。
onMounted(() => ensureCustomThemesApplied());

const allThemes = computed(() => {
  const builtins = THEMES.map((theme) => ({
    id: theme.id,
    label: t(theme.labelKey),
    desc: t(theme.descKey),
    swatch: theme.swatch,
  }));
  const customs = Object.values(customThemes.value).map((theme) => ({
    id: theme.id,
    label: theme.name,
    desc: theme.desc || t("settings.appearance.customDesc"),
    swatch: [
      theme.vars["--color-brand"],
      theme.vars["--text-primary"],
      theme.vars["--surface-page"],
    ] as string[],
  }));
  return [...builtins, ...customs];
});

const languageOptions = computed(() => [
  {
    value: "auto",
    label: t("common.language.autoCurrent", {
      locale: currentLocale.value === "zh-CN" ? t("common.language.zhCN") : t("common.language.enUS"),
    }),
    desc: t("common.language.desc"),
  },
  { value: "zh-CN", label: t("common.language.zhCN") },
  { value: "en-US", label: t("common.language.enUS") },
]);

function onLanguage(value: string) {
  setLanguage(value as LanguageSetting);
}


function isActive(id: string) {
  return themeId.value === id;
}

function iconStyle(swatch: string[]) {
  const a = swatch[0] ?? "var(--color-brand)";
  const b = swatch[1] ?? swatch[0] ?? "var(--color-brand)";
  return { background: `linear-gradient(135deg, ${a}, ${b})` };
}

function selectBuiltin(id: string) {
  themeId.value = id as ThemeId;
}

/** 主题卡片点击：自定义 → 激活；内置 → 切换。 */
function onPick(id: string) {
  if (isCustom(id)) selectCustom(id);
  else selectBuiltin(id);
}

/** 激活自定义主题（切换 + 注入）。 */
function selectCustom(id: string) {
  activateCustomTheme(id);
}

// ── 新建自定义 ──









/** 进入自定义编辑器（子路由）：新建从当前主题复制。 */
const router = useRouter();

function openNew(fromThemeId: string) {
  void router.push({ path: "/custom-theme", query: { from: fromThemeId } });
}

function openEdit(id: string) {
  void router.push({ path: "/custom-theme", query: { id } });
}

function removeCustom(id: string) {
  const theme = customThemes.value[id];
  if (!theme) return;
  if (window.confirm(t("settings.appearance.deleteConfirm", { name: theme.name }))) {
    deleteCustomTheme(id);
  }
}

</script>

<template>
  <section class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("common.language.title") }}</h3>
      <p class="stg-card-sub">{{ t("common.language.desc") }}</p>
    </div>
    <div class="lang-select">
      <DropSelect
        :model-value="languageSetting"
        :options="languageOptions"
        @update:model-value="onLanguage"
      />
    </div>
  </section>

  <section class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.appearance.title") }}</h3>
      <p class="stg-card-sub">{{ t("settings.appearance.subtitle") }}</p>
    </div>
    <div class="appearance-grid">
      <button
        v-for="theme in allThemes"
        :key="theme.id"
        class="appearance-item"
        :class="{ active: isActive(theme.id), 'appearance-item--custom': isCustom(theme.id) }"
        @click="onPick(theme.id)"
      >
        <span class="appearance-icon" :style="iconStyle(theme.swatch)"></span>
        <span class="appearance-name">{{ theme.label }}</span>
        <span class="appearance-desc">{{ theme.desc }}</span>
        <span v-if="isCustom(theme.id)" class="appearance-custom-ops" @click.stop>
          <button class="appearance-op" v-tooltip="t('common.action.edit')" @click="openEdit(theme.id)">
            <Pencil :size="13" />
          </button>
          <button class="appearance-op danger" v-tooltip="t('common.action.delete')" @click="removeCustom(theme.id)">
            <Trash2 :size="13" />
          </button>
        </span>
      </button>

      <button class="appearance-item appearance-add" @click="openNew(themeId)">
        <span class="appearance-icon appearance-add-icon"><Plus :size="26" /></span>
        <span class="appearance-name">{{ t("settings.appearance.addCustom") }}</span>
        <span class="appearance-desc">{{ t("settings.appearance.addCustomDesc") }}</span>
      </button>
    </div>

  </section>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
.lang-select {
  max-width: 320px;
}

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

.appearance-item--custom {
  padding-top: 40px;
}

/* 自定义卡片 ops：右上角，hover 淡入 */
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