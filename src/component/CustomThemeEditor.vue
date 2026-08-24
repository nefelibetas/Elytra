<script setup lang="ts">
// 自定义外观编辑器（子路由页）：所见即所得骨架预览，点击部位弹出内嵌调色板调色。
// 入口：设置-外观「添加自定义」（新建）或自定义卡片「编辑」（query id）；保存后返回设置页。
import { computed, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { goBack } from "../store/nav";
import { Check, RotateCcw } from "@lucide/vue";
import {
  THEMES, customThemes, builtinVars,
  createCustomTheme, updateCustomTheme, activateCustomTheme,
  type CustomThemeVars, type ThemeId,
} from "../store/theme";

const route = useRoute();

/** query id：存在 = 编辑已有自定义主题；缺失 = 新建。 */
const editingId = computed(() => (route.query.id as string) || "");

const editName = ref("");
const editDesc = ref("");
const editVars = ref<CustomThemeVars>(initVars());

function initVars(): CustomThemeVars {
  if (editingId.value && customThemes.value[editingId.value]) {
    // 编辑：载入现有值
    const t = customThemes.value[editingId.value];
    editName.value = t.name;
    editDesc.value = t.desc ?? "";
    return { ...t.vars };
  }
  // 新建：从当前主题（内置或自定义）复制起点
  const from = route.query.from as string | undefined;
  editName.value = "自定义外观";
  editDesc.value = "";
  if (from && THEMES.some((t) => t.id === from)) {
    return builtinVars(from as ThemeId);
  }
  if (from && customThemes.value[from]) {
    return { ...customThemes.value[from].vars };
  }
  return builtinVars("grass");
}

/** 同组件实例下 query 变化（新建 ↔ 编辑）：重建表单与色板状态。 */
watch(
  () => route.query,
  () => {
    editVars.value = initVars();
    activeField.value = null;
    paletteOpen.value = false;
  },
);

/** 骨架可调部位 → 对应 CSS 变量与标签。 */
interface PartDef { key: keyof CustomThemeVars; label: string }
const PARTS: PartDef[] = [
  { key: "--color-brand", label: "品牌色" },
  { key: "--text-primary", label: "主文字" },
  { key: "--text-secondary", label: "次要文字" },
  { key: "--surface-page", label: "页面底色" },
  { key: "--surface-hover", label: "悬停底色" },
  { key: "--border-default", label: "常规边框" },
  { key: "--border-strong", label: "强调边框" },
];
const PART_LABELS: Record<string, string> = Object.fromEntries(
  PARTS.map((p) => [p.key, p.label]),
);

/** 骨架预览 CSS 变量透传（局部作用域，不污染全局主题）。 */
const previewStyle = computed(() => {
  const v = editVars.value;
  const brand = v["--color-brand"];
  return {
    "--pv-color-brand": brand,
    "--pv-color-brand-bg": colorMix(brand, 10),
    "--pv-text-primary": v["--text-primary"],
    "--pv-text-secondary": v["--text-secondary"],
    "--pv-text-tertiary": v["--text-tertiary"],
    "--pv-surface-page": v["--surface-page"],
    "--pv-surface-card": "#FFFFFF",
    "--pv-surface-hover": visibleHover(v["--surface-hover"]),
    "--pv-border-default": v["--border-default"],
    "--pv-border-strong": v["--border-strong"],
  };
});

/** 预览专用：悬停底色 rgba alpha 过低时提升到可见下限（真实应用保持原值，
 *  骨架上需让该可点击部位清晰可辨）。hex 等不透明值原样返回。 */
function visibleHover(v: string): string {
  const m = v.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
  if (m) {
    const alpha = m[4] === undefined ? 1 : parseFloat(m[4]);
    const MIN = 0.14; // 预览可辨识下限（真实 0.05 太淡）
    if (alpha < MIN) {
      return `rgba(${m[1]}, ${m[2]}, ${m[3]}, ${MIN})`;
    }
  }
  return v;
}

/** hex + 透明度 → rgba（骨架选中项底色等半透明场景）。 */
function colorMix(hex: string, alphaPct: number): string {
  const m = hex.match(/^#([0-9a-fA-F]{6})$/);
  if (!m) return hex;
  const r = parseInt(m[1].slice(0, 2), 16);
  const g = parseInt(m[1].slice(2, 4), 16);
  const b = parseInt(m[1].slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alphaPct / 100})`;
}

// ── 调色板状态 ──

/** 当前选中的部位变量。 */
const activeField = ref<keyof CustomThemeVars | null>(null);

/** 调色板开关。 */
const paletteOpen = ref(false);

/** 点击骨架部位：选中 + 打开调色板。 */
function pickField(key: keyof CustomThemeVars) {
  activeField.value = key;
  paletteOpen.value = true;
}

/** 当前选中变量的 css 值。 */
const currentVal = computed(() =>
  activeField.value ? editVars.value[activeField.value] : "",
);

/** 预设色板（Minecraft 生态 + 中性色）。 */
const PRESET_COLORS = [
  "#059669", "#10B981", "#34D399", "#0EA5E9", "#38BDF8", "#7C3AED",
  "#A855F7", "#EA580C", "#F97316", "#F59E0B", "#EAB308", "#84CC16",
  "#DC2626", "#EF4444", "#EC4899", "#F43F5E", "#14B8A6", "#0891B2",
  "#1F2937", "#374151", "#6B7280", "#9CA3AF", "#D1D5DB", "#F3F4F6",
];

/** 从预设色板选色（保留原变量透明度语义）。 */
function pickPreset(hex: string) {
  if (!activeField.value) return;
  applyColor(hex);
}

/** 应用颜色到当前变量：原值 rgba 保留 alpha。 */
function applyColor(hex: string) {
  if (!activeField.value) return;
  if (!/^#[0-9a-fA-F]{6}$/.test(hex)) return;
  const key = activeField.value;
  const old = editVars.value[key];
  const m = old.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
  if (m) {
    const alpha = m[4] ?? "1";
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    editVars.value[key] = `rgba(${r}, ${g}, ${b}, ${alpha})`;
  } else {
    editVars.value[key] = hex;
  }
}

/** hex 文本框输入（change 校验）。 */
function onHexText(e: Event) {
  const el = e.target as HTMLInputElement;
  // 前缀 # 已从输入框拆出（样式展示），输入补全回 hex 校验
  let val = el.value.trim();
  if (!val.startsWith("#")) val = "#" + val;
  if (/^#[0-9a-fA-F]{6}$/.test(val)) {
    applyColor(val);
    el.value = val.slice(1);
  } else {
    el.value = currentHex.value.replace(/^#/, "");
  }
}

/** 当前选中变量的 hex 显示值（rgba → 主色 hex）。 */
const currentHex = computed(() => {
  const v = currentVal.value;
  const m = v.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
  if (m) {
    const toHex = (n: string) => (+n).toString(16).padStart(2, "0");
    return `#${toHex(m[1])}${toHex(m[2])}${toHex(m[3])}`;
  }
  return /^#[0-9a-fA-F]{6}$/.test(v) ? v : "#000000";
});

/** 重置当前变量为默认（grass 值）。 */
function resetField() {
  if (!activeField.value) return;
  editVars.value[activeField.value] = builtinVars("grass")[activeField.value];
}

// ── 保存 / 返回 ──

/** 保存：新建或更新，激活，返回设置页。 */
function save() {
  const name = editName.value.trim() || "自定义外观";
  const desc = editDesc.value.trim();
  const patch = { name, desc: desc || undefined, vars: { ...editVars.value } };
  if (editingId.value && customThemes.value[editingId.value]) {
    updateCustomTheme(editingId.value, patch);
    activateCustomTheme(editingId.value as ThemeId);
  } else {
    const t = createCustomTheme();
    updateCustomTheme(t.id, patch);
    activateCustomTheme(t.id as ThemeId);
  }
  goBack();
}
</script>

<template>
  <div class="cte">
    <div class="cte-body">
      <!-- 左：骨架预览 -->
      <div class="cte-preview" :style="previewStyle">
        <div class="pv-titlebar">
          <span
            class="pv-part pv-logo-wrap"
            :class="{ active: activeField === '--color-brand' }"
            @click="pickField('--color-brand')"
          >
            <span class="pv-logo"></span>
            <span class="pv-tag">品牌色</span>
          </span>
          <span
            class="pv-part pv-title"
            :class="{ active: activeField === '--text-primary' }"
            @click="pickField('--text-primary')"
          >
            <span class="pv-title-text">Elytra</span>
            <span class="pv-tag">主文字</span>
          </span>
          <span
            class="pv-part pv-window-btn"
            :class="{ active: activeField === '--border-strong' }"
            @click="pickField('--border-strong')"
          ></span>
          <span class="pv-part pv-window-btn" @click="pickField('--border-strong')"></span>
        </div>
        <div class="pv-main">
          <div class="pv-sidebar">
            <span
              class="pv-part pv-side-item pv-side-active"
              :class="{ active: activeField === '--color-brand' }"
              @click="pickField('--color-brand')"
            ></span>
            <span
              class="pv-part pv-side-item"
              :class="{ active: activeField === '--text-secondary' }"
              @click="pickField('--text-secondary')"
            ></span>
            <span
              class="pv-part pv-side-item"
              :class="{ active: activeField === '--border-strong' }"
              @click="pickField('--border-strong')"
            ></span>
          </div>
          <div class="pv-content">
            <div class="pv-card pv-card-head">
              <span
                class="pv-part pv-card-title"
                :class="{ active: activeField === '--text-primary' }"
                @click="pickField('--text-primary')"
              ></span>
              <span
                class="pv-part pv-card-btn"
                :class="{ active: activeField === '--color-brand' }"
                @click="pickField('--color-brand')"
              ></span>
            </div>
            <div class="pv-card pv-card-list">
              <span
                class="pv-part pv-list-item"
                :class="{ active: activeField === '--surface-hover' }"
                @click="pickField('--surface-hover')"
              ></span>
              <span
                class="pv-part pv-list-item pv-list-sel"
                :class="{ active: activeField === '--color-brand' }"
                @click="pickField('--color-brand')"
              ></span>
              <span class="pv-part pv-list-item pv-list-plain"></span>
            </div>
          </div>
        </div>
        <div class="pv-foot">
          <span
            class="pv-part pv-foot-btn"
            :class="{ active: activeField === '--text-secondary' }"
            @click="pickField('--text-secondary')"
          ></span>
          <span
            class="pv-part pv-foot-btn pv-foot-primary"
            :class="{ active: activeField === '--color-brand' }"
            @click="pickField('--color-brand')"
          ></span>
        </div>
        <span
          class="pv-part pv-page-dot"
          :class="{ active: activeField === '--surface-page' }"
          @click="pickField('--surface-page')"
        ></span>
      </div>

      <!-- 右栏：上调色板 + 下标题/保存 -->
      <div class="cte-right">
      <!-- 上：内嵌调色板（点击部位后出现） -->
      <div class="cte-palette" :class="{ open: paletteOpen }">
        <template v-if="activeField && paletteOpen">
          <div class="cte-palette-head">
            <span class="cte-palette-title">
              <span class="cte-title-dot" :style="{ background: currentHex }"></span>
              {{ PART_LABELS[activeField] }}
            </span>
            <button class="cte-palette-reset" title="重置" @click="resetField">
              <RotateCcw :size="13" />
            </button>
          </div>
          <div class="cte-palette-grid">
            <button
              v-for="c in PRESET_COLORS"
              :key="c"
              class="cte-preset"
              :class="{ picked: c.toLowerCase() === currentHex.toLowerCase() }"
              :style="{ background: c }"
              @click="pickPreset(c)"
            ></button>
          </div>
          <div class="cte-palette-custom">
            <input
              type="color"
              class="cte-custom-color"
              :value="currentHex"
              @input="pickPreset(($event.target as HTMLInputElement).value)"
            />
            <div class="cte-hex-box">
              <span class="cte-hex-prefix">#</span>
              <input
                class="cte-custom-text"
                :value="currentHex.replace(/^#/, '')"
                @change="onHexText"
                spellcheck="false"
              />
            </div>
          </div>
        </template>
        <div v-else class="cte-palette-empty">
          点击左侧预览中的色块<br />调整对应颜色
        </div>
      </div>

      <!-- 下：标题 / 副标题 / 保存 -->
      <div class="cte-meta">
        <div class="cte-titles">
          <input
            v-model="editName"
            class="cte-name"
            type="text"
            placeholder="外观名称"
            maxlength="24"
          />
          <input
            v-model="editDesc"
            class="cte-desc"
            type="text"
            placeholder="副标题（可选）"
            maxlength="60"
          />
        </div>
        <button class="cte-save" @click="save">
          <Check :size="15" />
          <span>保存</span>
        </button>
      </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cte {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--surface-page);
}

/* 右栏：上（调色板）下（标题/保存） */
.cte-right {
  flex: 0 0 280px;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.cte-titles {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 4px 6px 0;
}

/* 名称：无边框底划线，大字号加粗，focus 品牌线 */
.cte-name {
  width: 100%;
  padding: 6px 2px 8px;
  border: none;
  border-bottom: 2px solid var(--border-default);
  background: transparent;
  color: var(--text-primary);
  font-size: 16px;
  font-weight: 700;
  outline: none;
  transition: border-color 0.15s;
}

.cte-name::placeholder {
  color: var(--text-tertiary);
}

.cte-name:focus {
  border-bottom-color: var(--color-brand);
}

/* 副标题：无边框底划线，小字号弱化 */
.cte-desc {
  width: 100%;
  padding: 2px 2px 6px;
  border: none;
  border-bottom: 1px solid var(--border-subtle, var(--border-default));
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  outline: none;
  transition: border-color 0.15s;
}

.cte-desc::placeholder {
  color: var(--text-tertiary);
}

.cte-desc:focus {
  border-bottom-color: var(--color-brand);
}


/* 下区：标题 / 副标题 / 保存 */
.cte-meta {
  flex: none;
  display: flex;
  align-items: stretch;
  gap: 12px;
  padding: 16px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 14px;
  box-shadow: var(--shadow-md);
}

.cte-meta .cte-titles {
  justify-content: center;
}
.cte-save {
  flex: none;
  align-self: stretch;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 9px 20px;
  border: none;
  border-radius: 10px;
  background: var(--color-brand-gradient, linear-gradient(135deg, var(--color-brand), var(--color-brand-dark)));
  color: var(--text-inverse);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: 0 2px 8px var(--color-brand-glow, rgba(0,0,0,0.2));
  transition: all 0.15s;
}

.cte-save:hover {
  transform: translateY(-1px);
  box-shadow: 0 4px 14px var(--color-brand-glow, rgba(0,0,0,0.28));
  filter: brightness(1.05);
}

.cte-save:active {
  transform: translateY(0);
  box-shadow: 0 1px 4px var(--color-brand-glow, rgba(0,0,0,0.2));
}

/* 主体 */
.cte-body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 20px;
  padding: 20px;
  overflow: hidden;
}

/* 左：骨架预览 */
.cte-preview {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--pv-border-default, var(--border-default));
  border-radius: 14px;
  box-shadow: var(--shadow-md);
  background: var(--pv-surface-page, var(--surface-page));
  user-select: none;
}

/* 可调部位 */
.pv-part {
  position: relative;
  cursor: pointer;
  transition: outline 0.12s;
}

.pv-part:hover {
  outline: 1.5px solid var(--color-brand);
  outline-offset: 1px;
  z-index: 2;
}

.pv-part.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
  z-index: 3;
}

.pv-tag {
  position: absolute;
  left: 50%;
  top: 100%;
  transform: translateX(-50%);
  margin-top: 4px;
  padding: 2px 8px;
  border-radius: 4px;
  background: var(--color-brand);
  color: var(--text-inverse);
  font-size: 11px;
  white-space: nowrap;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.12s;
  z-index: 5;
}

.pv-part:hover .pv-tag,
.pv-part.active .pv-tag {
  opacity: 1;
}

.pv-titlebar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 16px;
  background: var(--pv-surface-card, var(--surface-elevated));
  border-bottom: 1px solid var(--pv-border-default, var(--border-default));
}

.pv-logo-wrap {
  flex: none;
  display: flex;
}

.pv-logo {
  width: 18px;
  height: 18px;
  border-radius: 5px;
  background: var(--pv-color-brand, var(--color-brand));
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
}

.pv-title {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  height: 22px;
  border-radius: 4px;
  padding: 0 8px;
}

.pv-title-text {
  font-size: 12px;
  font-weight: 600;
  color: var(--pv-text-primary, var(--text-primary));
  opacity: 0.8;
  letter-spacing: 0.5px;
}

.pv-window-btn {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--pv-border-strong, var(--border-strong));
}

.pv-main {
  flex: 1;
  display: flex;
  min-height: 0;
}

.pv-sidebar {
  width: 44px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 16px 0;
  background: var(--pv-surface-card, var(--surface-elevated));
  border-right: 1px solid var(--pv-border-default, var(--border-default));
}

.pv-side-item {
  width: 18px;
  height: 18px;
  border-radius: 6px;
  background: var(--pv-text-secondary, var(--text-secondary));
  opacity: 0.6;
}

.pv-side-active {
  background: var(--pv-color-brand, var(--color-brand));
  opacity: 1;
}

.pv-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  min-width: 0;
}

.pv-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid var(--pv-border-default, var(--border-default));
  border-radius: 10px;
  background: var(--pv-surface-card, var(--surface-elevated));
  box-shadow: var(--shadow-sm);
}

.pv-card-head {
  flex: none;
}

.pv-card-title {
  flex: 1;
  height: 14px;
  border-radius: 3px;
  background: var(--pv-text-primary, var(--text-primary));
  opacity: 0.8;
}

.pv-card-btn {
  flex: none;
  width: 52px;
  height: 22px;
  border-radius: 6px;
  background: var(--pv-color-brand, var(--color-brand));
}

.pv-card-list {
  flex: 1;
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
  overflow: hidden;
}

.pv-list-item {
  display: block;
  height: 20px;
  border-radius: 6px;
  background: var(--pv-surface-hover, var(--surface-hover));
}

.pv-list-sel {
  background: var(--pv-color-brand-bg, var(--color-brand-bg));
  box-shadow: inset 0 0 0 1px var(--pv-color-brand, var(--color-brand));
}

/* 普通列表项：白底无强调（不可点击，仅占位） */
.pv-list-plain {
  background: var(--pv-surface-card, #FFFFFF);
  box-shadow: inset 0 0 0 1px var(--pv-border-default, var(--border-default));
  cursor: default;
}

.pv-list-plain:hover,
.pv-list-plain.active {
  outline: none;
}

.pv-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--pv-border-default, var(--border-default));
  background: var(--pv-surface-card, var(--surface-elevated));
}

.pv-foot-btn {
  width: 64px;
  height: 20px;
  border-radius: 5px;
  background: var(--pv-text-secondary, var(--text-secondary));
  opacity: 0.55;
}

.pv-foot-primary {
  background: var(--pv-color-brand, var(--color-brand));
  opacity: 1;
}

.pv-page-dot {
  position: absolute;
  right: 12px;
  bottom: 50px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  border: 1px solid var(--pv-border-strong, var(--border-strong));
  background: var(--pv-surface-page, var(--surface-page));
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

/* 右：内嵌调色板 */
.cte-palette {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 14px;
  padding: 16px;
  box-shadow: var(--shadow-md);
  overflow-y: auto;
}

.cte-palette-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
  font-size: 13px;
  color: var(--text-tertiary);
  line-height: 1.8;
}

.cte-palette-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.cte-palette-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: 0.3px;
}

.cte-title-dot {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  flex-shrink: 0;
}

.cte-palette-reset {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s;
}

.cte-palette-reset:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

.cte-palette-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 8px;
  margin-bottom: 14px;
}

.cte-preset {
  aspect-ratio: 1;
  border: 2px solid transparent;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.12s;
  padding: 0;
}

.cte-preset:hover {
  transform: scale(1.1);
}

.cte-preset.picked {
  border-color: var(--text-primary);
  box-shadow: 0 0 0 2px var(--surface-elevated) inset;
}

.cte-palette-custom {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--border-default);
}

.cte-custom-color {
  flex: none;
  width: 36px;
  height: 36px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
}

.cte-hex-box {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 0 10px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-page);
  transition: border-color 0.15s;
}

.cte-hex-box:focus-within {
  border-color: var(--color-brand);
}

.cte-hex-prefix {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-tertiary);
  font-family: Consolas, "SF Mono", Menlo, "DejaVu Sans Mono", monospace;
  user-select: none;
}

.cte-custom-text {
  flex: 1;
  min-width: 0;
  padding: 8px 0;
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  font-family: Consolas, "SF Mono", Menlo, "DejaVu Sans Mono", monospace;
  letter-spacing: 0.5px;
  outline: none;
}
</style>
