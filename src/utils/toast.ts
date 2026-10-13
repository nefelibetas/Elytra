// App.vue 渲染，页面 import 调用；默认时长 2500ms。
import { reactive } from "vue";

export const toastState = reactive({ msg: "" });

let timer: number | undefined;

export function showToast(msg: string, durationMs = 2500) {
  toastState.msg = msg;
  clearTimeout(timer);
  timer = window.setTimeout(() => (toastState.msg = ""), durationMs);
}
