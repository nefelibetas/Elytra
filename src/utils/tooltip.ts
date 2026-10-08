// v-tooltip="'文本'" 自定义指令：替换 UA title 原生 tooltip（主题化气泡）。
// 用途：操作按钮说明 + 截断文本 hover 看全（卡片标题/路径/文件名）。
// 行为：hover 400ms 延迟显示、移出 100ms 隐藏、**跟随鼠标**（右下 12px，
// 视口右/下空间不足时翻转到鼠标左/上）、scroll/resize 时跟随重定位、
// 空值不显示、自动移除原生 title。
import type { Directive } from "vue";

interface TooltipTarget extends HTMLElement {
  __ttCleanup?: () => void;
  /** 最新提示文本：hover 时由它取值。
   *  指令的 binding 对象每次更新都会**重建**，闭包若直接捕获 `binding.value`，
   *  hover 显示的会是「首次绑定」时的快照——动态文案（如按登录状态给出不同的引导）
   *  会一直停在旧值。故在此存一份可变的最新值。 */
  __ttText?: string;
}

const TIP_CLASS = "elytra-tooltip";
const SHOW_DELAY = 400;
const HIDE_DELAY = 100;
const OFFSET_X = 12;
const OFFSET_Y = 14;

let tipEl: HTMLElement | null = null;
let showTimer: number | undefined;
let hideTimer: number | undefined;
let currentTarget: TooltipTarget | null = null;
let currentText = "";
let mouseX = 0;
let mouseY = 0;

function ensureTip(): HTMLElement {
  if (!tipEl || !tipEl.isConnected) {
    tipEl = document.createElement("div");
    tipEl.className = TIP_CLASS;
    document.body.appendChild(tipEl);
  }
  return tipEl;
}

function reposition() {
  if (!tipEl || !currentTarget || !tipEl.classList.contains("visible")) return;
  // 目标已脱离文档 → 隐藏
  if (!currentTarget.isConnected) {
    tipEl.classList.remove("visible");
    currentTarget = null;
    return;
  }
  const tw = tipEl.offsetWidth;
  const th = tipEl.offsetHeight;
  // 默认鼠标右下；空间不足翻转到左/上；再兜底视口内
  let left = mouseX + OFFSET_X;
  let top = mouseY + OFFSET_Y;
  if (left + tw > window.innerWidth - 4) left = mouseX - tw - OFFSET_X;
  if (top + th > window.innerHeight - 4) top = mouseY - th - OFFSET_Y;
  left = Math.max(4, left);
  top = Math.max(4, top);
  tipEl.style.left = `${left}px`;
  tipEl.style.top = `${top}px`;
}

function show(target: TooltipTarget, text: string) {
  if (!text) return;
  clearTimeout(hideTimer);
  showTimer = setTimeout(() => {
    currentTarget = target;
    currentText = text;
    const tip = ensureTip();
    tip.textContent = text;
    tip.classList.add("visible");
    reposition();
  }, SHOW_DELAY);
}

function hide() {
  clearTimeout(showTimer);
  hideTimer = setTimeout(() => {
    tipEl?.classList.remove("visible");
    currentTarget = null;
  }, HIDE_DELAY);
}

/** 全局记录鼠标位置（capture 捕获所有目标）；tooltip 可见时跟随移动。 */
window.addEventListener(
  "mousemove",
  (e) => {
    mouseX = e.clientX;
    mouseY = e.clientY;
    if (tipEl?.classList.contains("visible")) reposition();
  },
  true,
);

/** scroll/resize 全局跟随（capture 捕获所有滚动容器）。 */
window.addEventListener(
  "scroll",
  () => {
    if (currentTarget && tipEl?.classList.contains("visible")) reposition();
  },
  true,
);
window.addEventListener("resize", () => {
  if (currentTarget && tipEl?.classList.contains("visible")) reposition();
});

const tooltip: Directive<HTMLElement, string> = {
  mounted(el: TooltipTarget, binding) {
    // 去掉 UA 原生 tooltip（指令接管）
    el.removeAttribute("title");
    el.__ttText = binding.value;
    const onEnter = () => show(el, el.__ttText ?? "");
    const onLeave = () => hide();
    el.addEventListener("mouseenter", onEnter);
    el.addEventListener("mouseleave", onLeave);
    el.__ttCleanup = () => {
      el.removeEventListener("mouseenter", onEnter);
      el.removeEventListener("mouseleave", onLeave);
    };
  },
  updated(el: TooltipTarget, binding) {
    // 同步最新文本（hover 时读的就是它，见 __ttText 注释）
    el.__ttText = binding.value;
    // 值变化且正在显示 → 即时更新文本并重定位
    if (currentTarget === el && binding.value !== currentText) {
      const tip = ensureTip();
      tip.textContent = binding.value;
      currentText = binding.value;
      reposition();
    }
  },
  unmounted(el: TooltipTarget) {
    el.__ttCleanup?.();
    if (currentTarget === el) {
      tipEl?.classList.remove("visible");
      currentTarget = null;
    }
  },
};

export default tooltip;
