// 配置区读写样板：error ref + run（统一 try/catch 与错误格式化）。
// Settings.vue 四个配置区（Java / 启动 / 离线皮肤 / 下载）共用。
import { ref } from "vue";

export function useConfigSection() {
  const error = ref("");

  async function run(fn: () => Promise<void>) {
    error.value = "";
    try {
      await fn();
    } catch (e) {
      error.value = String(e);
    }
  }

  return { error, run };
}
