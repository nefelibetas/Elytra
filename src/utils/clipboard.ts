import { t } from "../i18n";
import { showToast } from "./toast";

/** okMsg 为成功提示；失败统一提示手动复制。 */
export async function copyText(text: string, okMsg = t("common.action.copied")) {
  try {
    await navigator.clipboard.writeText(text);
    showToast(okMsg);
  } catch {
    showToast(t("common.action.copyFailedManual"));
  }
}
