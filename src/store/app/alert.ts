// 全局错误弹窗状态（AlertDialog）：启动/登录/皮肤/披风等多处共用，跨 store 引用。
import { reactive } from "vue";

export const alertState = reactive({
  show: false,
  msg: "",
  /** 弹窗是否为复杂错误（需要复制按钮） */
  copyable: false,
});

export function showError(msg: string, copyable = false) {
  alertState.msg = msg;
  alertState.copyable = copyable;
  alertState.show = true;
}
