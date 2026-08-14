// 外观主题：多亮色主题选择（Grass 草绿默认 / End 末影紫 / Nether 下界橙 / Ice 冰霜青）。
// 持久化：localStorage（纯前端视觉状态；对齐 selectedVersion 先例）。
// 实现：向 <html> 注入 data-theme="<id>" 触发 App.vue 对应主题 token 覆盖块；
// 无 data-theme 属性 = 默认主题（grass）。

import { ref, watch } from "vue";

export type ThemeId = "grass" | "end" | "nether" | "ice";

export const THEMES: { id: ThemeId; label: string; desc: string; swatch: string[] }[] = [
  { id: "grass", label: "草方块绿", desc: "Minecraft 草方块 / 翡翠", swatch: ["#059669", "#10B981", "#F2F6F3", "#1F2937"] },
  { id: "end", label: "末影紫", desc: "末地 / 紫颂果", swatch: ["#7C3AED", "#A855F7", "#F6F3FB", "#241B45"] },
  { id: "nether", label: "下界橙", desc: "烈焰 / 下界岩", swatch: ["#EA580C", "#F97316", "#FBF5F0", "#292524"] },
  { id: "ice", label: "冰霜青", desc: "冰霜 / 钻石", swatch: ["#0EA5E9", "#38BDF8", "#F2F7FA", "#1E293B"] },
];

const STORAGE_KEY = "elytra.theme";

/** 旧版值兼容：暗色模式时代遗留的 light/dark/system → 默认 grass。 */
function normalize(raw: string | null): ThemeId {
  if (raw && THEMES.some((t) => t.id === raw)) return raw as ThemeId;
  return "grass";
}

export const themeId = ref<ThemeId>(normalize(localStorage.getItem(STORAGE_KEY)));

/** 应用当前主题到 <html data-theme>（挂载前调用一次；无属性 = 默认主题）。 */
export function applyTheme() {
  document.documentElement.dataset.theme = themeId.value;
}

watch(themeId, (id) => {
  localStorage.setItem(STORAGE_KEY, id);
  applyTheme();
});
