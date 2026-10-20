// i18n 门禁：key 对称 / 占位符一致 / 裸中文扫描。
// 用法：node scripts/i18n-check.mjs [--strict]（--strict 时裸中文也判失败）
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const strict = process.argv.includes("--strict");
const locales = ["zh-CN", "en-US"];
const messagesRoot = path.join(root, "src/i18n/messages");

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

// 后端 key：扫描 @i18n: 协议与 keyed 宏调用，确认 zh-CN 消息表存在。
function checkBackendKeys() {
  const dir = path.join(root, "src-tauri", "src");
  const missing = new Set();
  const keyRe = /(?:@i18n:|common_err_ctx_key!\(\s*"|bail_key!\(\s*")([a-z0-9_.-]+)/g;
  const walk = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (!entry.name.endsWith(".rs")) continue;
      const text = fs.readFileSync(full, "utf8");
      for (const match of text.matchAll(keyRe)) {
        if (!(match[1] in tables[source])) missing.add(match[1]);
      }
    }
  };
  walk(dir);
  if (missing.size) {
    failed = true;
    console.error("[i18n] backend keys missing:", [...missing].join(", "));
  }
}

checkBackendKeys();

const excludedDirs = ["src/i18n/messages/zh-CN", "src/types", "node_modules", "dist"];
const excludedFiles = new Set([
  "src/utils/give.ts",
  "src/utils/mcData.ts",
  "src/utils/commands.ts",
  "src/utils/nbt.ts",
  "src/utils/motd.ts",
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
