// 外观主题：多亮色主题选择（Grass 草绿默认 / End 末影紫 / Nether 下界橙 / Ice 冰霜青）
// + 用户自定义外观（核心色板编辑器，可持久化）。
// 持久化：localStorage（纯前端视觉状态；对齐 selectedVersion 先例）。
// 实现：向 <html> 注入 data-theme="<id>" 触发 App.vue 对应主题 token 覆盖块；
// 无 data-theme 属性 = 默认主题（grass）。
// 自定义主题：id 形如 `custom-<n>`，其变量覆盖块由本模块动态注入 <style>（无 App.vue 静态块）。

import { ref, watch } from "vue";

export type ThemeId = "grass" | "end" | "nether" | "ice" | `custom-${string}`;

export const THEMES: { id: ThemeId; label: string; desc: string; swatch: string[] }[] = [
  { id: "grass", label: "草方块绿", desc: "Minecraft 草方块 / 翡翠", swatch: ["#059669", "#10B981", "#F2F6F3", "#1F2937"] },
  { id: "end", label: "末影紫", desc: "末地 / 紫颂果", swatch: ["#7C3AED", "#A855F7", "#F6F3FB", "#241B45"] },
  { id: "nether", label: "下界橙", desc: "烈焰 / 下界岩", swatch: ["#EA580C", "#F97316", "#FBF5F0", "#292524"] },
  { id: "ice", label: "冰霜青", desc: "冰霜 / 钻石", swatch: ["#0EA5E9", "#38BDF8", "#F2F7FA", "#1E293B"] },
];

/** 自定义主题 id 前缀（与内置主题在 data-theme 与 localStorage 键上区分）。 */
const CUSTOM_PREFIX = "custom-";

// ── 可编辑的核心色板变量（品牌系 + 文本 + 浅表面 + 边框）──
// 品牌衍生物（dark/bg/glow/gradient）由品牌主色自动派生，不出现在编辑面板。
export interface CustomThemeVars {
  "--color-brand": string;
  "--text-primary": string;
  "--text-secondary": string;
  "--text-tertiary": string;
  "--surface-page": string;
  "--surface-hover": string;
  "--border-default": string;
  "--border-strong": string;
}

export interface CustomTheme {
  id: string;
  name: string;
  /** 副标题（卡片/编辑器展示；旧数据可能缺失） */
  desc?: string;
  vars: CustomThemeVars;
}

const STORAGE_KEY = "elytra.theme";
const CUSTOMS_KEY = "elytra.customThemes";

/** 内置主题 → 可作为自定义起点的完整可编辑变量值（新建自定义时复制）。 */
export function builtinVars(id: ThemeId): CustomThemeVars {
  switch (id) {
    case "grass":
      return {
        "--color-brand": "#059669",
        "--text-primary": "#1F2937",
        "--text-secondary": "#6B7280",
        "--text-tertiary": "#9CA3AF",
        "--surface-page": "#F2F6F3",
        "--surface-hover": "rgba(15, 23, 42, 0.05)",
        "--border-default": "#DFE5E1",
        "--border-strong": "#CBD5D1",
      };
    case "end":
      return {
        "--color-brand": "#7C3AED",
        "--text-primary": "#241B45",
        "--text-secondary": "#6B6580",
        "--text-tertiary": "#9A93B0",
        "--surface-page": "#F6F3FB",
        "--surface-hover": "rgba(36, 27, 69, 0.06)",
        "--border-default": "#E3DEF1",
        "--border-strong": "#D2CBEA",
      };
    case "nether":
      return {
        "--color-brand": "#EA580C",
        "--text-primary": "#292524",
        "--text-secondary": "#78716C",
        "--text-tertiary": "#A8A29E",
        "--surface-page": "#FBF5F0",
        "--surface-hover": "rgba(41, 37, 36, 0.05)",
        "--border-default": "#EDE3DC",
        "--border-strong": "#DED2C8",
      };
    default: // ice 及未知回退
      return {
        "--color-brand": "#0EA5E9",
        "--text-primary": "#1E293B",
        "--text-secondary": "#64748B",
        "--text-tertiary": "#94A3B8",
        "--surface-page": "#F2F7FA",
        "--surface-hover": "rgba(15, 23, 42, 0.05)",
        "--border-default": "#DCE7EE",
        "--border-strong": "#C6D6E0",
      };
  }
}

/** 旧版值兼容：暗色模式时代遗留的 light/dark/system → 默认 grass。 */
function normalize(raw: string | null): ThemeId {
  if (raw && THEMES.some((t) => t.id === raw)) return raw as ThemeId;
  if (raw && raw.startsWith(CUSTOM_PREFIX)) return raw as ThemeId;
  return "grass";
}

export const themeId = ref<ThemeId>(normalize(localStorage.getItem(STORAGE_KEY)));

// ── 自定义主题集合 ──

/** 已保存的自定义主题（id → 主题）。 */
export const customThemes = ref<Record<string, CustomTheme>>(loadCustoms());

function loadCustoms(): Record<string, CustomTheme> {
  try {
    const raw = localStorage.getItem(CUSTOMS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, CustomTheme>;
    // 结构校验：仅接受合理的 vars（缺字段的丢弃）
    const required: (keyof CustomThemeVars)[] = [
      "--color-brand", "--text-primary", "--text-secondary", "--text-tertiary",
      "--surface-page", "--surface-hover", "--border-default", "--border-strong",
    ];
    const out: Record<string, CustomTheme> = {};
    for (const [id, t] of Object.entries(parsed)) {
      if (!t || typeof t.id !== "string" || typeof t.name !== "string" || typeof t.vars !== "object") continue;
      if (required.every((k) => typeof t.vars[k] === "string")) {
        // 旧数据无 desc：补默认
        if (typeof t.desc !== "string") t.desc = "自定义配色";
        out[id] = t;
      }
    }
    return out;
  } catch {
    return {};
  }
}

function saveCustoms() {
  localStorage.setItem(CUSTOMS_KEY, JSON.stringify(customThemes.value));
}

/** 新建自定义主题：从指定主题复制初始色板（内置或现有自定义，默认从当前主题）。
 *  返回新主题；不自动切换（由外部决定是否激活）。 */
export function createCustomTheme(from: ThemeId = themeId.value): CustomTheme {
  const base = THEMES.some((t) => t.id === from)
    ? builtinVars(from)
    : (customThemes.value[from]?.vars ?? builtinVars("grass"));
  // 唯一 id
  let n = Object.keys(customThemes.value).length + 1;
  let id = `${CUSTOM_PREFIX}${n}`;
  while (customThemes.value[id]) {
    n += 1;
    id = `${CUSTOM_PREFIX}${n}`;
  }
  const t: CustomTheme = {
    id,
    name: `自定义 ${n}`,
    desc: "自定义配色",
    vars: { ...base },
  };
  customThemes.value = { ...customThemes.value, [id]: t };
  saveCustoms();
  return t;
}

/** 更新自定义主题变量（改名/调色）：立即保存并应用（若正在使用）。 */
export function updateCustomTheme(id: string, patch: Partial<CustomTheme>) {
  const t = customThemes.value[id];
  if (!t) return;
  customThemes.value = {
    ...customThemes.value,
    [id]: { ...t, ...patch, vars: patch.vars ? { ...t.vars, ...patch.vars } : t.vars },
  };
  saveCustoms();
  if (themeId.value === id) applyTheme();
}

/** 删除自定义主题：若正在使用则回退到默认主题。 */
export function deleteCustomTheme(id: string) {
  const { [id]: _, ...rest } = customThemes.value;
  customThemes.value = rest;
  saveCustoms();
  removeCustomStyle(id);
  if (themeId.value === id) {
    themeId.value = "grass";
    applyTheme();
  }
}

/** 当前主题 id 是否为自定义（收 string：调用方常传任意 id 字符串）。 */
export function isCustom(id: string): boolean {
  return id.startsWith(CUSTOM_PREFIX);
}

// ── 主题应用与持久化 ──

/** 品牌主色 → 品牌衍生物变量（dark/hover/bg/glow/gradient）。
 *  面板只编辑品牌主色，其余自动派生，保证配色协调。 */
function deriveBrandVars(brand: string): Record<string, string> {
  // 解析 hex 做 darken + 透明度（bg/glow 固定透明度）
  const hex = brand.replace("#", "");
  if (!/^[0-9a-fA-F]{6}$/.test(hex)) {
    // 非法色值：用渐变更安全的默认（rgba 均按 0.08/0.2）
    return {
      "--color-brand-dark": brand,
      "--color-brand-hover": brand,
      "--color-brand-bg": "rgba(5, 150, 105, 0.08)",
      "--color-brand-glow": "rgba(5, 150, 105, 0.22)",
      "--color-brand-gradient": `linear-gradient(135deg, ${brand}, ${brand})`,
    };
  }
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  const darken = (f: number) => `#${[r, g, b].map((v) => Math.round(v * f)).map((v) => v.toString(16).padStart(2, "0")).join("")}`;
  // 渐变第二色 = 品牌色与白色 25% 混合提亮
  const lighten = (f: number) =>
    `#${[r, g, b].map((v) => Math.round(v + (255 - v) * f)).map((v) => v.toString(16).padStart(2, "0")).join("")}`;
  return {
    "--color-brand-dark": darken(0.85),
    "--color-brand-hover": darken(0.85),
    "--color-brand-bg": `rgba(${r}, ${g}, ${b}, 0.08)`,
    "--color-brand-glow": `rgba(${r}, ${g}, ${b}, 0.2)`,
    "--color-brand-gradient": `linear-gradient(135deg, ${brand}, ${lighten(0.28)})`,
  };
}

/** 动态注入自定义主题覆盖块（<style id="custom-theme-<id>">）。
 *  一个主题一条规则；已存在则覆盖，不用则移除。 */
export function applyCustomThemeStyle(id: string) {
  const t = customThemes.value[id];
  if (!t) return;
  const derived = deriveBrandVars(t.vars["--color-brand"]);
  const lines = [
    ...Object.entries(t.vars).map(([k, v]) => `  ${k}: ${v};`),
    // 品牌衍生物（面板不直接编辑，自动派生注入）
    ...Object.entries(derived).map(([k, v]) => `  ${k}: ${v};`),
  ].join("\n");
  const css = `[data-theme="${id}"] {\n${lines}\n}`;
  let el = document.getElementById(`custom-theme-${id}`) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = `custom-theme-${id}`;
    document.head.appendChild(el);
  }
  el.textContent = css;
}

function removeCustomStyle(id: string) {
  document.getElementById(`custom-theme-${id}`)?.remove();
}

/** 应用当前主题到 <html data-theme>：内置主题由 App.vue 静态块处理；
 *  自定义主题需先注入动态规则再设属性（挂载前调用一次）。 */
export function applyTheme() {
  if (isCustom(themeId.value)) {
    applyCustomThemeStyle(themeId.value);
  }
  document.documentElement.dataset.theme = themeId.value;
}

/** 激活自定义主题（切换 + 注入 + 持久化）。 */
export function activateCustomTheme(id: string) {
  if (!customThemes.value[id]) return;
  applyCustomThemeStyle(id);
  themeId.value = id as ThemeId;
}

watch(themeId, (id) => {
  localStorage.setItem(STORAGE_KEY, id);
  applyTheme();
});

/** 挂载时确保已存在的自定义主题样式被注入（页面刷新后 <style> 不持久）。
 *  返回当前是否为自定义主题（供 UI 高亮）。 */
export function ensureCustomThemesApplied() {
  for (const id of Object.keys(customThemes.value)) {
    if (customThemes.value[id]) applyCustomThemeStyle(id);
  }
  if (isCustom(themeId.value)) applyTheme();
}