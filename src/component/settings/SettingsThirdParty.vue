<script setup lang="ts">
// 设置-第三方登录：登录（服务器下拉选择）+ 预设管理（label + value）。
// 下拉选项 = 内置预设 + 用户预设（下面有什么，上面才能选什么）。
import { computed, onMounted, ref } from "vue";
import { loadThirdPartyConfig, saveThirdPartyConfig } from "../../store/thirdParty";
import { Plus, Trash2, ExternalLink } from "@lucide/vue";
import DropSelect from "../common/DropSelect.vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  externalAccount, externalAccounts, loadExternalAccounts,
  loginExternal, removeExternalAccount, switchExternalAccount,
} from "../../store/auth";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

interface ServerOption {
  label: string;
  value: string;
}

/** 预设服务器列表（内置 + 用户新增；get_third_party_config 默认返回内置）。 */
const presetServers = ref<ServerOption[]>([]);

/** 登录下拉选项 = 全部预设。 */
const serverOptions = computed(() => presetServers.value);

const extApi = ref("");
const extUser = ref("");
const extPass = ref("");
const extBusy = ref(false);
const extError = ref("");
/** 已登录时是否展开"登录新账号"表单 */
const showLoginForm = ref(false);

/** 新增预设输入（label + value）。 */
const newLabel = ref("");
const newValue = ref("");

/** 登录：选中服务器 + 账号密码。 */
async function doLogin() {
  if (extBusy.value) return;
  if (!extApi.value.trim()) { extError.value = "请选择验证服务器"; return; }
  if (!extUser.value.trim() || !extPass.value) { extError.value = "请输入账号与密码"; return; }
  extBusy.value = true;
  extError.value = "";
  try {
    await loginExternal(extApi.value.trim(), extUser.value.trim(), extPass.value);
    extUser.value = "";
    extPass.value = "";
    showLoginForm.value = false;
    showToast("登录成功");
  } catch (e) {
    extError.value = String(e);
  } finally {
    extBusy.value = false;
  }
}

/** 打开服务器站点（官网）：取 value 域名拼 https://。 */
function openServerLink(url: string) {
  const host = url.replace(/^https?:\/\//, "").split("/")[0];
  if (host) void openUrl(`https://${host}`);
}

// ── 预设管理 ──
async function loadPresets() {
  try {
    const cfg = await loadThirdPartyConfig();
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
    await saveThirdPartyConfig({ servers: presetServers.value });
  } catch (e) {
    showToast(errMsg(e));
  }
}

function addPreset() {
  const label = newLabel.value.trim();
  const value = newValue.value.trim();
  if (!label || !value) {
    showToast("请填写名称与地址");
    return;
  }
  if (presetServers.value.some(s => s.value === value)) {
    showToast("该地址已存在");
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
      <h3>第三方登录</h3>
      <div class="stg-card-sub">外置认证（authlib-injector），登录后游戏内显示该站皮肤</div>
    </div>

    <!-- 全部已存账号（点击切换启用） -->
    <div v-if="externalAccounts.length" class="tp-accounts">
      <div
        v-for="acc in externalAccounts"
        :key="acc.api_url + acc.uuid"
        class="tp-acct"
        :class="{ current: acc.uuid === externalAccount?.uuid && acc.api_url === externalAccount?.api_url }"
        @click="switchExternalAccount(acc.api_url, acc.uuid)"
      >
        <span class="tp-acct-check">{{ acc.uuid === externalAccount?.uuid && acc.api_url === externalAccount?.api_url ? "✓" : "" }}</span>
        <span class="tp-acct-name">{{ acc.site_username || "未知账号" }}</span>
        <span class="tp-acct-server" :title="acc.api_url">{{ acc.server_name || acc.api_url }}</span>
        <button
          class="tp-del"
          title="移除该账号"
          @click.stop="removeExternalAccount(acc.api_url, acc.uuid)"
        >×</button>
      </div>
    </div>

    <!-- 登录新账号：分隔线中间插文本（可点击开关）；未登录时表单直接显示 -->
    <div v-if="externalAccount" class="tp-divider" @click="showLoginForm = !showLoginForm">
      <span class="tp-divider-line"></span>
      <span class="tp-divider-text">{{ showLoginForm ? "收起" : "登录新账号" }}</span>
      <span class="tp-divider-line"></span>
    </div>

    <!-- 登录表单（展开动画） -->
    <Transition name="expand">
    <div v-if="!externalAccount || showLoginForm" class="tp-login">

      <!-- 行1：服务器选择 + 打开站点 -->
      <div class="tp-row tp-field">
        <span class="tp-label">服务器</span>
        <DropSelect
          :model-value="extApi"
          :options="serverOptions"
          placeholder="选择服务器"
          width="220px"
          @update:model-value="extApi = $event"
        />
        <button
          class="tp-btn tp-open"
          :disabled="!extApi.trim()"
          title="打开该皮肤站官网"
          @click="openServerLink(extApi)"
        >
          <ExternalLink :size="13" />
        </button>
      </div>
      <!-- 行2：账号 + 密码 + 登录 -->
      <div class="tp-row tp-field">
        <input v-model="extUser" class="tp-input" type="text" placeholder="皮肤站账号" />
        <input
          v-model="extPass"
          class="tp-input"
          type="password"
          placeholder="密码"
          @keyup.enter="doLogin"
        />
        <button class="tp-btn tp-primary tp-login-btn" :disabled="extBusy" @click="doLogin">
          {{ extBusy ? "登录中…" : "登录" }}
        </button>
      </div>
      <!-- 行3：错误提示（出现时） -->
      <div v-if="extError" class="tp-error-row">{{ extError }}</div>
    </div>
    </Transition>
  </div>

  <!-- 预设服务器管理 -->
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>预设服务器</h3>
      <div class="stg-card-sub">登录下拉选项来自此处（含内置）；新增填名称 + 地址</div>
      <div class="tp-compat">
        <div>兼容说明：内置 LittleSkin 与自建皮肤站为标准 Yggdrasil 格式（如 https://xxx/api/yggdrasil）</div>
        <div><b>Ely.by 为内置特殊适配</b>（非标准路径，自动处理）</div>
        <div>自定义添加请使用标准格式地址</div>
      </div>
    </div>
    <div class="tp-list">
      <div v-for="(sv, i) in presetServers" :key="sv.value" class="tp-row">
        <span class="tp-name" :title="sv.value">{{ sv.label }}</span>
        <span class="tp-url" :title="sv.value">{{ sv.value }}</span>
        <button class="tp-del" title="打开站点" @click="openServerLink(sv.value)">
          <ExternalLink :size="13" />
        </button>
        <button class="tp-del" title="移除" @click="removePreset(i)">
          <Trash2 :size="13" />
        </button>
      </div>
      <div v-if="presetServers.length === 0" class="tp-empty">暂无预设</div>
      <div class="tp-add-row">
        <input
          v-model="newLabel"
          class="tp-input tp-add-label"
          type="text"
          placeholder="名称（如 LittleSkin）"
          @keyup.enter="addPreset"
        />
        <input
          v-model="newValue"
          class="tp-input"
          type="text"
          placeholder="地址（如 https://littleskin.cn/api/yggdrasil）"
          @keyup.enter="addPreset"
        />
        <button class="tp-btn" @click="addPreset">
          <Plus :size="13" />
          添加
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
}
/* 展开/收起动画（max-height 写在 enter-to/leave-from，勿移 active） */
.expand-enter-active,
.expand-leave-active {
  transition: max-height 0.22s ease, opacity 0.18s ease;
}
.expand-enter-from,
.expand-leave-to {
  max-height: 0;
  opacity: 0;
  overflow: hidden;
}
.expand-enter-to,
.expand-leave-from {
  max-height: 1000px;
  overflow: hidden;
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
