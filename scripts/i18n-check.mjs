// i18n 门禁：key 对称 / 占位符一致 / 后端 key 全覆盖 / 前端静态 key 存在 / 裸中文扫描。
// 用法：node scripts/i18n-check.mjs [--strict]（--strict 时裸中文与未 keyed 的中文错误首行也判失败）
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const strict = process.argv.includes("--strict");
const locales = ["zh-CN", "en-US"];
const messagesRoot = path.join(root, "src/i18n/messages");
const rustRoot = path.join(root, "src-tauri", "src");

function loadLocale(locale) {
  const dir = path.join(messagesRoot, locale);
  const out = {};
  for (const file of fs.readdirSync(dir)) {
    if (!file.endsWith(".ts") || file === "index.ts") continue;
    const source = fs.readFileSync(path.join(dir, file), "utf8");
    const js = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2020,
      },
    }).outputText;
    const mod = { exports: {} };
    new Function("exports", "module", js)(mod.exports, mod);
    out[file.replace(/\.ts$/, "")] = mod.exports.default ?? {};
  }
  return out;
}

function flatten(value, prefix = "", out = {}) {
  for (const [key, child] of Object.entries(value)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (child && typeof child === "object" && !Array.isArray(child)) {
      flatten(child, full, out);
    } else {
      out[full] = child;
    }
  }
  return out;
}

function placeholders(value) {
  return [...new Set([...String(value).matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort();
}

/** 递归遍历目录（按扩展名过滤；回调拿到相对路径与文件内容）。 */
function walk(dir, ext, cb) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, ext, cb);
    else if (entry.name.endsWith(ext)) {
      cb(path.relative(root, full).replaceAll("\\", "/"), fs.readFileSync(full, "utf8"));
    }
  }
}

/** `?a=1&b={x}` → ["a","b"]（i18n 侧占位符必须与之一致）。 */
function queryParams(query) {
  return [...(query ?? "").matchAll(/(?:^|[?&])(\w+)=/g)].map((m) => m[1]).sort();
}

const tables = Object.fromEntries(locales.map((l) => [l, flatten(loadLocale(l))]));
const [source, ...targets] = locales;
let failed = false;

for (const locale of targets) {
  const missing = Object.keys(tables[source]).filter((k) => !(k in tables[locale]));
  const extra = Object.keys(tables[locale]).filter((k) => !(k in tables[source]));
  if (missing.length || extra.length) {
    failed = true;
    console.error(`[i18n] ${locale} key mismatch`);
    if (missing.length) console.error("  missing:", missing.join(", "));
    if (extra.length) console.error("  extra:", extra.join(", "));
  }
  for (const key of Object.keys(tables[source])) {
    if (!(key in tables[locale])) continue;
    const a = placeholders(tables[source][key]);
    const b = placeholders(tables[locale][key]);
    if (a.join(",") !== b.join(",")) {
      failed = true;
      console.error(`[i18n] placeholder mismatch ${locale} ${key}: ${a} vs ${b}`);
    }
  }
}

// ── 后端 key：四种入口（@i18n: 字面量 / common_err_ctx_key! / bail_key! / keyed_msg!）
//    逐处核对双语表存在性，并校验 `?k=` 参数与文案占位符一致（否则插值静默丢失或露出 `{k}`）。
function checkBackendKeys() {
  const entries = [];
  const macroRe = /(?:bail_key|common_err_ctx_key|keyed_msg)!\s*\(\s*"([^"]*)"/g;
  const literalRe = /@i18n:([A-Za-z0-9_.-]+)(\?[^\s"'`\\]*)?/g;
  walk(rustRoot, ".rs", (rel, text) => {
    text.split(/\r?\n/).forEach((line, i) => {
      for (const m of line.matchAll(macroRe)) {
        const [key, query] = m[1].split("?");
        entries.push({ where: `${rel}:${i + 1}`, key, params: queryParams(query) });
      }
      for (const m of line.matchAll(literalRe)) {
        entries.push({ where: `${rel}:${i + 1}`, key: m[1], params: queryParams(m[2]) });
      }
    });
  });

  const problems = [];
  const seen = new Set();
  for (const { where, key, params } of entries) {
    const tag = `${where}|${key}|${params.join(",")}`;
    if (seen.has(tag)) continue;
    seen.add(tag);
    for (const locale of locales) {
      const text = tables[locale][key];
      if (text === undefined) {
        problems.push(`缺少 ${locale} key: ${key}  (${where})`);
        continue;
      }
      const need = placeholders(text);
      if (need.join(",") !== params.join(",")) {
        problems.push(
          `占位符不符 ${locale} ${key}: 文案 [${need}] / 调用处 [${params}]  (${where})`,
        );
      }
    }
  }
  if (problems.length) {
    failed = true;
    console.error("[i18n] backend key problems:");
    for (const p of problems) console.error(`  ${p}`);
  }
}

checkBackendKeys();

// ── 前端：静态 key 必须存在（动态拼接的 key 不在此列，typod key 会在运行时直接显示原字符串）。
function checkFrontendKeys() {
  const missing = [];
  const callRe = /\b(?:t|translate)\(\s*"([^"`]+)"/g;
  const walkSrc = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (full.includes(path.join("i18n", "messages"))) continue;
        walkSrc(full);
        continue;
      }
      if (!/\.(ts|vue)$/.test(entry.name)) continue;
      const rel = path.relative(root, full).replaceAll("\\", "/");
      fs.readFileSync(full, "utf8")
        .split(/\r?\n/)
        .forEach((line, i) => {
          if (line.trimStart().startsWith("//") || line.trimStart().startsWith("*")) return;
          for (const m of line.matchAll(callRe)) {
            if (!(m[1] in tables["zh-CN"]) || !(m[1] in tables["en-US"])) {
              missing.push(`${m[1]}  (${rel}:${i + 1})`);
            }
          }
        });
    }
  };
  walkSrc(path.join(root, "src"));
  if (missing.length) {
    failed = true;
    console.error("[i18n] frontend keys missing:");
    for (const m of missing) console.error(`  ${m}`);
  }
}

checkFrontendKeys();

// ── 后端未 keyed 的中文错误首行：存量锁进基线（scripts/i18n-rust-plain-baseline.json），
//    新增即失败、修掉提示剪枝（--update-baseline 重写基线），避免新错误绕过 i18n。
const plainBaselineFile = path.join(root, "scripts", "i18n-rust-plain-baseline.json");

function collectRustPlainErrors() {
  const hits = [];
  const constructRe = /(?:Err\(\s*(?:format!\(\s*)?"|(?:^|[^_])bail!\s*\(\s*"|ok_or(?:_else\(\s*\|\|\s*(?:format!\(\s*)?"|")?|common_err_ctx!\s*\(\s*")/;
  walk(rustRoot, ".rs", (rel, text) => {
    if (rel.endsWith("macros.rs")) return;
    // 测试与实现同文件、按惯例在尾部：截掉 test 区，避免断言消息误报
    const testAt = text.indexOf("#[cfg(test)]");
    const body = testAt >= 0 ? text.slice(0, testAt) : text;
    body.split(/\r?\n/).forEach((line, i) => {
      const trimmed = line.trim();
      if (trimmed.startsWith("//") || /^(?:debug_)?assert|println!|eprintln!|log::/.test(trimmed)) {
        return;
      }
      if (/keyed_msg!|bail_key!|common_err_ctx_key!/.test(line)) return;
      const at = constructRe.exec(line);
      if (!at) return;
      const msg = /"([^"]*[\u4e00-\u9fff][^"]*)"/.exec(line.slice(at.index));
      if (!msg) return;
      hits.push({ rel, line: i + 1, msg: msg[1].split("\\n")[0] });
    });
  });
  return hits;
}

function checkRustPlainErrors() {
  const hits = collectRustPlainErrors();
  const key = (h) => `${h.rel}|${h.msg}`;
  const current = [...new Set(hits.map(key))].sort();
  if (process.argv.includes("--update-baseline")) {
    fs.writeFileSync(plainBaselineFile, `${JSON.stringify(current, null, 2)}\n`);
    console.warn(`[i18n] baseline updated: ${current.length} entries`);
    return;
  }
  const baseline = fs.existsSync(plainBaselineFile)
    ? JSON.parse(fs.readFileSync(plainBaselineFile, "utf8"))
    : [];
  const baseSet = new Set(baseline);
  const currentSet = new Set(current);
  const added = hits.filter((h) => !baseSet.has(key(h)));
  const fixed = baseline.filter((k) => !currentSet.has(k));
  if (added.length) {
    failed = true;
    console.error(`[i18n] new unlocalized Chinese error first lines: ${added.length}`);
    for (const h of added.slice(0, 20)) console.error(`  ${h.rel}:${h.line}  ${h.msg.slice(0, 60)}`);
    if (added.length > 20) console.error(`  ... ${added.length - 20} more`);
  }
  if (fixed.length) {
    console.warn(
      `[i18n] ${fixed.length} baseline entries are now localized — prune with --update-baseline`,
    );
  }
  const remaining = current.length - added.length;
  if (remaining > 0) {
    console.warn(
      `[i18n] legacy unlocalized Chinese error first lines: ${remaining} (baseline; fix incrementally)`,
    );
  }
}

checkRustPlainErrors();

const excludedDirs = ["src/i18n/messages/zh-CN", "src/types", "node_modules", "dist"];
const excludedFiles = new Set([
  "src/utils/give.ts",
  "src/utils/mcData.ts",
  "src/utils/commands.ts",
  "src/utils/nbt.ts",
  "src/utils/motd.ts",
  "src/utils/blockNames.ts",
  "src/store/skin/skinEditor.ts",
]);
const hits = [];

function stripComments(text) {
  return text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

function scan(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    const rel = path.relative(root, full).replaceAll("\\", "/");
    if (entry.isDirectory()) {
      if (excludedDirs.some((d) => rel === d || rel.startsWith(`${d}/`))) continue;
      scan(full);
      continue;
    }
    if (!/\.(vue|ts)$/.test(entry.name) || excludedFiles.has(rel) || rel.endsWith("vite-env.d.ts")) {
      continue;
    }
    const rawLines = fs.readFileSync(full, "utf8").split(/\r?\n/);
    const lines = stripComments(rawLines.join("\n")).split(/\r?\n/);
    lines.forEach((line, i) => {
      if (
        /[\u4e00-\u9fff]/.test(line) &&
        !line.includes("console.") &&
        !rawLines[i]?.includes("i18n-ignore")
      ) {
        hits.push(`${rel}:${i + 1}`);
      }
    });
  }
}

scan(path.join(root, "src"));

if (hits.length) {
  console.warn(`[i18n] bare Chinese in ${hits.length} lines (${strict ? "error" : "warning"})`);
  for (const hit of hits.slice(0, 30)) console.warn(`  ${hit}`);
  if (hits.length > 30) console.warn(`  ... ${hits.length - 30} more`);
  if (strict) failed = true;
}

if (failed) process.exit(1);
console.log("[i18n] ok");
