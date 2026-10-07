<script setup lang="ts">
// 服务器状态检测：输入地址（Java/基岩版，可自动识别），探测在线状态。
// 后端实现：commands/server_status.rs → server_status.rs（SLP / RakNet）。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Radio, RefreshCw } from "@lucide/vue";
import MotdPreview from "./MotdPreview.vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";

/** 服务器状态（镜像 Rust ServerStatus）。 */
interface ServerStatus {
  edition: string;
  version_name: string;
  protocol: number | null;
  online_players: number;
  max_players: number;
  motd: string;
  player_sample: string[];
  favicon: string | null;
}

type Edition = "" | "java" | "bedrock";

const address = ref("");
const edition = ref<Edition>("");
/** 协议选择选项（DropSelect 用）。 */
const editionOptions: SelOption[] = [
  { value: "", label: "自动识别" },
  { value: "java", label: "Java 版" },
  { value: "bedrock", label: "基岩版" },
];
const loading = ref(false);
const status = ref<ServerStatus | null>(null);
const error = ref("");
const elapsedMs = ref(0);

/** 探测（回车/按钮均触发）。 */
async function handlePing() {
  const addr = address.value.trim();
  if (!addr) {
    error.value = "请输入服务器地址";
    status.value = null;
    return;
  }
  loading.value = true;
  error.value = "";
  const start = performance.now();
  try {
    status.value = await invoke<ServerStatus>("ping_server", {
      address: addr,
      edition: edition.value || null,
    });
  } catch (e) {
    status.value = null;
    error.value = String(e);
  } finally {
    elapsedMs.value = Math.round(performance.now() - start);
    loading.value = false;
  }
}

/** 结果头部提示：在线（绿点）/ 无响应（红点）。 */
function statusLine(): string {
  if (status.value?.edition === "java") return "Java 版 · 在线";
  if (status.value?.edition === "bedrock") return "基岩版 · 在线";
  return "无响应";
}

function editionLabel(): string {
  if (status.value?.edition === "java") return "Java 版";
  if (status.value?.edition === "bedrock") return "基岩版";
  return "未知";
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>服务器状态检测</h3>
      <div class="tool-card-sub">探测任意 Java / 基岩版服务器的在线状态、版本、人数与 MOTD</div>
    </div>

    <!-- 输入行 -->
    <div class="ping-row">
      <input
        v-model="address"
        class="ping-input"
        placeholder="服务器地址[:端口]，如 play.example.com 或 1.2.3.4:25565"
        :disabled="loading"
        @keyup.enter="handlePing"
      />
      <DropSelect
        :model-value="edition"
        :options="editionOptions"
        width="108px"
        :disabled="loading"
        @update:model-value="(v: string) => (edition = v as Edition)"
      />
      <button class="tool-btn ping-btn" :disabled="loading || !address.trim()" @click="handlePing">
        <RefreshCw :size="14" :class="{ spin: loading }" />
        {{ loading ? "检测中…" : "检测" }}
      </button>
    </div>

    <!-- 错误 -->
    <div v-if="error" class="ping-error">{{ error }}</div>

    <!-- 结果 -->
    <div v-if="status" class="ping-result">
      <div class="ping-head">
        <img v-if="status.favicon" class="ping-icon" :src="status.favicon" alt="服务器图标" />
        <div class="ping-meta">
          <div class="ping-line1">
            <span class="ping-dot" :class="{ online: status.edition === 'java' || status.edition === 'bedrock' }" />
            <span class="ping-status">{{ statusLine() }}</span>
            <span class="ping-edition">{{ editionLabel() }}</span>
            <span class="ping-elapsed">{{ elapsedMs }} ms</span>
          </div>
          <div class="ping-line2">
            <span class="ping-ver">{{ status.version_name }}</span>
            <span v-if="status.protocol" class="ping-protocol">协议 {{ status.protocol }}</span>
          </div>
          <div class="ping-players">
            <span :class="{ full: status.online_players >= status.max_players && status.max_players > 0 }">
              {{ status.online_players }} / {{ status.max_players }}
            </span>
            人在线
          </div>
        </div>
      </div>

      <div class="ping-motd-label">MOTD</div>
      <MotdPreview :motd="status.motd" />

      <!-- 玩家样例 -->
      <div v-if="status.player_sample.length > 0" class="ping-sample">
        <span class="ping-sample-label">在线玩家</span>
        <div class="ping-sample-list">
          <span v-for="(name, i) in status.player_sample" :key="i" class="ping-sample-item">{{ name }}</span>
          <span v-if="status.online_players > status.player_sample.length" class="ping-sample-more">
            等 {{ status.online_players }} 人
          </span>
        </div>
      </div>
    </div>

    <!-- 提示 -->
    <div v-else class="ping-hint">
      <Radio :size="13" />
      联机时可用它确认主机游戏端口是否已开放；地址不填端口时默认 Java 25565 / 基岩版 19132。
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

/* ===== 输入行 ===== */
.ping-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.ping-input {
  flex: 1;
  min-width: 0;
  padding: 8px 10px;
  font-size: 13px;
  color: var(--text-primary);
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.ping-input:hover,
.ping-input:focus {
  border-color: var(--color-brand);
}

.ping-btn {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.spin {
  animation: ping-spin 0.8s linear infinite;
}

@keyframes ping-spin {
  to { transform: rotate(360deg); }
}

/* ===== 错误 ===== */
.ping-error {
  margin-top: 12px;
  padding: 10px 12px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--color-danger);
  background: var(--color-danger-bg);
  background: color-mix(in srgb, var(--color-danger) 8%, transparent);
  border: 1px solid var(--color-danger);
  border: 1px solid color-mix(in srgb, var(--color-danger) 25%, transparent);
  border-radius: var(--radius-sm);
  white-space: pre-wrap;
  word-break: break-all;
}

/* ===== 结果 ===== */
.ping-result {
  margin-top: 14px;
  padding: 12px 14px;
  background: var(--surface-page);
  border-radius: var(--radius-sm);
}

.ping-head {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.ping-icon {
  width: 48px;
  height: 48px;
  border-radius: 4px;
  image-rendering: pixelated;
  flex: 0 0 auto;
}

.ping-meta {
  min-width: 0;
}

.ping-line1 {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.ping-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-tertiary);
  flex: 0 0 auto;
}

.ping-dot.online {
  background: #55ff55;
  box-shadow: 0 0 6px rgba(85, 255, 85, 0.6);
}

.ping-status {
  font-weight: 600;
  color: var(--text-primary);
}

.ping-edition {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

.ping-elapsed {
  font-size: 11px;
  color: var(--text-tertiary);
}

.ping-line2 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}

.ping-protocol {
  font-size: 11px;
  color: var(--text-tertiary);
}

.ping-players {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}

.ping-players span {
  font-weight: 600;
  color: var(--text-primary);
}

.ping-players span.full {
  color: #ff5555;
}

.ping-motd-label {
  margin: 12px 0 6px;
  font-size: 11px;
  color: var(--text-tertiary);
}

/* ===== 玩家样例 ===== */
.ping-sample {
  margin-top: 12px;
}

.ping-sample-label {
  display: block;
  font-size: 11px;
  color: var(--text-tertiary);
  margin-bottom: 6px;
}

.ping-sample-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.ping-sample-item {
  padding: 2px 8px;
  font-size: 12px;
  border-radius: 999px;
  background: var(--surface-hover);
  color: var(--text-secondary);
}

.ping-sample-more {
  padding: 2px 8px;
  font-size: 12px;
  color: var(--text-tertiary);
}

/* ===== 提示 ===== */
.ping-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 12px;
  font-size: 12px;
  color: var(--text-tertiary);
}
</style>
