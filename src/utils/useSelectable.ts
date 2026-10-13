// 列表多选共享 composable：全选仅作用于当前可见列表；切 tab/搜索后仅保留仍可见的选中。
import { ref, computed } from "vue";
import type { Ref } from "vue";

/** visibleKeys 为当前可见 item 的 key 列表。 */
export function useSelectable(visibleKeys: Ref<string[]>) {
  /** 选中 key 集合（需整体替换才能触发响应）。 */
  const selected = ref<string[]>([]);
  const selectedSet = computed(() => new Set(selected.value));

  const selecting = computed(() => selected.value.length > 0);

  const allVisibleSelected = computed(
    () => visibleKeys.value.length > 0 && visibleKeys.value.every(k => selectedSet.value.has(k)),
  );

  function toggle(key: string) {
    selected.value = selectedSet.value.has(key)
      ? selected.value.filter(k => k !== key)
      : [...selected.value, key];
  }

  /** 已全选时 = 取消全选。 */
  function toggleAll() {
    selected.value = allVisibleSelected.value ? [] : [...visibleKeys.value];
  }

  function clear() {
    selected.value = [];
  }

  return { selected, selectedSet, selecting, allVisibleSelected, toggle, toggleAll, clear };
}
