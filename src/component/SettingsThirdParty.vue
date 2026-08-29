<script setup lang="ts">
// 设置-启动设置-第三方登录：常用验证服务器（皮肤站）预设管理。
// 启动页第三方登录表单的"预设"下拉从这里读。
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2 } from "@lucide/vue";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";

const servers = ref<string[]>([]);
const newServer = ref("");

async function load() {
  try {
    const cfg = await invoke<{ servers: string[] }>("get_third_party_config");
    servers.value = cfg.servers ?? [];
  } catch {
    servers.value = [];
  }
}

async function save() {
  try {
    await invoke("save_third_party_config", { config: { servers: servers.value } });
    showToast("已保存");
  } catch (e) {
    showToast(errMsg(e));
  }
}

function addServer() {
  const v = newServer.value.trim();
  if (!v) return;
  if (servers.value.includes(v)) {
    showToast("该地址已存在");
    return;
  }
  servers.value.push(v);
  newServer.value = "";
  void save();
}

function removeServer(i: number) {
  servers.value.splice(i, 1);
  void save();
}

onMounted(load);
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>第三方登录</h3>
      <div class="stg-card-sub">常用验证服务器（皮肤站）预设，任意 authlib-injector 兼容地址</div>
    </div>
    <div class="tp-list">
      <div v-for="(sv, i) in servers" :key="sv" class="tp-row">
        <span class="tp-url" :title="sv">{{ sv }}</span>
        <button class="tp-del" title="移除" @click="removeServer(i)">
          <Trash2 :size="13" />
        </button>
      </div>
      <div v-if="servers.length === 0" class="tp-empty">暂无预设，可添加常用皮肤站地址</div>
      <div class="tp-row tp-add">
        <input
          v-model="newServer"
          class="tp-input"
          type="text"
          placeholder="如 https://littleskin.cn/api/yggdrasil"
          @keyup.enter="addServer"
        />
        <button class="tp-add-btn" @click="addServer">
          <Plus :size="13" />
          添加
        </button>
      </div>
    </div>
    <p class="tp-note">
      提示：第三方登录（外置认证）使用 authlib-injector 技术，支持任意实现了
      Yggdrasil 协议的皮肤站（LittleSkin、Blessing Skin 等）。登录后游戏内显示该站皮肤。
    </p>
  </div>
</template>

<style scoped>
.tp-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.tp-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid var(--border-default);
  border-radius: 6px;
  background: var(--surface-elevated);
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
.tp-input {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--surface-page);
  border: none;
  outline: none;
}
.tp-add-btn {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  font-size: 12px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.tp-add-btn:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.tp-note {
  margin: 12px 0 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--text-tertiary);
}
</style>
