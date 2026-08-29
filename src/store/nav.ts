// 鼠标侧键（后退/前进）的应用内导航。
//
// 侧键若直接触达 WebView 原生浏览器历史，会绕过 vue-router 自身栈导致
// 页面状态与路由不一致（App.vue 曾在捕获阶段整段禁用侧键）。这里改由
// 本模块跨出导航，按"页面语义"而非"走过的每一步"来定返回目标：
//
//   - 后退（goBack）：规则式返回上一级 —— 顶级 Tab/版本页回到启动页；
//     资源详情回资源列表（或依赖链上一详情）；启动页处空操作（永不离开应用）。
//   - 前进（goForward）：重进最后一次后退离开的页面（LIFO redo 栈），
//     无可用前进目标时空操作（"不能在启动页更进一步"）。
//
// redo 栈在用户发起新导航时清空（afterEach 兜底），仅在程序化前后退
// 期间保留，避免跨分支污染。TitleBar 的返回键与侧键共用 goBack，保证
// 两套入口语义一致。

import { vdState } from "./versionDownload";
import router from "../router";
import { goBackDetail } from "./resourceDetail";

/** 程序化导航（本模块发起）计数：afterEach 消耗一枚 → 不清空 redo 栈。
 *  用计数而非布尔开关，因资源详情的 goBackDetail 内部异步 push/replace，
 *  其 afterEach 必须同样被视为"程序化"，否则会错误清空前进分支。 */
let pending = 0;

/** 前进目标：后退越过的页面地址（fullPath），LIFO。 */
let redoStack: string[] = [];

/** 在 App 挂载时调用：挂 afterEach 兜底清理 redo 栈（用户新导航断开前进分支）。 */
export function initNavHistory() {
  router.afterEach(() => {
    if (pending > 0) {
      pending -= 1;
    } else {
      redoStack.length = 0;
    }
  });
}

/** 规则式后退：按当前页面返回上一级；已在启动页返回 false（不可再退）。 */
export function goBack(): boolean {
  const current = router.currentRoute.value;
  const path = current.path;
  if (path === "/") return false; // 启动页不可再退（守卫：永不离开应用）

  // 记录前进目标（向后要能重进刚离开的页面）
  redoStack.push(current.fullPath);

  if (path === "/resource-detail") {
    // 资源详情：弹依赖链栈回上一详情；栈空回资源列表并恢复来源 tab
    pending += 1;
    goBackDetail();
    return true;
  }

  if (path === "/download") {
    // 资源中心-原版游戏：详情态先回列表（清选中版本，停在资源中心页），
    // 列表态回启动页（保持原有顶级 Tab 语义）
    if (vdState.selectedVersion) {
      pending += 1; // 详情→列表的 query 清理视为程序化（保 redo 栈）
      vdState.backToList();
      return true;
    }
    pending += 1;
    void router.push("/");
    return true;
  }

  if (path === "/custom-theme") {
    // 自定义外观编辑器：回设置页（settingsTab 保持外观 tab）
    pending += 1;
    void router.push("/settings");
    return true;
  }

  if (path === "/version-setup") {
    // 版本设置：从版本选择进入 → 回版本选择；其他入口 → 回启动页
    const back = (history.state as { back?: string } | null)?.back ?? "";
    if (back.includes("/version-select")) {
      pending += 1;
      void router.push("/version-select");
      return true;
    }
    pending += 1;
    void router.push("/");
    return true;
  }

  // 其余（顶级 Tab / 版本选择 / 版本设置）→ 回启动页

  pending += 1;
  void router.push("/");
  return true;
}

/** 前进：重进最后一次后退离开的页面；无可前进目标返回 false。 */
export function goForward(): boolean {
  const target = redoStack.pop();
  if (target === undefined) return false;
  pending += 1;
  void router.push(target);
  return true;
}
