<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import TitleBar from "./component/TitleBar.vue";
import AlertDialog from "./component/AlertDialog.vue";
import { dlState, ensureListen, checkInterrupted, waitingUser, decideWaitingTask, type TaskKind } from "./store/download";
import { startMemoryMonitor } from "./store/memory";
import { toastState, showToast } from "./utils/toast";

const router = useRouter();

/** 下载失败错误 dialog（body 只放短提示，完整错误由复制按钮带走）。 */
const errorDialog = ref(false);
const errorMessage = ref("");

dlState.onTaskDone = (message: string, _prevRoute: string, kind?: TaskKind) => {
  // JDK 安装任务：完成处理（自动选中等）由 jdkConfig 专属监听负责，此处不提示不跳转
  if (kind === "jdk") return;
  // 文案由 download.ts 统一格式化（资源名映射 → "下载任务[资源名]完成"，其余原名）
  showToast(message);
  // 不自动退出任务中心：任务中心已是顶级 tab（FAB 移除），完成停留本页查看状态
};

dlState.onTaskError = (_taskName: string, error: string, prevRoute: string) => {
  errorMessage.value = error;
  errorDialog.value = true;
  // 失败任务卡片已由事件处理清理，退出任务中心页回到来源页
  if (prevRoute) router.push(prevRoute);
};

/** 复制挂起任务错误全文（中文首行 + 技术细节）到剪贴板。 */
async function copyWaitingError() {
  const w = waitingUser.value;
  if (!w?.error) return;
  try {
    await navigator.clipboard.writeText(w.error);
    showToast("已复制错误信息");
  } catch {
    showToast("复制失败");
  }
}

function onKeyDown(e: KeyboardEvent) {
  // 禁止打开开发者工具：F12 / Ctrl+Shift+I/J/C / Ctrl+U
  if (
    e.key === "F12" ||
    (e.ctrlKey && e.shiftKey && ["I", "J", "C"].includes(e.key.toUpperCase())) ||
    (e.ctrlKey && e.key.toUpperCase() === "U")
  ) {
    e.preventDefault();
    return;
  }
  // 禁止 Tab 切换控件焦点
  if (e.key === "Tab") {
    e.preventDefault();
    return;
  }
  // 禁止全选（Ctrl+A）
  if (e.ctrlKey && e.key.toLowerCase() === "a") {
    e.preventDefault();
    return;
  }
  // ESC：非启动页面且无弹窗时回到启动页
  if (e.key === "Escape" && !document.querySelector(".dialog-overlay")) {
    const path = router.currentRoute.value.path;
    if (path !== "/") router.push("/");
  }
}

/** 禁用鼠标侧键（后退/前进）触发的 WebView 历史导航。
 *  侧键会绕过 Vue 路由栈直接操作浏览器历史，导致页面状态与路由不一致（UI 栈混乱）。
 *  拦截分两层：
 *  1. 捕获阶段拦截 mousedown/mouseup/pointer 事件（button 3=后退 4=前进）；
 *  2. popstate 兜底：若侧键仍在更底层触发了历史导航，立即把 hash 拉回应用当前路由，
 *     保证 UI 栈稳定。 */
let pinnedHash = "#/";

function onSideButton(e: MouseEvent) {
  if (e.button === 3 || e.button === 4) {
    e.preventDefault();
    e.stopPropagation();
  }
}

function onPopState() {
  const cur = location.hash || "#/";
  if (pinnedHash && cur !== pinnedHash) {
    // 不新增历史条目地拉回 hash，并同步 vue-router 当前路由
    history.replaceState(null, "", pinnedHash);
    router.replace(pinnedHash.slice(1) || "/").catch(() => {});
  }
}

function onContextMenu(e: MouseEvent) {
  e.preventDefault();
}

onMounted(() => {
  pinnedHash = location.hash || "#/";
  router.afterEach(() => {
    pinnedHash = location.hash || "#/";
  });
  document.addEventListener("contextmenu", onContextMenu);
  document.addEventListener("keydown", onKeyDown);
  // 捕获阶段拦截（WebView2 可能不派发冒泡阶段的侧键事件）
  document.addEventListener("mousedown", onSideButton, true);
  document.addEventListener("mouseup", onSideButton, true);
  document.addEventListener("pointerdown", onSideButton, true);
  document.addEventListener("pointerup", onSideButton, true);
  window.addEventListener("popstate", onPopState);
  ensureListen();
  // 崩溃/重启恢复检查：拉取上次未完成的任务（DownloadDetail 横幅询问恢复）
  checkInterrupted();
  // 应用启动即持续监听系统内存（设置页/启动页共用）
  startMemoryMonitor();
});

onUnmounted(() => {
  document.removeEventListener("contextmenu", onContextMenu);
  document.removeEventListener("keydown", onKeyDown);
  document.removeEventListener("mousedown", onSideButton, true);
  document.removeEventListener("mouseup", onSideButton, true);
  document.removeEventListener("pointerdown", onSideButton, true);
  document.removeEventListener("pointerup", onSideButton, true);
  window.removeEventListener("popstate", onPopState);
});
</script>

<template>
  <div class="app-layout">
    <TitleBar />
    <main class="main-content">
      <router-view v-slot="{ Component, route }">
        <Transition :name="route.meta.transition ?? 'route'" mode="out-in">
          <component :is="Component" />
        </Transition>
      </router-view>
    </main>
    <Transition name="toast">
      <div v-if="toastState.msg" class="toast">{{ toastState.msg }}</div>
    </Transition>
    <AlertDialog v-model="errorDialog" title="下载失败" :message="errorMessage" copyable>
      <p>下载失败，已清理该任务下载的文件，请重试</p>
    </AlertDialog>

    <!-- 批量文件下载失败挂起：用户决策 跳过/重试/取消（WaitingForUser） -->
    <Transition name="fade">
      <div v-if="waitingUser" class="dialog-overlay" @click.self.stop>
        <div class="waiting-dialog">
          <h3 class="wd-title">下载失败</h3>
          <p class="wd-file">{{ waitingUser.file_name }}</p>
          <p class="wd-err">{{ waitingUser.error }}</p>
          <div class="wd-actions">
            <button class="wd-btn" @click="copyWaitingError">复制错误信息</button>
            <button class="wd-btn wd-retry" @click="decideWaitingTask('retry')">重试</button>
            <button class="wd-btn" @click="decideWaitingTask('skip')">跳过</button>
            <button class="wd-btn wd-cancel" @click="decideWaitingTask('cancel')">取消任务</button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style>
html, body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  overscroll-behavior-x: none;
  background: transparent;
  height: 100%;
  width: 100%;
  /* 全局禁止选中（输入框/文本域内例外） */
  -webkit-user-select: none;
  user-select: none;
}

/* 桌面应用 UI：点击聚焦不显示浏览器默认焦点框（黑色 outline）。
   键盘/编程聚焦保留品牌色可见轮廓（可访问性底线；Tab 键拦截时仅对编程焦点生效，无害）。 */
:focus { outline: none; }
:focus-visible { outline: 2px solid var(--color-brand); outline-offset: 2px; }

#app {
  height: 100%;
  width: 100%;
}

/* 输入控件保持可选中/可编辑 */
input, textarea {
  -webkit-user-select: text;
  user-select: text;
}

/* placeholder 全局统一（组件级自定义可覆盖） */
::placeholder {
  color: var(--text-tertiary);
  opacity: 1;
}

:root {
  --titlebar-h: 48px;
  font-family: Inter, "Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", "PingFang SC", sans-serif;
  font-size: 14px;
  line-height: 1.5;
  font-weight: 400;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;

  /* brand —— 默认主题 Grass（草方块绿）：取自 Minecraft 草方块/翡翠。
    多主题体系：data-theme 覆盖块见下（end 末影紫 / nether 下界橙 / ice 冰霜青）。
    accent 小面积使用（标题栏非色块，激活态/启动按钮/焦点才用品牌色） */
  --color-brand: #059669;
  --color-brand-dark: #047857;
  --color-brand-hover: #047857;
  --color-brand-bg: rgba(5, 150, 105, 0.08);
  --color-brand-glow: rgba(5, 150, 105, 0.22);
  --color-brand-gradient: linear-gradient(135deg, #059669, #10B981);

  /* text */
  --text-primary: #1F2937;
  --text-secondary: #6B7280;
  --text-tertiary: #9CA3AF;
  --text-inverse: #FFFFFF;

  /* surface */
  --surface-page: #F2F6F3;
  --surface-header: var(--surface-elevated);
  --surface-card: #FFFFFF;
  --surface-elevated: #FFFFFF;
  --surface-input: var(--surface-card);
  --surface-hover: rgba(15, 23, 42, 0.05);

  /* border */
  --border-default: #DFE5E1;
  --border-strong: #CBD5D1;

  /* shadow */
  --shadow-sm: 0 1px 2px rgba(15, 23, 42, 0.06);
  --shadow-md: 0 2px 8px rgba(15, 23, 42, 0.08);
  --shadow-lg: 0 4px 16px rgba(15, 23, 42, 0.10);

  /* 语义色（错误/成功/警示；危险操作与状态统一收口） */
  --color-danger: #E5484D;
  --color-danger-bg: rgba(229, 72, 77, 0.1);
  --color-success: #10B981;
  --color-warning: #D97706;

  /* ModLoader 标签色（fabric/forge/neoforge/quilt 区分，随主题覆盖切换） */
  --ld-fabric: #D97706;
  --ld-forge: #DC2626;
  --ld-neoforge: #16A34A;
  --ld-quilt: #7C3AED;

  /* 弹窗遮罩 */
  --overlay: rgba(15, 23, 42, 0.5);
  --toast-bg: #1F2937;

  /* radius */
  --radius-sm: 6px;
  --radius-md: 8px;
  --radius-lg: 12px;

  /* scrollbar */
  --scrollbar-thumb: #CBD5D1;
}

/* ===== 主题 End（末影紫）：取自 Minecraft 末地 ===== */
[data-theme="end"] {
  --color-brand: #7C3AED;
  --color-brand-dark: #6D28D9;
  --color-brand-hover: #6D28D9;
  --color-brand-bg: rgba(124, 58, 237, 0.08);
  --color-brand-glow: rgba(124, 58, 237, 0.2);
  --color-brand-gradient: linear-gradient(135deg, #7C3AED, #A855F7);

  --text-primary: #241B45;
  --text-secondary: #6B6580;
  --text-tertiary: #9A93B0;

  --surface-page: #F6F3FB;
  --surface-hover: rgba(36, 27, 69, 0.06);

  --border-default: #E3DEF1;
  --border-strong: #D2CBEA;

  --shadow-sm: 0 1px 2px rgba(36, 27, 69, 0.06);
  --shadow-md: 0 2px 8px rgba(36, 27, 69, 0.08);
  --shadow-lg: 0 4px 16px rgba(36, 27, 69, 0.10);

  --scrollbar-thumb: #D2CBEA;

  --ld-fabric: #B45309;
  --ld-forge: #E11D48;
  --ld-neoforge: #059669;
  --ld-quilt: #A855F7;
}

/* ===== 主题 Nether（下界橙/烈焰）：取自 Minecraft 下界 ===== */
[data-theme="nether"] {
  --color-brand: #EA580C;
  --color-brand-dark: #C2410C;
  --color-brand-hover: #C2410C;
  --color-brand-bg: rgba(234, 88, 12, 0.08);
  --color-brand-glow: rgba(234, 88, 12, 0.2);
  --color-brand-gradient: linear-gradient(135deg, #EA580C, #F97316);

  --text-primary: #292524;
  --text-secondary: #78716C;
  --text-tertiary: #A8A29E;

  --surface-page: #FBF5F0;
  --surface-hover: rgba(41, 37, 36, 0.05);

  --border-default: #EDE3DC;
  --border-strong: #DED2C8;

  --shadow-sm: 0 1px 2px rgba(41, 37, 36, 0.06);
  --shadow-md: 0 2px 8px rgba(41, 37, 36, 0.08);
  --shadow-lg: 0 4px 16px rgba(41, 37, 36, 0.10);

  --scrollbar-thumb: #DED2C8;

  --ld-fabric: #EA580C;
  --ld-forge: #DC2626;
  --ld-neoforge: #65A30D;
  --ld-quilt: #C026D3;
}

/* ===== 主题 Ice（冰霜青）：取自 Minecraft 冰霜/钻石 ===== */
[data-theme="ice"] {
  --color-brand: #0EA5E9;
  --color-brand-dark: #0284C7;
  --color-brand-hover: #0284C7;
  --color-brand-bg: rgba(14, 165, 233, 0.08);
  --color-brand-glow: rgba(14, 165, 233, 0.2);
  --color-brand-gradient: linear-gradient(135deg, #0EA5E9, #38BDF8);

  --text-primary: #1E293B;
  --text-secondary: #64748B;
  --text-tertiary: #94A3B8;

  --surface-page: #F2F7FA;
  --surface-hover: rgba(15, 23, 42, 0.05);

  --border-default: #DCE7EE;
  --border-strong: #C6D6E0;

  --shadow-sm: 0 1px 2px rgba(15, 23, 42, 0.06);
  --shadow-md: 0 2px 8px rgba(15, 23, 42, 0.08);
  --shadow-lg: 0 4px 16px rgba(15, 23, 42, 0.10);

  --scrollbar-thumb: #C6D6E0;

  --ld-fabric: #F59E0B;
  --ld-forge: #F43F5E;
  --ld-neoforge: #10B981;
  --ld-quilt: #8B5CF6;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: var(--scrollbar-thumb);
  border-radius: 3px;
}

.app-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  border-radius: var(--radius-md);
  background-color: var(--surface-page);
  color: var(--text-primary);
}

.main-content {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  background-color: var(--surface-page);
}

/* Route transitions — simple opacity fade, normal flow, no scroll jitter */
.route-enter-active,
.route-leave-active,
.slide-left-enter-active,
.slide-left-leave-active,
.slide-right-enter-active,
.slide-right-leave-active {
  transition: opacity 0.18s ease;
}
.route-enter-from,
.slide-left-enter-from,
.slide-right-enter-from { opacity: 0; }
.route-leave-to,
.slide-left-leave-to,
.slide-right-leave-to { opacity: 0; }

.toast {
  position: fixed; bottom: 80px; left: 50%; transform: translateX(-50%);
  z-index: 9999;
  padding: 10px 24px; border-radius: var(--radius-md);
  background: var(--toast-bg); color: #fff;
  font-size: 13px; font-weight: 500;
  white-space: nowrap; box-shadow: var(--shadow-lg);
  pointer-events: none;
}
.toast-enter-active { transition: all 0.3s ease-out; }
.toast-leave-active { transition: all 0.2s ease-in; }
.toast-enter-from { opacity: 0; transform: translateX(-50%) translateY(16px); }
.toast-leave-to   { opacity: 0; transform: translateX(-50%) translateY(16px); }

/* 文件下载失败等待决策弹窗（WaitingForUser） */
.dialog-overlay {
  position: fixed; inset: 0; z-index: 200;
  display: flex; align-items: center; justify-content: center;
  background: var(--overlay);
}

.waiting-dialog {
  width: 380px; max-width: calc(100vw - 48px);
  padding: 20px 22px;
  background: var(--surface-card);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
}

.wd-title { margin: 0 0 10px; font-size: 15px; color: var(--text-primary); }

.wd-file {
  margin: 0 0 6px; font-size: 13px; font-weight: 600;
  color: var(--text-primary); word-break: break-all;
}

.wd-err {
  margin: 0 0 16px; font-size: 12px; color: var(--text-tertiary);
  word-break: break-all; max-height: 64px; overflow-y: auto;
}

.wd-actions { display: flex; gap: 8px; justify-content: flex-end; }

.wd-btn {
  padding: 6px 14px; font-size: 13px; cursor: pointer;
  border: 1px solid var(--border-strong); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary);
  transition: all 0.15s;
}

.wd-btn:hover { border-color: var(--color-brand); }
.wd-retry { background: var(--color-brand); border-color: var(--color-brand); color: #fff; }
.wd-retry:hover { opacity: 0.9; }
.wd-cancel { color: var(--color-danger); }
.wd-cancel:hover { border-color: var(--color-danger); }

.fade-enter-active, .fade-leave-active { transition: opacity 0.15s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

/* v-tooltip 气泡（替换 UA 原生 title tooltip） */
.elytra-tooltip {
  position: fixed;
  z-index: 99999;
  max-width: 320px;
  padding: 6px 10px;
  background: var(--toast-bg);
  color: #fff;
  font-size: 12px;
  line-height: 1.5;
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-lg);
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transition: opacity 0.12s ease;
  word-break: break-word;
  white-space: pre-line;
}

.elytra-tooltip.visible {
  opacity: 1;
  visibility: visible;
}
</style>
