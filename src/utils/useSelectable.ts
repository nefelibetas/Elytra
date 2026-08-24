// 管理页列表多选状态（选中/全选/取消）共享 composable。
// 版本设置三个管理页（Mod/资源/存档）统一交互：点击 item 切换选中，底部 action 栏
// 对选中项批量操作；全选作用于当前可见列表（items），切 tab/搜索后仅保留仍可见的选中。
import { ref, computed } from "vue";
import type { Ref } from "vue";

/**
 * 创建多选状态。
 * @param visibleKeys 当前可见 item 的 key 列表（全选作用于这些 key）。
 */
export function useSelectable(visibleKeys: Ref<string[]>) {
  /** 选中 key 集合（ref<Set> 需整体替换触发响应）。 */
  const selected = ref<string[]>([]);
  const selectedSet = computed(() => new Set(selected.value));

  /** 是否处于多选模式（至少选中一项）。 */
  const selecting = computed(() => selected.value.length > 0);

  /** 当前可见项是否全部选中。 */
  const allVisibleSelected = computed(
    () => visibleKeys.value.length > 0 && visibleKeys.value.every(k => selectedSet.value.has(k)),
  );

  /** 切换单项选中。 */
  function toggle(key: string) {
    selected.value = selectedSet.value.has(key)
      ? selected.value.filter(k => k !== key)
      : [...selected.value, key];
  }

  /** 全选/取消全选（作用于当前可见项；已全选时点全选 = 取消）。 */
  function toggleAll() {
    selected.value = allVisibleSelected.value ? [] : [...visibleKeys.value];
  }

  /** 清空选中（删除/操作完成后调用）。 */
  function clear() {
    selected.value = [];
  }

  return { selected, selectedSet, selecting, allVisibleSelected, toggle, toggleAll, clear };
}
