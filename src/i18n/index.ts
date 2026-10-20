import { computed, ref } from "vue";
import { createI18n } from "vue-i18n";

import enUS from "./messages/en-US";
import zhCN from "./messages/zh-CN";

export type LanguageSetting = "auto" | "zh-CN" | "en-US";
export type ResolvedLocale = "zh-CN" | "en-US";

const STORAGE_KEY = "elytra.language";

function readSetting(): LanguageSetting {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "auto" || saved === "zh-CN" || saved === "en-US") return saved;
  } catch {
    // 存储不可用时按自动处理
  }
  return "auto";
}

export function detectLocale(): ResolvedLocale {
  const lang = typeof navigator === "undefined" ? "zh-CN" : navigator.language;
  return lang.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

export const languageSetting = ref<LanguageSetting>(readSetting());
const initialLocale = languageSetting.value === "auto" ? detectLocale() : languageSetting.value;

export const i18n = createI18n({
  legacy: false,
  globalInjection: true,
  locale: initialLocale,
  fallbackLocale: "zh-CN",
  messages: { "zh-CN": zhCN, "en-US": enUS },
  missingWarn: import.meta.env.DEV,
  fallbackWarn: import.meta.env.DEV,
});

export const currentLocale = computed<ResolvedLocale>(
  () => i18n.global.locale.value as ResolvedLocale,
);

type TranslateFn = (key: string, params?: unknown, count?: number) => string;

/** 组件外调用（store / 工具函数）；模板内优先用 useI18n 的 t。 */
export function t(key: string, params?: Record<string, unknown> | number, count?: number): string {
  const fn = i18n.global.t as unknown as TranslateFn;
  return count === undefined ? fn(key, params) : fn(key, params, count);
}

/** 仅在 key 存在时翻译；用于后端 @i18n 错误的降级判断。 */
export function translate(
  key: string,
  params?: Record<string, unknown>,
  count?: number,
): string | null {
  const te = i18n.global.te as unknown as (k: string, locale?: string) => boolean;
  if (!te(key, currentLocale.value) && !te(key, "zh-CN")) return null;
  return t(key, params, count);
}

function applyLocaleMeta() {
  if (typeof document === "undefined") return;
  document.documentElement.lang = currentLocale.value;
  document.title = t("common.appTitle");
}

export function initI18n() {
  applyLocaleMeta();
}

/** 文本长度变化会触发整页重排；短暂关掉过渡，避免 hover/主题动画把重排放大成抖动。 */
function suppressTransitions() {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.add("i18n-switching");
  const clear = () => root.classList.remove("i18n-switching");
  if (typeof requestAnimationFrame === "function") {
    requestAnimationFrame(() => requestAnimationFrame(clear));
  }
  // 窗口不可见时 rAF 可能被暂停；兜底保证类名不会永久残留。
  setTimeout(clear, 120);
}

export function setLanguage(setting: LanguageSetting) {
  suppressTransitions();
  languageSetting.value = setting;
  try {
    localStorage.setItem(STORAGE_KEY, setting);
  } catch {
    // 存储不可用时仅本会话生效
  }
  i18n.global.locale.value = setting === "auto" ? detectLocale() : setting;
  applyLocaleMeta();
}
