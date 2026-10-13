import { ref } from "vue";
import { errMsg } from "./error";

export function useConfigSection() {
  const error = ref("");

  async function run(fn: () => Promise<void>) {
    error.value = "";
    try {
      await fn();
    } catch (e) {
      error.value = errMsg(e);
    }
  }

  return { error, run };
}
