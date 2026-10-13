// v-tooltip="文本"：主题化气泡替代 UA 原生 title；hover 400ms 显示、移出 100ms 隐藏、跟随鼠标（越界翻转）、空值不显示。
import type { Directive } from "vue";

interface TooltipTarget extends HTMLElement {
  __ttCleanup?: () => void;
  /** 最新提示文本：hover 时读它。闭包若直接捕获 `binding.value` 会停在首次绑定快照，故存可变最新值。 */
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
  if (!currentTarget.isConnected) {
    tipEl.classList.remove("visible");
    currentTarget = null;
    return;
  }
  const tw = tipEl.offsetWidth;
  const th = tipEl.offsetHeight;
  // 鼠标右下；越界翻转到左/上
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

/** 全局记录鼠标位置（capture）。 */
window.addEventListener(
  "mousemove",
  (e) => {
    mouseX = e.clientX;
    mouseY = e.clientY;
    if (tipEl?.classList.contains("visible")) reposition();
  },
  true,
);

/** scroll/resize 全局跟随（capture 覆盖所有滚动容器）。 */
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
    el.__ttText = binding.value;
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
