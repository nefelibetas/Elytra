import { t, translate } from "../i18n";

const I18N_PREFIX = "@i18n:";

export interface FormattedError {
  /** 翻译后的短摘要（toast / 弹窗正文） */
  summary: string;
  /** 技术细节（不翻译） */
  detail: string;
  /** 摘要 + 细节，供复制 */
  full: string;
}

/** 兼容旧约定 "短文案\n技术细节" 与后端 @i18n 协议；未识别的错误原样展示。 */
export function formatError(e: unknown): FormattedError {
  const raw = String(e ?? "");
  const [first = "", ...rest] = raw.split("\n");
  const line = first.trim();

  if (line.startsWith(I18N_PREFIX)) {
    const spec = line.slice(I18N_PREFIX.length);
    const queryAt = spec.indexOf("?");
    const key = queryAt >= 0 ? spec.slice(0, queryAt) : spec;
    const query = queryAt >= 0 ? spec.slice(queryAt + 1) : "";
    const params = Object.fromEntries(new URLSearchParams(query));
    const fallback = rest[0]?.trim() || key;
    const detail = rest.slice(1).join("\n").trim();
    const summary = translate(key, params) ?? fallback;
    return { summary, detail, full: detail ? `${summary}\n${detail}` : summary };
  }

  const summary = line || t("errors.unknown");
  const detail = rest.join("\n").trim();
  return { summary, detail, full: detail ? `${summary}\n${detail}` : summary };
}

/** 取首行（已翻译）作为短提示。 */
export function errMsg(e: unknown): string {
  return formatError(e).summary;
}
