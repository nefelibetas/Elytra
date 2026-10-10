// 版本选择状态单例（Launch / VersionSelect 共用）。
// 收敛原先两处手写的 localStorage 键（elytra.selectedVersion / selectedVersionDir）契约。
import { ref } from "vue";

const SELECTED_VERSION_KEY = "elytra.selectedVersion";
const SELECTED_DIR_KEY = "elytra.selectedVersionDir";

export const selectedVersion = ref(localStorage.getItem(SELECTED_VERSION_KEY) ?? "");

export function setSelected(v: string, dir?: string) {
  selectedVersion.value = v;
  localStorage.setItem(SELECTED_VERSION_KEY, v);
  if (dir !== undefined) localStorage.setItem(SELECTED_DIR_KEY, dir);
}

export function clearSelected() {
  localStorage.removeItem(SELECTED_VERSION_KEY);
  localStorage.removeItem(SELECTED_DIR_KEY);
  selectedVersion.value = "";
}

/** 上次选择所属目录（跨目录校验用）。 */
export function getSavedDir(): string | null {
  return localStorage.getItem(SELECTED_DIR_KEY);
}
