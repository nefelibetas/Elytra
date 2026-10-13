// 版本选择状态单例（Launch / VersionSelect 共用），对应两个 localStorage 键。
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

export function getSavedDir(): string | null {
  return localStorage.getItem(SELECTED_DIR_KEY);
}
