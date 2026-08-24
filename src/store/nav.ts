// 应用内导航历史栈：为鼠标侧键（后退/前进）提供精确的 Vue 路由导航。
//
// 侧键若直接触达 WebView 原生浏览器历史，会绕过 vue-router 自身的栈
// 导致页面状态与路由不一致（UI 栈混乱，App.vue 曾整段禁用侧键）。
// 这里改走自管栈：把每次真实导航的完整地址（fullPath）记录进栈，
// 侧键只在栈内移动指针并 replace 回放，不新增浏览器历史条目，也不
// 可能越出应用 —— 在根路由（/）处后退即为空操作，永不离开应用。

import router from "../router";

/** 导航栈快照：history 为地址序列，index 指向当前所在位置。 */
export interface NavStack {
  history: string[];
  index: number;
  get canBack(): boolean;
  get canForward(): boolean;
}

/** 供 App.vue 只读使用的当前栈。 */
export const navStack: NavStack = {
  history: [],
  index: -1,
  get canBack() {
    return this.index > 0;
  },
  get canForward() {
    return this.index >= 0 && this.index < this.history.length - 1;
  },
};

/** 回放中置 true：afterEach 不再把回放目标当新导航入栈（指针已手动移动）。 */
let replaying = false;

/** 在 App 挂载时调用：以当前地址初始化栈，并挂 afterEach 记录导航。 */
export function initNavHistory() {
  const start = router.currentRoute.value.fullPath || "/";
  navStack.history = [start];
  navStack.index = 0;

  router.afterEach((to) => {
    if (replaying) return;
    const p = to.fullPath;
    // 与当前重复（含 replace 同址刷新）不入栈；否则从当前点截断前进分支后入栈
    if (navStack.history[navStack.index] !== p) {
      navStack.history.length = navStack.index + 1;
      navStack.history.push(p);
      navStack.index += 1;
    }
  });
}

function replayTo(target: string) {
  replaying = true;
  // push 回放：不依赖浏览器历史，回放期间 afterEach 被 replaying 跳过
  // （指针已在调用方移动），重登同一地址也不会重复入栈
  const done = router.push(target).finally(() => {
    replaying = false;
  });
  void done;
}

/** 侧键后退：栈内前移回去；已在根（Launcher）则空操作。 */
export function sideBack(): boolean {
  if (!navStack.canBack) return false;
  navStack.index -= 1;
  replayTo(navStack.history[navStack.index]);
  return true;
}

/** 侧键前进：栈内后移重放；已到最后一项则空操作。 */
export function sideForward(): boolean {
  if (!navStack.canForward) return false;
  navStack.index += 1;
  replayTo(navStack.history[navStack.index]);
  return true;
}
