// 应用顶层提示（App.vue 在右上角渲染，任意页面 import 调用）：同屏最多 5 条、超出排队，hover 暂停自动关闭，可手动关闭。
import { reactive } from "vue";

/** 默认驻留时长：需让人看清一句提示（过短会在读到前消失） */
const DEFAULT_MS = 4000;
/** 同屏条数上限：超出的排队，最早一条消失后按序补位 */
const MAX_VISIBLE = 5;
/** 恢复计时后的最短驻留：避免鼠标划过即消失 */
const MIN_RESUME_MS = 600;

export interface ToastItem {
  id: number;
  msg: string;
  durationMs: number;
}

/** 可见提示（按到达顺序自上而下排列） */
export const toasts = reactive<ToastItem[]>([]);

/** 待显示队列（同屏满员时进入） */
const pending: ToastItem[] = [];
/** 运行中的计时器与到期时刻 */
const live = new Map<number, { timer: number; deadline: number }>();
/** 暂停时的剩余时长 */
const remainings = new Map<number, number>();
let nextId = 0;

function arm(id: number, ms: number) {
  live.set(id, {
    timer: window.setTimeout(() => dismissToast(id), ms),
    deadline: Date.now() + ms,
  });
}

/** 停表并记下剩余时长（hover 暂停用） */
function hold(id: number) {
  const entry = live.get(id);
  if (!entry) return;
  clearTimeout(entry.timer);
  live.delete(id);
  remainings.set(id, Math.max(0, entry.deadline - Date.now()));
}

function forget(id: number) {
  const entry = live.get(id);
  if (entry) {
    clearTimeout(entry.timer);
    live.delete(id);
  }
  remainings.delete(id);
}

function pump() {
  while (toasts.length < MAX_VISIBLE && pending.length > 0) {
    const item = pending.shift()!;
    toasts.push(item);
    arm(item.id, item.durationMs);
  }
}

/** 弹出一条提示；同屏已满则排队。返回提示 id。 */
export function showToast(msg: string, durationMs = DEFAULT_MS): number {
  const item: ToastItem = { id: ++nextId, msg, durationMs: Math.max(MIN_RESUME_MS, durationMs) };
  if (toasts.length < MAX_VISIBLE) {
    toasts.push(item);
    arm(item.id, item.durationMs);
  } else {
    pending.push(item);
  }
  return item.id;
}

/** 关闭指定提示（手动关闭与到时自动关闭共用）。 */
export function dismissToast(id: number) {
  forget(id);
  const visible = toasts.findIndex((t) => t.id === id);
  if (visible >= 0) toasts.splice(visible, 1);
  const queued = pending.findIndex((t) => t.id === id);
  if (queued >= 0) pending.splice(queued, 1);
  pump();
}

/** 鼠标悬停：暂停自动关闭。 */
export function pauseToast(id: number) {
  hold(id);
}

/** 鼠标移出：按剩余时长继续计时。 */
export function resumeToast(id: number) {
  if (live.has(id)) return;
  const item = toasts.find((t) => t.id === id);
  if (!item) return;
  arm(id, Math.max(MIN_RESUME_MS, remainings.get(id) ?? item.durationMs));
}
