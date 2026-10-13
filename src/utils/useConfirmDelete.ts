// 删除确认弹窗共享 composable：confirmDelete 先关窗再执行回调（耗时删除不阻塞关窗）。
import { ref } from "vue";

/** onConfirm 参数为 openDelete 记录的目标名。 */
export function useConfirmDelete(onConfirm: (target: string) => Promise<void> | void) {
  const deleteDialog = ref(false);
  /** 待删除目标名（模板显示用）。 */
  const deleteTarget = ref("");

  function openDelete(target: string) {
    deleteTarget.value = target;
    deleteDialog.value = true;
  }

  async function confirmDelete() {
    const target = deleteTarget.value;
    deleteDialog.value = false;
    await onConfirm(target);
  }

  return { deleteDialog, deleteTarget, openDelete, confirmDelete };
}
