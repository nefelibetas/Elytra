<script setup lang="ts">
// 设置-第三方登录：登录（服务器下拉内置+预设+自定义，账号密码）+ 预设管理。
// 启动页仅显示登录状态，登录逻辑集中在此。
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, LogOut } from "@lucide/vue";
import DropSelect from "./DropSelect.vue";
import {
  externalAccount, loadExternalAccounts,
  loginExternal, removeExternalAccount,
} from "../store/auth";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";

/** 内置常用验证服务器（label 展示名 + value API 地址）。 */
const BUILTIN_SERVERS = [
  { label: "LittleSkin", value: "https://littleskin.cn/api/yggdrasil" },
] as const;

/** 用户预设服务器（设置里管理，value = API 地址；label 取域名短名）。 */
const presetServers = ref<string[]>([]);

/** 合并后的下拉选项：内置 + 预设。 */
const serverOptions = computed(() => {
  const options: { label: string; value: string }[] = BUILTIN_SERVERS.map(b => ({ label: b.label, value: b.value }));
  for (const url of presetServers.value) {
    if (BUILTIN_SERVERS.some(b => b.value === url)) continue;
    options.push({ label: shortLabel(url), value: url });
  }
  return options;
});

/** URL → 展示短名（域名或去掉协议的路径首段）。 */
function shortLabel(url: string): string {
  const cleaned = url.replace(/^https?:\/\//, "").replace(/\/+$/, "");
  const host = cleaned.split("/")[0];
  // 已知皮肤站常见域名直接展示域名主体
  return host || url;
}

const extApi = ref("");
const extUser = ref("");
const extPass = ref("");
const extBusy = ref(false);
const extError = ref("");
const newServer = ref("");

/** 登录：服务器（选中或自定义）+ 账号密码。 */
async function doLogin() {
  if (extBusy.value) return;
  if (!extApi.value.trim()) { extError.value = "请选择或输入验证服务器地址"; return; }
  if (!extUser.value.trim() || !extPass.value) { extError.value = "请输入账号与密码"; return; }
  extBusy.value = true;
  extError.value = "";
  try {
    await loginExternal(extApi.value.trim(), extUser.value.trim(), extPass.value);
    extUser.value = "";
    extPass.value = "";
    showToast("登录成功");
  } catch (e) {
    extError.value = String(e);
  } finally {
    extBusy.value = false;
  }
}

async function removeCurrent() {
  const acc = externalAccount.value;
  if (!acc) return;
  try {
    await removeExternalAccount(acc.api_url, acc.uuid);
    showToast("已移除");
  } catch (e) {
    showToast(errMsg(e));
  }
}

// ── 预设管理 ──
async function loadPresets() {
  try {
    const cfg = await invoke<{ servers: string[] }>("get_third_party_config");
    presetServers.value = cfg.servers ?? [];
  } catch {
    presetServers.value = [];
  }
}

async function savePresets() {
  try {
    await invoke("save_third_party_config", { config: { servers: presetServers.value } });
  } catch (e) {
    showToast(errMsg(e));
  }
}

function addPreset() {
  const v = newServer.value.trim();
  if (!v) return;
  if (presetServers.value.includes(v) || BUILTIN_SERVERS.some(b => b.value === v)) {
    showToast("该地址已存在");
    return;
  }
  presetServers.value.push(v);
  newServer.value = "";
  void savePresets();
}

function removePreset(i: number) {
  presetServers.value.splice(i, 1);
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

    <!-- 当前登录状态 -->
    <div v-if="externalAccount" class="tp-current">
      <span class="tp-cur-name">{{ externalAccount.name }}</span>
      <span class="tp-cur-server" :title="externalAccount.api_url">{{ externalAccount.api_url }}</span>
      <button class="tp-btn" @click="removeCurrent">
        <LogOut :size="13" />
        移除账号
      </button>
    </div>

    <!-- 登录表单 -->
    <div v-if="!externalAccount" class="tp-login">
      <div class="tp-row tp-field">
        <DropSelect
          :model-value="extApi"
          :options="serverOptions"
          placeholder="选择服务器"
          width="170px"
          @update:model-value="extApi = $event"
        />
        <input
          v-model="extApi"
          class="tp-input"
          type="text"
          placeholder="验证服务器地址（如 littleskin.cn/api/yggdrasil）"
        />
      </div>
      <div class="tp-row tp-field">
        <input v-model="extUser" class="tp-input" type="text" placeholder="皮肤站账号" />
        <input
          v-model="extPass"
          class="tp-input"
          type="password"
          placeholder="密码"
          @keyup.enter="doLogin"
        />
      </div>
      <div class="tp-row tp-field">
        <button class="tp-btn tp-primary" :disabled="extBusy" @click="doLogin">
          {{ extBusy ? "登录中…" : "登录" }}
        </button>
        <span v-if="extError" class="tp-error">{{ extError }}</span>
      </div>
    </div>
  </div>

  <!-- 预设服务器管理 -->
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>预设服务器</h3>
      <div class="stg-card-sub">常用皮肤站地址，登录下拉可直接选择</div>
    </div>
    <div class="tp-list">
      <div v-for="(sv, i) in presetServers" :key="sv" class="tp-row">
        <span class="tp-url" :title="sv">{{ sv }}</span>
        <button class="tp-del" title="移除" @click="removePreset(i)">
          <Trash2 :size="13" />
        </button>
      </div>
      <div v-if="presetServers.length === 0" class="tp-empty">
        无自定义预设（内置 LittleSkin；可添加其他皮肤站地址）
      </div>
      <div class="tp-row tp-add">
        <input
          v-model="newServer"
          class="tp-input"
          type="text"
          placeholder="如 https://littleskin.cn/api/yggdrasil"
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
.tp-login {
  display: flex;
  flex-direction: column;
  gap: 8px;
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
.tp-field {
  gap: 8px;
}
.tp-select {
  flex: none;
  max-width: 160px;
  height: 30px;
  font-size: 12px;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  outline: none;
  padding: 0 6px;
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
.tp-primary {
  color: var(--text-inverse);
  background: var(--color-brand);
  border-color: var(--color-brand);
}
.tp-primary:hover {
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
.tp-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.tp-url {
  flex: 1;
  font-size: 12px;
  color: var(--text-secondary);
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
.tp-add {
  border-style: dashed;
  border-color: var(--border-strong);
}
</style>
