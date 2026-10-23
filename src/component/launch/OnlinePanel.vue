<script setup lang="ts">
// 联机面板：EasyTier 虚拟局域网（host 开房 / guest 加入 / 停止）；配置在「设置 → 联机设置」。
// host 可先开联机（等待态）再「对局域网开放」，检测到端口自动生成房间码；正版 host 由服务器验证会话。
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Copy, Link2, Unplug } from "@lucide/vue";
import {
  onlineStatus, onlineRoomCode, onlineLocalPort, onlinePlayers,
  onlineErrorHost, onlineErrorGuest,
  startHost, joinRoom, stopOnline, loadOnlineStatus, ensureOnlineListen,
} from "../../store/config/online";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { copyText } from "../../utils/clipboard";

const { t } = useI18n();

onMounted(() => {
  ensureOnlineListen();
  void loadOnlineStatus();
});

/** guest 输入的房间码。 */
const roomCodeInput = ref("");
const busy = ref(false);

async function handleStartHost() {
  busy.value = true;
  try {
    await startHost();
    showToast(t("tools.online.hostStarted"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    busy.value = false;
  }
}

async function handleJoin() {
  busy.value = true;
  try {
    await joinRoom(roomCodeInput.value.trim());
    showToast(t("tools.online.joined"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    busy.value = false;
  }
}

async function handleStop() {
  try {
    await stopOnline();
    showToast(t("tools.online.stopped"));
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function copyRoomCode() {
  await copyText(onlineRoomCode.value, t("tools.online.roomCodeCopied"));
}
</script>

<template>
  <div class="online-panel">
    <div class="online-status">
      <span class="online-dot" :class="{ active: onlineStatus !== 'idle', pulse: onlineStatus === 'waiting' }" />
      <span class="online-status-text">
        {{
          onlineStatus === "hosting" ? t("tools.online.statusHosting")
          : onlineStatus === "joining" ? t("tools.online.statusJoining")
          : onlineStatus === "waiting" ? t("tools.online.statusWaiting")
          : t("tools.online.statusIdle")
        }}
      </span>
    </div>

    <div class="online-sec">
      <div class="online-sec-title">{{ t("tools.online.hostSection") }}</div>
      <p class="online-desc">{{ t("tools.online.hostDesc1") }}</p>
      <p class="online-desc">{{ t("tools.online.hostDesc2") }}</p>

      <div v-if="onlineStatus === 'waiting'" class="online-waiting">
        {{ t("tools.online.waitingHint") }}
      </div>

      <div v-if="onlineStatus === 'hosting'" class="online-room">
        <span class="online-room-code">{{ onlineRoomCode }}</span>
      </div>

      <div v-if="onlineErrorHost" class="online-error">{{ errMsg(onlineErrorHost) }}</div>

      <div class="online-actions">
        <button
          v-if="onlineStatus === 'hosting'"
          class="online-btn"
          @click="copyRoomCode"
        >
          <Copy :size="14" />
          {{ t("tools.online.copyRoomCode") }}
        </button>
        <button
          v-if="onlineStatus === 'hosting' || onlineStatus === 'waiting'"
          class="online-btn online-btn-stop"
          @click="handleStop"
        >
          <Unplug :size="14" />
          {{ t("tools.online.stop") }}
        </button>
        <button
          v-else-if="onlineStatus === 'idle'"
          class="online-btn online-btn-primary"
          :disabled="busy"
          @click="handleStartHost"
        >
          <Link2 :size="14" />
          {{ busy ? t("tools.online.startingHost") : t("tools.online.startHost") }}
        </button>
      </div>
    </div>

    <div class="online-sec">
      <div class="online-sec-title">{{ t("tools.online.joinSection") }}</div>
      <p class="online-desc">{{ t("tools.online.joinDesc") }}</p>

      <div class="online-row">
        <span class="online-label">{{ t("tools.online.roomCode") }}</span>
        <input
          v-model="roomCodeInput"
          class="online-input"
          placeholder="U/XXXX-XXXX-XXXX-XXXX"
          :disabled="onlineStatus !== 'idle'"
          @keyup.enter="handleJoin"
          autocomplete="off"
        />
      </div>

      <div v-if="onlineStatus === 'joining' && onlineLocalPort" class="online-room">
        <span class="online-room-code">{{ t("tools.online.roomDisplayName") }}</span>
        <span class="online-hint">{{ t("tools.online.joiningHint", { port: onlineLocalPort }) }}</span>
      </div>

      <div v-if="onlineErrorGuest" class="online-error">{{ errMsg(onlineErrorGuest) }}</div>

      <div class="online-actions">
        <button
          v-if="onlineStatus === 'joining'"
          class="online-btn online-btn-stop"
          @click="handleStop"
        >
          <Unplug :size="14" />
          {{ t("tools.online.stop") }}
        </button>
        <button
          v-else-if="onlineStatus === 'idle'"
          class="online-btn online-btn-primary"
          :disabled="busy || !roomCodeInput.trim()"
          @click="handleJoin"
        >
          <Link2 :size="14" />
          {{ busy ? t("tools.online.joining") : t("tools.online.join") }}
        </button>
      </div>
    </div>

    <div v-if="onlinePlayers.length > 0" class="online-sec">
      <div class="online-sec-title">{{ t("tools.online.players", { count: onlinePlayers.length }, onlinePlayers.length) }}</div>
      <ul class="online-players">
        <li v-for="p in onlinePlayers" :key="p.machine_id" class="online-player">
          <span class="online-player-dot" :class="p.kind === 'HOST' ? 'host' : 'guest'" />
          <span class="online-player-name">{{ p.name }}</span>
          <span class="online-player-tag">{{ p.kind === "HOST" ? t("tools.online.roleHost") : p.kind === "LOCAL" ? t("tools.online.roleSelf") : t("tools.online.roleGuest") }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.online-panel {
  display: flex;
  flex-direction: column;
}

.online-status {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 8px 0;
  font-size: 13px;
  color: var(--text-secondary);
}

.online-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--text-tertiary);
}

.online-dot.active {
  background-color: var(--color-brand);
  box-shadow: 0 0 6px var(--color-brand);
}

.online-dot.pulse {
  animation: online-pulse 1.2s ease-in-out infinite;
}

@keyframes online-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.online-status-text { font-weight: 500; }

.online-sec {
  padding: 12px 0;
  border-top: 1px solid var(--border-default);
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

.online-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 5px 0;
}

.online-label {
  flex: 0 0 64px;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.online-input {
  flex: 1;
  min-width: 0;
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

.online-room {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 8px 0;
}

.online-room-code {
  font-family: "Cascadia Mono", "Consolas", monospace;
  font-size: 14px;
  font-weight: 600;
  color: var(--color-brand);
  letter-spacing: 0.5px;
  user-select: none;
}

.online-hint {
  font-size: 12px;
  color: var(--text-tertiary);
}

.online-waiting {
  padding: 8px 0;
  font-size: 12.5px;
  color: var(--text-tertiary);
  font-style: italic;
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

.online-btn-stop {
  color: var(--color-danger);
}

.online-btn-primary {
  color: #fff;
  background-color: var(--color-brand);
  border-color: var(--color-brand);
}

.online-btn-primary:hover:not(:disabled) {
  background-color: var(--color-brand);
  color: #fff;
  filter: brightness(1.08);
}

.online-error {
  margin-top: 8px;
  padding: 8px 10px;
  font-size: 12px;
  line-height: 1.5;
  color: var(--color-danger);
  background-color: var(--color-danger-bg);
  background-color: color-mix(in srgb, var(--color-danger) 8%, transparent);
  border-radius: var(--radius-sm);
  white-space: pre-wrap;
  word-break: break-all;
}

.online-players {
  list-style: none;
  margin: 4px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.online-player {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  background-color: var(--surface-default);
  background-color: color-mix(in srgb, var(--surface-default) 60%, transparent);
}

.online-player-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background-color: var(--text-tertiary);
}

.online-player-dot.host {
  background-color: var(--color-brand);
}

.online-player-dot.guest {
  background-color: var(--text-secondary);
}

.online-player-name {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.online-player-tag {
  flex: 0 0 auto;
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>
