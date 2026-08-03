// 全局轻提示（App.vue 渲染；页面 import showToast 调用）。
// 替代原先 App.vue 全局 + Launch/Settings 本地 + (window as any).__showToast 的多份实现。
import { reactive } from "vue";

export const toastState = reactive({ msg: "" });

let timer: number | undefined;

export function showToast(msg: string, durationMs = 2500) {
  toastState.msg = msg;
  clearTimeout(timer);
  timer = window.setTimeout(() => (toastState.msg = ""), durationMs);
}
