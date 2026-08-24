// 删除确认弹窗三件套（deleteDialog / deleteTarget / confirmDelete）共享 composable。
// 各资源管理组件的删除确认交互完全一致：openDelete(target) 记录目标并弹出 AlertDialog，
// confirmDelete() 先关弹窗再执行删除回调（与既有「先关后删」行为一致，耗时删除不阻塞关窗）。
import { ref } from "vue";

/**
 * 创建删除确认弹窗状态。
 * @param onConfirm 确认删除回调，参数为 openDelete 记录的目标名（文件名/存档名等）。
 */
export function useConfirmDelete(onConfirm: (target: string) => Promise<void> | void) {
  /** 弹窗开关。 */
  const deleteDialog = ref(false);
  /** 待删除目标名（模板正文显示用）。 */
  const deleteTarget = ref("");

  /** 打开确认弹窗（记录本次删除目标）。 */
  function openDelete(target: string) {
    deleteTarget.value = target;
    deleteDialog.value = true;
  }

  /** 确认删除：先关弹窗，再执行回调。 */
  async function confirmDelete() {
    const target = deleteTarget.value;
    deleteDialog.value = false;
    await onConfirm(target);
  }

  return { deleteDialog, deleteTarget, openDelete, confirmDelete };
}
