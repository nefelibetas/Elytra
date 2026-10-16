// 键盘焦点遍历：App.vue 的 Tab/Shift+Tab 转发目标。浏览器原生遍历会绕过页面语义直接改焦点，
// 故顺序与焦点环都由应用自管——文档序（TitleBar → 页面内容）；有弹窗时范围收窄到该弹窗（等效焦点陷阱）。

/** 参与遍历的控件；`data-tabskip`（自身或祖先）整棵子树排除 */
const CANDIDATE_SELECTOR = "button, a[href], input, select, textarea, [tabindex]";

/** 焦点环状态标记类（无配套 CSS 规则，供 DevTools/测试识别当前焦点环落在谁身上） */
const RING_CLASS = "kb-focus";

let ringed: HTMLElement | null = null;

function isFocusable(el: HTMLElement): boolean {
	if (el.hasAttribute("data-tabskip") || el.closest("[data-tabskip]")) return false;
	if (el.hasAttribute("disabled")) return false;
	const tabindex = el.getAttribute("tabindex");
	if (tabindex !== null && Number(tabindex) < 0) return false;
	if (el.getClientRects().length === 0) return false;
	return getComputedStyle(el).visibility !== "hidden";
}

/** 当前焦点范围：最上层打开的弹窗优先，否则整个应用外壳 */
function scopeRoot(): HTMLElement {
	const dialogs = document.querySelectorAll<HTMLElement>(".dialog-overlay");
	return dialogs.length > 0 ? dialogs[dialogs.length - 1] : document.body;
}

/** 当前范围内可遍历的控件（文档序） */
export function focusables(): HTMLElement[] {
	const root = scopeRoot();
	return Array.from(root.querySelectorAll<HTMLElement>(CANDIDATE_SELECTOR)).filter(isFocusable);
}

/** 清掉键盘焦点环（鼠标交互或失焦时） */
export function clearFocusRing() {
	if (!ringed) return;
	ringed.classList.remove(RING_CLASS);
	ringed.style.removeProperty("outline");
	ringed.style.removeProperty("outline-offset");
	ringed = null;
}

/** 打上焦点环：环走内联 `!important`——组件里的 scoped `outline: none` 特异性高于任何全局单类规则，
 *  样式表写法会被盖掉；内描边（负 offset）不产生元素外的异色细边。 */
function mark(el: HTMLElement) {
	clearFocusRing();
	ringed = el;
	el.classList.add(RING_CLASS);
	// 标题栏铺品牌渐变，品牌色环在其上不可辨：标题栏内改用前景色
	const color = el.closest(".titlebar") ? "var(--titlebar-fg)" : "var(--color-brand)";
	el.style.setProperty("outline", `2px solid ${color}`, "important");
	el.style.setProperty("outline-offset", "-2px", "important");
	el.addEventListener("blur", clearFocusRing, { once: true });
}

/** 按文档序移动焦点（step = 1 下一个 / -1 上一个，两端环绕）；无可遍历控件返回 false */
export function moveFocus(step: number): boolean {
	const items = focusables();
	if (items.length === 0) return false;
	const active = document.activeElement as HTMLElement | null;
	const from = active ? items.indexOf(active) : -1;
	const index = from < 0
		? (step > 0 ? 0 : items.length - 1)
		: (from + step + items.length) % items.length;
	const target = items[index];
	target.focus({ preventScroll: true });
	target.scrollIntoView({ block: "nearest", inline: "nearest" });
	mark(target);
	return true;
}

/** 弹窗打开：焦点移入首个可遍历控件（无则聚焦卡片自身）；返回原焦点元素供关闭时归还 */
export function focusDialog(card: HTMLElement): HTMLElement | null {
	const prev = document.activeElement as HTMLElement | null;
	const inside = Array.from(card.querySelectorAll<HTMLElement>(CANDIDATE_SELECTOR)).filter(isFocusable);
	(inside[0] ?? card).focus({ preventScroll: true });
	return prev && prev !== card && prev.isConnected && prev !== document.body ? prev : null;
}

/** 弹窗关闭：把焦点还给打开它的元素（元素已卸载则不动） */
export function restoreFocus(el: HTMLElement | null) {
	if (el && el.isConnected) el.focus({ preventScroll: true });
}
