// 鼠标侧键/TitleBar 返回键的应用内导航：规则式 goBack，不走 WebView 原生历史（会绕过 vue-router）；TitleBar 返回键亦调用 goBack。goForward 走 LIFO redo 栈，用户新导航时清空，仅程序化前后退保留。

import { vdState } from "../task/versionDownload";
import router from "../../router";
import { goBackDetail } from "../community/resourceDetail";

/** 程序化导航计数：afterEach 消耗一枚则不清空 redo 栈（计数而非布尔，goBackDetail 内部异步 push/replace）。 */
let pending = 0;

/** 前进目标：后退越过的页面地址（fullPath），LIFO。 */
let redoStack: string[] = [];

/** App 挂载时调用：afterEach 兜底清理 redo 栈（用户新导航断开前进分支）。 */
export function initNavHistory() {
  router.afterEach(() => {
    if (pending > 0) {
      pending -= 1;
    } else {
      redoStack.length = 0;
    }
  });
}

export function goBack(): boolean {
  const current = router.currentRoute.value;
  const path = current.path;
  if (path === "/") return false; // 启动页不可再退

  redoStack.push(current.fullPath);

  if (path === "/resource-detail") {
    pending += 1;
    goBackDetail();
    return true;
  }

  if (path === "/download") {
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
    pending += 1;
    void router.push("/settings");
    return true;
  }

  if (path === "/world-preview") {
    const query = current.query;
    const version = String(query.version ?? "");
    pending += 1;
    if (query.from === "setup" && version) {
      void router.push({ path: "/version-setup", query: { name: version } });
    } else {
      void router.push("/");
    }
    return true;
  }

  if (path === "/version-setup") {
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

  pending += 1;
  void router.push("/");
  return true;
}

export function goForward(): boolean {
  const target = redoStack.pop();
  if (target === undefined) return false;
  pending += 1;
  void router.push(target);
  return true;
}
