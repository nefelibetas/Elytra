// 剪贴板复制：统一 navigator.clipboard 调用与失败提示，避免各生成器逐个重复实现。
import { showToast } from "./toast";

/** 复制文本到剪贴板；okMsg 为成功提示，失败统一提示手动复制。 */
export async function copyText(text: string, okMsg = "已复制") {
  try {
    await navigator.clipboard.writeText(text);
    showToast(okMsg);
  } catch {
    showToast("复制失败，请手动复制");
  }
}
