import { showToast } from "./toast";

/** okMsg 为成功提示；失败统一提示手动复制。 */
export async function copyText(text: string, okMsg = "已复制") {
  try {
    await navigator.clipboard.writeText(text);
    showToast(okMsg);
  } catch {
    showToast("复制失败，请手动复制");
  }
}
