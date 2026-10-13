import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

/** 全局内存状态（MB），由 startMemoryMonitor 刷新。 */
export const memState = reactive({
  totalMb: 0,
  availMb: 0,
});

let timer: number | undefined;

/** 启动内存监听：立即刷新一次，之后每 intervalMs 一次。幂等。 */
export function startMemoryMonitor(intervalMs = 2000) {
  if (timer !== undefined) return;
  const tick = () => {
    invoke<[number, number]>("get_memory_info")
      .then(([total, avail]) => {
        memState.totalMb = total;
        memState.availMb = avail;
      })
      .catch(() => {});
  };
  tick();
  timer = window.setInterval(tick, intervalMs);
}
