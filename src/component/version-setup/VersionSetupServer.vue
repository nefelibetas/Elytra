<script setup lang="ts">
// 版本设置-服务器：快速进服（--server/--port），per-version 存储，启动时优先于全局配置。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { loadVersionServer, saveVersionServer } from "../../store/task/versionSetup";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

const route = useRoute();
const versionName = computed(() => String(route.query.name ?? ""));

const server = ref("");
const port = ref("");
const saved = ref(false);

onMounted(async () => {
  try {
    const cfg = await loadVersionServer(versionName.value);
    server.value = cfg.server ?? "";
    port.value = cfg.port != null ? String(cfg.port) : "";
  } catch {
    // 无配置保持空值
  }
});

async function save() {
  try {
    await saveVersionServer(
      versionName.value,
      server.value.trim() || null,
      port.value.trim() ? Number(port.value.trim()) : null,
    );
    saved.value = true;
    setTimeout(() => (saved.value = false), 1500);
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vs-page">
    <div class="setup-card">
      <div class="setup-card-hdr">
        <div class="setup-card-titles">
          <div class="setup-card-title">服务器</div>
          <div class="setup-desc">启动时自动连接该服务器（--server/--port），仅对当前版本生效</div>
        </div>
      </div>
      <div class="vssrv">
        <div class="vssrv-row">
          <label class="vssrv-label" for="vssrv-server">服务器地址</label>
          <input
            id="vssrv-server"
            v-model="server"
            class="vssrv-input"
            type="text"
            placeholder="留空不直连（如 mc.example.com）"
            maxlength="128"
            @change="save"
            autocomplete="off"
          />
        </div>
        <div class="vssrv-row">
          <label class="vssrv-label" for="vssrv-port">端口</label>
          <input
            id="vssrv-port"
            v-model="port"
            class="vssrv-input"
            type="number"
            min="1"
            max="65535"
            placeholder="25565"
            @change="save"
            autocomplete="off"
          />
          <span v-if="saved" class="vssrv-saved">已保存</span>
        </div>
        <p class="vssrv-note">
          启动游戏时自动连接该服务器（--server/--port）。仅对当前版本生效；留空则按全局配置。
        </p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.vs-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}
.setup-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  max-width: 640px;
  padding: 14px 16px;
}
.setup-card-hdr {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
}
.setup-card-titles {
  min-width: 0;
}
.setup-card-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
}
.setup-desc {
  margin-top: 2px;
  font-size: 12px;
  color: var(--text-tertiary);
}
.vssrv {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.vssrv-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.vssrv-label {
  flex: none;
  width: 72px;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.vssrv-input {
  flex: 1;
  height: 32px;
  padding: 0 10px;
  font-size: 12.5px;
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  outline: none;
}
.vssrv-input:focus {
  border-color: var(--color-brand);
}
.vssrv-saved {
  flex: none;
  font-size: 12px;
  color: var(--color-brand);
}
.vssrv-note {
  margin: 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-tertiary);
}
</style>
