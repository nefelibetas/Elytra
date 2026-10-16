<script setup lang="ts">
// 指令生成器：顶部指令选择 + 按 schema（`utils/commands.ts`）动态渲染参数字段 + 输出命令。
// 新增指令 = 在 COMMANDS 里加一条定义，本组件零改动。
import { computed, reactive, ref } from "vue";
import { Copy } from "@lucide/vue";
import DropSelect from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import ItemSearchInput from "./ItemSearchInput.vue";
import { COMMANDS, defaultValues, type CommandDef, type CommandValues } from "../../utils/commands";
import { checkNbtFragment, parseItemStackFragment } from "../../utils/nbtText";
import { copyText } from "../../utils/clipboard";

const activeId = ref(COMMANDS[0].id);
const active = computed<CommandDef>(
  () => COMMANDS.find((c) => c.id === activeId.value) ?? COMMANDS[0],
);

/** 各指令的值状态独立保存：切换回来不丢已填内容。 */
const store = reactive<Record<string, CommandValues>>({});
function valuesFor(def: CommandDef): CommandValues {
  if (!store[def.id]) store[def.id] = defaultValues(def);
  return store[def.id];
}
const values = computed(() => valuesFor(active.value));

const output = computed(() => active.value.build(values.value));

/** NBT 文本域的括号配平预检（不阻断输入，仅提示）——只检查带内容的 textarea 字段。 */
const fieldErrors = computed<Record<string, string | null>>(() => {
  const out: Record<string, string | null> = {};
  for (const f of active.value.fields) {
    if (f.kind !== "textarea") continue;
    const raw = String(values.value[f.key] ?? "").trim();
    out[f.key] = raw ? checkNbtFragment(raw) : null;
  }
  return out;
});

/** NBT 文本域输入：粘贴「物品 NBT 生成」产出的物品栈片段时自动导入（/give：提取物品与数量，
 *  组件内文转 give 参数语法；legacy 标签保留）。解析不出物品 id 时按普通 NBT 原样保留。 */
function onNbtInput(fieldKey: string, raw: string) {
  values.value[fieldKey] = raw;
  if (active.value.id !== "give") return;
  const parsed = parseItemStackFragment(raw);
  if (!parsed?.item) return;
  values.value["item"] = parsed.item;
  if (parsed.count != null) values.value["count"] = parsed.count;
  values.value[fieldKey] = parsed.modernArgs ?? parsed.legacyTags ?? "";
}

async function copyOutput() {
  if (!output.value) return;
  await copyText(output.value, "命令已复制");
}

// 首次进入即初始化全部指令的默认值（切换时字段立即可用）
for (const def of COMMANDS) valuesFor(def);
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>指令生成器</h3>
      <div class="tool-card-sub">原版 Java 指令可视化生成（一期热门指令）；目标选择器与 NBT 支持手写扩展</div>
    </div>

    <div class="cmd-picker">
      <button
        v-for="c in COMMANDS"
        :key="c.id"
        class="cmd-pick-btn"
        :class="{ active: c.id === activeId }"
        @click="activeId = c.id"
      >{{ c.name }}</button>
    </div>
    <p class="cmd-desc">{{ active.desc }}</p>

    <div v-for="f in active.fields" :key="f.key" class="cmd-row" :class="{ 'cmd-row-top': f.kind === 'textarea' }">
      <span class="cmd-label">{{ f.label }}</span>
      <template v-if="f.kind === 'enum'">
        <DropSelect
          :model-value="String(values[f.key] ?? '')"
          :options="f.options"
          width="220px"
          @update:model-value="(v: string) => (values[f.key] = v)"
        />
      </template>
      <template v-else-if="f.kind === 'item'">
        <ItemSearchInput
          :model-value="String(values[f.key] ?? '')"
          :placeholder="f.placeholder"
          max-width="360px"
          @update:model-value="(v: string) => (values[f.key] = v)"
        />
      </template>
      <template v-else-if="f.kind === 'text'">
        <input
          :value="String(values[f.key] ?? '')"
          class="cmd-input"
          :placeholder="f.placeholder"
          spellcheck="false"
          autocomplete="off"
          @input="values[f.key] = ($event.target as HTMLInputElement).value"
        />
      </template>
      <template v-else-if="f.kind === 'num'">
        <input
          v-model.number="values[f.key]"
          type="number"
          class="cmd-num"
          :min="f.min"
          :max="f.max"
          autocomplete="off"
        />
      </template>
      <template v-else-if="f.kind === 'check'">
        <CheckToggle
          :model-value="Boolean(values[f.key])"
          :size="14"
          @update:model-value="(v: string | boolean) => (values[f.key] = v)"
        >{{ f.label }}</CheckToggle>
      </template>
      <template v-else-if="f.kind === 'textarea'">
        <textarea
          :value="String(values[f.key] ?? '')"
          class="cmd-textarea"
          :rows="f.rows ?? 2"
          :placeholder="f.placeholder"
          spellcheck="false"
          autocomplete="off"
          @input="onNbtInput(f.key, ($event.target as HTMLTextAreaElement).value)"
        ></textarea>
      </template>
    </div>
    <p v-for="(err, key) in fieldErrors" v-show="err" :key="'e-' + key" class="cmd-err">{{ err }}</p>

    <div class="cmd-output">
      <div class="cmd-output-head">
        <span>命令（{{ active.name }}）</span>
        <button class="tool-btn cmd-copy-btn" @click="copyOutput">
          <Copy :size="14" />
          复制
        </button>
      </div>
      <code class="gen-output gen-output-cmd">{{ output || "（填写参数后生成）" }}</code>
    </div>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>

<style scoped>
.cmd-picker {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
}

.cmd-pick-btn {
  padding: 4px 12px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12.5px;
  color: var(--text-secondary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.cmd-pick-btn.active {
  color: var(--text-primary);
  border-color: var(--color-brand);
  background-color: var(--surface-page);
}

.cmd-desc {
  margin: 0 0 12px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.cmd-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.cmd-row-top {
  align-items: flex-start;
}

.cmd-label {
  /* 固定宽：label 随字数变宽会让各行控件左缘参差 */
  flex: 0 0 64px;
  white-space: nowrap;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.cmd-input {
  flex: 1;
  min-width: 0;
  height: 30px;
  padding: 0 10px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.cmd-input:hover,
.cmd-input:focus {
  border-color: var(--color-brand);
}

.cmd-num {
  width: 96px;
  height: 30px;
  padding: 0 8px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.cmd-num:focus {
  border-color: var(--color-brand);
}

.cmd-textarea {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  resize: vertical;
  outline: none;
}

.cmd-textarea:focus {
  border-color: var(--color-brand);
}

.cmd-err {
  margin: -4px 0 8px;
  font-size: 11px;
  color: var(--color-danger);
}

/* ── 输出区 ── */
.cmd-output {
  margin-top: 4px;
  padding-top: 12px;
  border-top: 1px solid var(--border-default);
}

.cmd-output-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-tertiary);
  margin-bottom: 6px;
}

.cmd-copy-btn {
  font-size: 12px;
  padding: 3px 10px;
}
</style>
