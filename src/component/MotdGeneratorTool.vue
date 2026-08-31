<script setup lang="ts">
// MOTD 生成器：编辑含 § 格式码的 MOTD 文本（16 色/样式/1.16+ 渐变），实时预览，
// 复制原始文本或 server.properties 转义形式；可一键设为联机广播名
// （guest 端 MC 局域网标签页显示的名字，支持 § 颜色渲染）。
import { onMounted, ref } from "vue";
import {
  Bold, Italic, Underline, Strikethrough, Sparkles, Eraser, Copy, Save, Server,
} from "@lucide/vue";
import MotdPreview from "./MotdPreview.vue";
import {
  MC_COLORS, MC_COLOR_KEYS, MC_COLOR_NAMES, MC_STYLES,
  gradientMotd, escapeServerProperties, visibleLength, hexToSectionX,
} from "../utils/motd";
import {
  loadOnlineConfig, saveAdvertiseMotd, onlineAdvertiseMotd,
} from "../store/online";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";

/** 当前编辑的 MOTD 文本（含 § 码）。 */
const text = ref("§6Elytra §f联机服务器");
/** 渐变起止色（hex）。 */
const gradFrom = ref("#FFAA00");
const gradTo = ref("#FF5555");
const taRef = ref<HTMLTextAreaElement | null>(null);
const saving = ref(false);

onMounted(() => {
  void loadOnlineConfig();
});

/** 在光标处（或选区前）插入文本（格式码或完整 §x 序列）。 */
function insertText(s: string) {
  const ta = taRef.value;
  if (!ta) return;
  const start = ta.selectionStart;
  const end = ta.selectionEnd;
  const prefix = s;
  if (start !== end) {
    // 有选区：格式码作用于选区（插在选区起点）
    text.value = text.value.slice(0, start) + prefix + text.value.slice(start);
    // 选区已被格式码"覆盖"（作用于其后字符），光标保持原选区起点后
    requestAnimationFrame(() => {
      ta.focus();
      ta.selectionStart = ta.selectionEnd = start + prefix.length;
    });
  } else {
    text.value = text.value.slice(0, start) + prefix + text.value.slice(start);
    requestAnimationFrame(() => {
      ta.focus();
      ta.selectionStart = ta.selectionEnd = start + prefix.length;
    });
  }
}

/** 在光标处插入格式码（§c）。 */
function insertCode(code: string) {
  insertText(`§${code}`);
}

/** 自定义颜色（color picker）→ 插入 §x 序列（1.16+）。 */
function onCustomColor(e: Event) {
  const v = (e.target as HTMLInputElement).value;
  const sx = hexToSectionX(v);
  if (sx) insertText(sx);
}

/** 清除全部格式码（保留纯文本）。 */
function stripCodes() {
  text.value = text.value.replace(/§[0-9a-fA-FkKlLmMnNoOrRxX](?:§[0-9a-fA-F]){6}/g, "").replace(/§[0-9a-fA-FkKlLmMnNoOrRxX]/g, "");
}

/** 应用渐变（整体文本，空白字符不染色）。 */
function applyGradient() {
  text.value = gradientMotd(text.value, gradFrom.value, gradTo.value);
}

async function copyRaw() {
  try {
    await navigator.clipboard.writeText(text.value);
    showToast("已复制（§ 原始格式）");
  } catch {
    showToast("复制失败，请手动复制");
  }
}

async function copyEscaped() {
  try {
    await navigator.clipboard.writeText(escapeServerProperties(text.value));
    showToast("已复制（server.properties 转义格式）");
  } catch {
    showToast("复制失败，请手动复制");
  }
}

/** 设为联机广播名（guest 端局域网标签页显示名）。 */
async function handleSaveAsAdvertise() {
  saving.value = true;
  try {
    onlineAdvertiseMotd.value = text.value;
    await saveAdvertiseMotd();
    showToast("已设为联机广播名");
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    saving.value = false;
  }
}

const charCount = () => visibleLength(text.value);
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>MOTD 生成器</h3>
      <div class="tool-card-sub">编辑服务器欢迎语 / 联机广播名，支持 16 色、样式与 1.16+ 渐变（§x 代码）</div>
    </div>

    <!-- 编辑区 -->
    <textarea
      ref="taRef"
      v-model="text"
      class="motd-input"
      rows="3"
      placeholder="输入 MOTD 文本，用下方按钮插入格式代码…"
      spellcheck="false"
    ></textarea>

    <!-- 工具栏 -->
    <div class="motd-toolbar">
      <div class="motd-toolbar-row">
        <span class="motd-tool-label">颜色</span>
        <button
          v-for="key in MC_COLOR_KEYS"
          :key="key"
          class="motd-color"
          :style="{ backgroundColor: MC_COLORS[key] }"
          :title="`§${key} ${MC_COLOR_NAMES[key]}`"
          @click="insertCode(key)"
        />
        <input
          type="color"
          class="motd-color-custom"
          title="自定义颜色（§x，需 1.16+）"
          @input="onCustomColor"
        />
      </div>
      <div class="motd-toolbar-row">
        <span class="motd-tool-label">样式</span>
        <button class="motd-style" title="粗体 §l" @click="insertCode(MC_STYLES.bold)">
          <Bold :size="14" />
        </button>
        <button class="motd-style" title="斜体 §o" @click="insertCode(MC_STYLES.italic)">
          <Italic :size="14" />
        </button>
        <button class="motd-style" title="下划线 §n" @click="insertCode(MC_STYLES.underline)">
          <Underline :size="14" />
        </button>
        <button class="motd-style" title="删除线 §m" @click="insertCode(MC_STYLES.strikethrough)">
          <Strikethrough :size="14" />
        </button>
        <button class="motd-style" title="乱码 §k（游戏内随机字符）" @click="insertCode(MC_STYLES.obfuscated)">
          <Sparkles :size="14" />
        </button>
        <button class="motd-style" title="重置格式 §r" @click="insertCode(MC_STYLES.reset)">
          <Eraser :size="14" />
        </button>
        <button class="tool-btn motd-clear-btn" @click="stripCodes">清除格式</button>
      </div>
      <div class="motd-toolbar-row">
        <span class="motd-tool-label">渐变</span>
        <input v-model="gradFrom" type="color" class="motd-grad" title="渐变起色" />
        <span class="motd-grad-arrow">→</span>
        <input v-model="gradTo" type="color" class="motd-grad" title="渐变止色" />
        <button class="tool-btn motd-grad-btn" @click="applyGradient">应用渐变</button>
      </div>
    </div>

    <!-- 预览 -->
    <div class="motd-preview-label">
      <span>预览</span>
      <span class="motd-count">{{ charCount() }} 字符</span>
    </div>
    <MotdPreview :motd="text" />

    <!-- 输出 -->
    <div class="motd-output">
      <button class="tool-btn" @click="copyRaw">
        <Copy :size="14" />
        复制（§ 原始格式）
      </button>
      <button class="tool-btn" @click="copyEscaped">
        <Copy :size="14" />
        复制（server.properties 转义）
      </button>
    </div>

    <!-- 联机广播名 -->
    <div class="motd-online">
      <Server :size="14" class="motd-online-icon" />
      <span class="motd-online-text">
        当前联机广播名：
        <template v-if="onlineAdvertiseMotd">自定义（在联机设置中可清除恢复自动）</template>
        <template v-else>自动（host 世界名）</template>
      </span>
      <button
        class="tool-btn motd-online-btn"
        :disabled="saving || !text.trim()"
        @click="handleSaveAsAdvertise"
      >
        <Save :size="14" />
        {{ saving ? "保存中…" : "设为联机广播名" }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.tool-card {
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 16px 18px;
}

.tool-card-hdr {
  margin-bottom: 14px;
}

.tool-card-hdr h3 {
  margin: 0 0 4px;
  font-size: 15px;
  color: var(--text-primary);
}

.tool-card-sub {
  font-size: 12px;
  color: var(--text-tertiary);
}

/* ===== 编辑区 ===== */
.motd-input {
  width: 100%;
  box-sizing: border-box;
  padding: 10px 12px;
  font-family: "Cascadia Mono", Consolas, "Courier New", monospace;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-primary);
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
  resize: none;
  min-height: 72px;
}

.motd-input:hover,
.motd-input:focus {
  border-color: var(--color-brand);
}

/* ===== 工具栏 ===== */
.motd-toolbar {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 10px 0 12px;
}

.motd-toolbar-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
}

.motd-tool-label {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-right: 2px;
  flex: 0 0 auto;
}

.motd-color {
  width: 18px;
  height: 18px;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  padding: 0;
  cursor: pointer;
  transition: transform 0.1s;
}

.motd-color:hover {
  transform: scale(1.15);
}

.motd-color-custom {
  width: 20px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: none;
  cursor: pointer;
}

.motd-style {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 24px;
  padding: 0;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-page);
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}

.motd-style:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

.motd-grad {
  width: 24px;
  height: 24px;
  padding: 0;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-page);
  cursor: pointer;
}

.motd-grad-arrow {
  font-size: 12px;
  color: var(--text-tertiary);
}

.motd-grad-btn,
.motd-clear-btn {
  font-size: 12px;
  padding: 4px 10px;
}

.motd-clear-btn {
  margin-left: auto;
}

/* ===== 预览 ===== */
.motd-preview-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-tertiary);
  margin-bottom: 6px;
}

.motd-count {
  font-variant-numeric: tabular-nums;
}

/* ===== 输出 ===== */
.motd-output {
  display: flex;
  gap: 8px;
  margin-top: 12px;
  flex-wrap: wrap;
}

/* ===== 联机广播名 ===== */
.motd-online {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border-default);
  flex-wrap: wrap;
}

.motd-online-icon {
  color: var(--color-brand);
  flex: 0 0 auto;
}

.motd-online-text {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
}

.motd-online-btn {
  font-size: 12px;
  padding: 4px 10px;
}
</style>
