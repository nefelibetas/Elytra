<script setup lang="ts">
// 设置-第三方登录：登录（服务器下拉）+ 预设管理（label + value）；下拉选项 = 内置预设 + 用户预设。
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { loadExternalConfig, saveExternalConfig } from "../../store/config/externalConfig";
import { Plus, Trash2, ExternalLink } from "@lucide/vue";
import DropSelect from "../common/DropSelect.vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  externalAccount, externalAccounts, loadExternalAccounts,
  loginExternal, removeExternalAccount, switchExternalAccount,
} from "../../store/auth/auth";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

interface ServerOption {
  label: string;
  value: string;
}

const { t } = useI18n();

const presetServers = ref<ServerOption[]>([]);

const serverOptions = computed(() => presetServers.value);

const extApi = ref("");
const extUser = ref("");
const extPass = ref("");
const extBusy = ref(false);
const extError = ref("");
/** 已登录时是否展开"登录新账号"表单 */
const showLoginForm = ref(false);

const newLabel = ref("");
const newValue = ref("");

async function doLogin() {
  if (extBusy.value) return;
  if (!extApi.value.trim()) { extError.value = t("settings.external.errorServer"); return; }
  if (!extUser.value.trim() || !extPass.value) { extError.value = t("settings.external.errorCredentials"); return; }
  extBusy.value = true;
  extError.value = "";
  try {
    await loginExternal(extApi.value.trim(), extUser.value.trim(), extPass.value);
    extUser.value = "";
    extPass.value = "";
    showLoginForm.value = false;
    showToast(t("settings.external.loginSuccess"));
  } catch (e) {
    extError.value = errMsg(e);
  } finally {
    extBusy.value = false;
  }
}

/** 打开服务器站点：取域名拼 https://。 */
function openServerLink(url: string) {
  const host = url.replace(/^https?:\/\//, "").split("/")[0];
  if (host) void openUrl(`https://${host}`);
}

// ── 预设管理 ──
async function loadPresets() {
  try {
    const cfg = await loadExternalConfig();
    presetServers.value = cfg.servers ?? [];
    if (!extApi.value && presetServers.value.length) {
      extApi.value = presetServers.value[0].value;
    }
  } catch {
    presetServers.value = [];
  }
}

async function savePresets() {
  try {
    await saveExternalConfig({ servers: presetServers.value });
  } catch (e) {
    showToast(errMsg(e));
  }
}

function addPreset() {
  const label = newLabel.value.trim();
  const value = newValue.value.trim();
  if (!label || !value) {
    showToast(t("settings.external.nameAndUrlRequired"));
    return;
  }
  if (presetServers.value.some(s => s.value === value)) {
    showToast(t("settings.external.urlExists"));
    return;
  }
  presetServers.value.push({ label, value });
  newLabel.value = "";
  newValue.value = "";
  void savePresets();
}

function removePreset(i: number) {
  presetServers.value.splice(i, 1);
  // 当前选中的服务器被删 → 回退第一个
  if (extApi.value && !presetServers.value.some(s => s.value === extApi.value)) {
    extApi.value = presetServers.value[0]?.value ?? "";
  }
  void savePresets();
}

onMounted(() => {
  void loadExternalAccounts();
  void loadPresets();
});
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.external.title") }}</h3>
      <div class="stg-card-sub">{{ t("settings.external.subtitle") }}</div>
    </div>

    <div v-if="externalAccounts.length" class="tp-accounts">
      <div
        v-for="acc in externalAccounts"
        :key="acc.api_url + acc.uuid"
        class="tp-acct"
        :class="{ current: acc.uuid === externalAccount?.uuid && acc.api_url === externalAccount?.api_url }"
        @click="switchExternalAccount(acc.api_url, acc.uuid)"
      >
        <span class="tp-acct-check">{{ acc.uuid === externalAccount?.uuid && acc.api_url === externalAccount?.api_url ? "✓" : "" }}</span>
        <span class="tp-acct-name">{{ acc.site_username || t("settings.external.unknownAccount") }}</span>
        <span class="tp-acct-server" :title="acc.api_url">{{ acc.server_name || acc.api_url }}</span>
        <button
          class="tp-del"
          :title="t('settings.external.removeAccount')"
          @click.stop="removeExternalAccount(acc.api_url, acc.uuid)"
        >×</button>
      </div>
    </div>

    <!-- 登录新账号：分隔线中插文本可点击；未登录时表单直接显示 -->
    <div v-if="externalAccount" class="tp-divider" @click="showLoginForm = !showLoginForm">
      <span class="tp-divider-line"></span>
      <span class="tp-divider-text">{{ showLoginForm ? t("settings.external.collapse") : t("settings.external.loginNew") }}</span>
      <span class="tp-divider-line"></span>
    </div>

    <Transition name="expand">
    <div v-if="!externalAccount || showLoginForm" class="tp-login">

      <div class="tp-row tp-field">
        <span class="tp-label">{{ t("settings.external.server") }}</span>
        <DropSelect
          :model-value="extApi"
          :options="serverOptions"
          :placeholder="t('settings.external.selectServer')"
          width="220px"
          @update:model-value="extApi = $event"
        />
        <button
          class="tp-btn tp-open"
          :disabled="!extApi.trim()"
          :title="t('settings.external.openSite')"
          @click="openServerLink(extApi)"
        >
          <ExternalLink :size="13" />
        </button>
      </div>
      <div class="tp-row tp-field">
        <input v-model="extUser" class="tp-input" type="text" :placeholder="t('settings.external.accountPlaceholder')" autocomplete="off" />
        <input
          v-model="extPass"
          class="tp-input"
          type="password"
          :placeholder="t('settings.external.password')"
          @keyup.enter="doLogin"
          autocomplete="new-password"
        />
        <button class="tp-btn tp-primary tp-login-btn" :disabled="extBusy" @click="doLogin">
          {{ extBusy ? t("settings.external.loggingIn") : t("settings.external.login") }}
        </button>
      </div>
      <div v-if="extError" class="tp-error-row">{{ extError }}</div>
    </div>
    </Transition>
  </div>

  <!-- 预设服务器管理 -->
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.external.presetTitle") }}</h3>
      <div class="stg-card-sub">{{ t("settings.external.presetSubtitle") }}</div>
      <div class="tp-compat">
        <div>{{ t("settings.external.compatYggdrasil") }}</div>
        <div>{{ t("settings.external.compatElyby") }}</div>
        <div>{{ t("settings.external.compatCustom") }}</div>
      </div>
    </div>
    <div class="tp-list">
      <div v-for="(sv, i) in presetServers" :key="sv.value" class="tp-row">
        <span class="tp-name" :title="sv.value">{{ sv.label }}</span>
        <span class="tp-url" :title="sv.value">{{ sv.value }}</span>
        <button class="tp-del" :title="t('settings.external.openPresetSite')" @click="openServerLink(sv.value)">
          <ExternalLink :size="13" />
        </button>
        <button class="tp-del" :title="t('settings.external.remove')" @click="removePreset(i)">
          <Trash2 :size="13" />
        </button>
      </div>
      <div v-if="presetServers.length === 0" class="tp-empty">{{ t("settings.external.emptyPresets") }}</div>
      <div class="tp-add-row">
        <input
          v-model="newLabel"
          class="tp-input tp-add-label"
          type="text"
          :placeholder="t('settings.external.labelPlaceholder')"
          @keyup.enter="addPreset"
          autocomplete="off"
        />
        <input
          v-model="newValue"
          class="tp-input"
          type="text"
          :placeholder="t('settings.external.urlPlaceholder')"
          @keyup.enter="addPreset"
          autocomplete="off"
        />
        <button class="tp-btn" @click="addPreset">
          <Plus :size="13" />
          {{ t("settings.external.add") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tp-divider {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 16px 0 6px;
  cursor: pointer;
  user-select: none;
}
.tp-divider-line {
  flex: 1;
  height: 1px;
  background: var(--border-default);
}
.tp-divider-text {
  flex: none;
  font-size: 12px;
  color: var(--text-tertiary);
  transition: color 0.12s;
}
.tp-divider:hover .tp-divider-text {
  color: var(--color-brand);
}

.tp-login {
  display: flex;
  flex-direction: column;
  gap: 8px;
  /* expand 目标高度（共享骨架见 styles/transitions.css） */
  --expand-h: 1000px;
}

.tp-current {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: 1px solid var(--border-default);
  border-radius: 8px;
  background: var(--surface-elevated);
}
.tp-cur-badge {
  flex: none;
  padding: 2px 8px;
  font-size: 11px;
  border-radius: 10px;
  background: var(--color-brand);
  color: var(--text-inverse);
}
.tp-accounts {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 8px;
}
.tp-acct {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid var(--border-default);
  border-radius: 6px;
  cursor: pointer;
  transition: border-color 0.12s;
}
.tp-acct:hover {
  border-color: var(--color-brand);
}
.tp-acct.current {
  border-color: var(--color-brand);
  background: var(--color-brand-bg);
}
.tp-acct-check {
  flex: none;
  width: 14px;
  color: var(--color-brand);
  font-weight: 700;
}
.tp-acct-name {
  flex: none;
  min-width: 0;
  max-width: 150px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tp-acct-server {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tp-cur-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.tp-cur-server {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tp-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.tp-label {
  flex: none;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.tp-input {
  flex: 1;
  min-width: 0;
  height: 30px;
  padding: 0 10px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  outline: none;
}
.tp-input:focus {
  border-color: var(--color-brand);
}
.tp-open {
  flex: none;
  padding: 5px 9px;
}
.tp-login-btn {
  flex: none;
}
.tp-error-row {
  font-size: 11.5px;
  color: var(--color-danger);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tp-btn {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 5px 12px;
  font-size: 12px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.tp-btn:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.tp-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.tp-primary {
  color: var(--text-inverse);
  background: var(--color-brand);
  border-color: var(--color-brand);
}
.tp-primary:hover {
  color: var(--text-inverse);
  opacity: 0.9;
}
.tp-error {
  flex: 1;
  font-size: 11.5px;
  color: var(--color-danger);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tp-compat {
  margin: 0 0 10px;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--text-tertiary);
}

.tp-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.tp-name {
  flex: none;
  min-width: 0;
  max-width: 160px;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tp-url {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
}
.tp-del {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
}
.tp-del:hover {
  color: var(--color-danger);
  background: var(--color-danger-bg);
}
.tp-empty {
  padding: 10px;
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
}
.tp-add-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 1px dashed var(--border-strong);
  border-radius: 6px;
}
.tp-add-label {
  flex: none;
  max-width: 140px;
}
</style>
