<script setup lang="ts">
// 联机设置卡片：协调节点（单行添加/删除）+ 中继策略 + 广播名/昵称。
// 会话操作在启动页右栏联机条（OnlinePanel.vue）；store 为模块级单例，两处共享状态。
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Save, ExternalLink, Plus, X, SearchCheck } from "@lucide/vue";
import {
  onlinePublicNodes, onlineAdvertiseMotd, onlineNickname, onlineAllowRelay,
  loadOnlineConfig, saveOnlineConfig, saveAdvertiseMotd, saveNickname, checkOnlineNodes,
} from "../../store/config/online";
import type { NodeCheckResult } from "../../store/config/online";
import CheckToggle from "../common/CheckToggle.vue";
import AlertDialog from "../common/AlertDialog.vue";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

const { t } = useI18n();

/** 节点输入框（回车或「添加」入列）。 */
const nodeInput = ref("");
/** 节点检查结果（按当前列表顺序）。 */
const nodeChecks = ref<NodeCheckResult[]>([]);
const checkingNodes = ref(false);
/** URL → 检查结果，渲染在对应节点 item 内。 */
const checkByNode = computed<Record<string, NodeCheckResult>>(() =>
  Object.fromEntries(nodeChecks.value.map((r) => [r.node, r])),
);

/** 中继开关的受控显示值：开启前先弹确认，取消时回弹。 */
const relaySwitch = ref(false);
const relayConfirmOpen = ref(false);

onMounted(async () => {
  await loadOnlineConfig();
  relaySwitch.value = onlineAllowRelay.value;
});

watch(onlineAllowRelay, (v) => {
  relaySwitch.value = v;
});

const savingConfig = ref(false);
const savingMotd = ref(false);
const savingNickname = ref(false);

function addNode() {
  const value = nodeInput.value.trim();
  if (!value) return;
  if (onlinePublicNodes.value.includes(value)) {
    showToast(t("settings.online.nodeExists"));
    return;
  }
  onlinePublicNodes.value.push(value);
  nodeInput.value = "";
  nodeChecks.value = [];
}

function removeNode(index: number) {
  onlinePublicNodes.value.splice(index, 1);
  nodeChecks.value = [];
}

async function handleCheckNodes() {
  if (onlinePublicNodes.value.length === 0) return;
  checkingNodes.value = true;
  try {
    nodeChecks.value = await checkOnlineNodes([...onlinePublicNodes.value]);
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    checkingNodes.value = false;
  }
}

function nodeStatusLabel(result: NodeCheckResult) {
  switch (result.status) {
    case "ok":
      return result.latency_ms == null
        ? t("settings.online.nodeOk")
        : t("settings.online.nodeOkLatency", { ms: result.latency_ms });
    case "unreachable":
      return t("settings.online.nodeUnreachable");
    case "invalid":
      return t("settings.online.nodeInvalid");
    case "not_node":
      return t("settings.online.nodeNotNode");
    default:
      return t("settings.online.nodeUnchecked");
  }
}

async function saveRelay(value: boolean) {
  const previous = onlineAllowRelay.value;
  onlineAllowRelay.value = value;
  relaySwitch.value = value;
  try {
    await saveOnlineConfig();
  } catch (e) {
    onlineAllowRelay.value = previous;
    relaySwitch.value = previous;
    showToast(errMsg(e));
  }
}

async function onRelayToggle(value: boolean | string) {
  if (!value) {
    await saveRelay(false);
    return;
  }
  // 受控回弹：先让受控值追上用户点击后的 DOM，再复位为关闭（否则 prop 值未变，Vue 不会重写 checked）。
  relaySwitch.value = true;
  await nextTick();
  relaySwitch.value = false;
  relayConfirmOpen.value = true;
}

async function confirmRelay() {
  relayConfirmOpen.value = false;
  await saveRelay(true);
}

async function handleSaveConfig() {
  savingConfig.value = true;
  try {
    await saveOnlineConfig();
    showToast(t("settings.online.savedConfig"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingConfig.value = false;
  }
}

async function handleSaveMotd() {
  savingMotd.value = true;
  try {
    await saveAdvertiseMotd();
    showToast(t("settings.online.savedMotd"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingMotd.value = false;
  }
}

async function handleSaveNickname() {
  savingNickname.value = true;
  try {
    await saveNickname();
    showToast(t("settings.online.savedNickname"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingNickname.value = false;
  }
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.online.title") }}</h3>
      <div class="stg-card-sub">
        {{ t("settings.online.subtitle") }}
      </div>
    </div>

    <!-- 协调服务器（跨网联机） -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.coordTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.coordDesc", { example: t("settings.online.coordExample") }) }}</p>
      <a
        class="online-link"
        href="https://easytier.cn"
        target="_blank"
        rel="noopener"
      >
        {{ t("settings.online.coordGuide") }}
        <ExternalLink :size="12" />
      </a>

      <div class="online-node-add">
        <input
          v-model="nodeInput"
          class="online-input"
          :placeholder="t('settings.online.nodeInputPlaceholder')"
          autocomplete="off"
          @keyup.enter="addNode"
        />
        <button class="online-btn" :disabled="!nodeInput.trim()" @click="addNode">
          <Plus :size="14" />
          {{ t("settings.online.addNode") }}
        </button>
      </div>

      <div v-if="onlinePublicNodes.length" class="online-node-list">
        <div v-for="(node, index) in onlinePublicNodes" :key="node" class="online-node-item">
          <span class="online-node-url">{{ node }}</span>
          <span
            v-if="checkByNode[node]"
            class="online-node-check-status"
            :class="`is-${checkByNode[node].status}`"
          >
            {{ nodeStatusLabel(checkByNode[node]) }}
          </span>
          <button
            class="online-node-remove"
            :title="t('settings.online.removeNode')"
            :aria-label="t('settings.online.removeNode')"
            @click="removeNode(index)"
          >
            <X :size="14" />
          </button>
        </div>
      </div>
      <p v-else class="online-empty">{{ t("settings.online.noNodes") }}</p>

      <div class="online-actions">
        <button
          class="online-btn"
          :disabled="checkingNodes || onlinePublicNodes.length === 0"
          @click="handleCheckNodes"
        >
          <SearchCheck :size="14" />
          {{ checkingNodes ? t("settings.online.nodeChecking") : t("settings.online.nodeCheck") }}
        </button>
        <button class="online-btn" :disabled="savingConfig" @click="handleSaveConfig">
          <Save :size="14" />
          {{ savingConfig ? t("settings.online.saving") : t("common.action.save") }}
        </button>
      </div>

      <p class="online-hint">{{ t("settings.online.nodeHint") }}</p>
    </div>

    <!-- 中继策略 -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.relayTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.relayDesc") }}</p>
      <CheckToggle :model-value="relaySwitch" :size="15" @change="onRelayToggle">
        {{ t("settings.online.relayToggle") }}
      </CheckToggle>
    </div>

    <!-- 自定义联机广播名 -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.motdTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.motdDesc", { example: t("settings.online.motdExample") }) }}</p>
      <input
        v-model="onlineAdvertiseMotd"
        class="online-input"
        :placeholder="t('settings.online.motdPlaceholder')"
        autocomplete="off"
      />
      <div class="online-actions">
        <button class="online-btn" :disabled="savingMotd" @click="handleSaveMotd">
          <Save :size="14" />
          {{ savingMotd ? t("settings.online.saving") : t("common.action.save") }}
        </button>
      </div>
    </div>

    <!-- 联机昵称 -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.nicknameTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.nicknameDesc") }}</p>
      <input
        v-model="onlineNickname"
        class="online-input"
        :placeholder="t('settings.online.nicknamePlaceholder')"
        autocomplete="off"
      />
      <div class="online-actions">
        <button class="online-btn" :disabled="savingNickname" @click="handleSaveNickname">
          <Save :size="14" />
          {{ savingNickname ? t("settings.online.saving") : t("common.action.save") }}
        </button>
      </div>
    </div>

    <AlertDialog v-model="relayConfirmOpen" :title="t('settings.online.relayConfirmTitle')">
      <p class="online-desc online-confirm-text">{{ t("settings.online.relayConfirmMsg") }}</p>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmRelay">
          {{ t("settings.online.relayConfirmOk") }}
        </button>
        <button class="dialog-btn" @click="relayConfirmOpen = false">
          {{ t("common.action.cancel") }}
        </button>
      </template>
    </AlertDialog>
  </div>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
.online-sec {
  padding: 12px 0;
  border-top: 1px solid var(--border-default);
}
.online-sec:first-of-type {
  border-top: none;
  padding-top: 0;
}
.online-sec-title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 4px;
}
.online-desc {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.5;
  margin: 0 0 10px;
}
.online-link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--color-brand);
  text-decoration: none;
  margin-bottom: 8px;
}
.online-link:hover {
  text-decoration: underline;
}
.online-mono {
  font-family: "Cascadia Mono", "Consolas", monospace;
  color: var(--text-secondary);
}
.online-input {
  width: 100%;
  box-sizing: border-box;
  height: 32px;
  padding: 0 10px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}
.online-input:focus {
  border-color: var(--color-brand);
  outline: none;
}
.online-actions {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  padding-top: 8px;
}
.online-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.15s;
}
.online-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--text-primary);
}
.online-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 节点单行添加 + 列表 */
.online-node-add {
  display: flex;
  align-items: center;
  gap: 8px;
}
.online-node-add .online-input {
  flex: 1;
  min-width: 0;
}
.online-node-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}
.online-node-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px 6px 10px;
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}
.online-node-url {
  flex: 1;
  min-width: 0;
  font-family: "Cascadia Mono", "Consolas", monospace;
  font-size: 12px;
  color: var(--text-secondary);
  overflow-wrap: anywhere;
}
.online-node-remove {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  color: var(--text-tertiary);
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}
.online-node-remove:hover {
  color: var(--color-danger);
  background-color: var(--color-danger-bg);
}
.online-empty {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
}
.online-node-check-status {
  flex: none;
  font-size: 12px;
  white-space: nowrap;
}
.online-node-check-status.is-ok {
  color: var(--color-brand);
}
.online-node-check-status.is-unreachable,
.online-node-check-status.is-invalid,
.online-node-check-status.is-not_node {
  color: var(--color-danger);
}
.online-node-check-status.is-unchecked {
  color: var(--text-tertiary);
}
.online-hint {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.5;
}
.online-confirm-text {
  margin: 0;
}
</style>
