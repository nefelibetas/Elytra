<script setup lang="ts">
// 联机工具：EasyTier 虚拟局域网操作（host 开房间 / guest 加入 / 停止）。
// 配置（协调服务器、广播名）在"设置 → 联机设置"；此处只管会话操作。
// 顺序自由：host 可先开联机（等待态），再在游戏里"对局域网开放"；
// 检测到游戏端口后自动生成完整房间码。
// Scaffolding 协议：host 开房间（房间码 U/XXXX-XXXX-XXXX-XXXX），guest 加入；
// 跨网联机需在设置里配置协调服务器。
// 会话模式：host 正版登录开局域网 → 服务器验证会话，朋友需正版；离线 host → 任意可进。
import { onMounted, ref } from "vue";
import { Copy, Link2, Unplug } from "@lucide/vue";
import {
  onlineStatus, onlineRoomCode, onlineLocalPort, onlinePlayers,
  onlineErrorHost, onlineErrorGuest,
  startHost, joinRoom, stopOnline, loadOnlineStatus, ensureOnlineListen,
} from "../../store/config/online";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { copyText } from "../../utils/clipboard";

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
    showToast("联机已开启，等待游戏开放局域网");
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
    showToast(`已加入，游戏内局域网标签页点 "Elytra 联机"`);
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    busy.value = false;
  }
}

async function handleStop() {
  try {
    await stopOnline();
    showToast("联机已停止");
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function copyRoomCode() {
  await copyText(onlineRoomCode.value, "房间码已复制");
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>联机</h3>
      <div class="tool-card-sub">EasyTier 虚拟局域网：无需公网服务器，房间码互连</div>
    </div>

    <!-- 状态行 -->
    <div class="online-status">
      <span class="online-dot" :class="{ active: onlineStatus !== 'idle', pulse: onlineStatus === 'waiting' }" />
      <span class="online-status-text">
        {{
          onlineStatus === "hosting" ? "联机中（主机）"
          : onlineStatus === "joining" ? "联机中（已加入）"
          : onlineStatus === "waiting" ? "等待游戏中开启局域网…"
          : "未联机"
        }}
      </span>
    </div>

    <!-- host：作为主机 -->
    <div class="online-sec">
      <div class="online-sec-title">作为主机</div>
      <p class="online-desc">点"开启联机"后，游戏里"对局域网开放"即可——启动器自动生成房间码，发给朋友即可加入。</p>
      <p class="online-desc">联机昵称与广播名可在 设置 → 联机设置 中配置（昵称显示给其他玩家，广播名是 guest 端看到的房间名）。</p>

      <div v-if="onlineStatus === 'waiting'" class="online-waiting">
        游戏开放局域网后这里会显示房间码
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
          复制房间码
        </button>
        <button
          v-if="onlineStatus === 'hosting' || onlineStatus === 'waiting'"
          class="online-btn online-btn-stop"
          @click="handleStop"
        >
          <Unplug :size="14" />
          停止联机
        </button>
        <button
          v-else-if="onlineStatus === 'idle'"
          class="online-btn online-btn-primary"
          :disabled="busy"
          @click="handleStartHost"
        >
          <Link2 :size="14" />
          {{ busy ? "开启中..." : "开启联机" }}
        </button>
      </div>
    </div>

    <!-- guest：加入联机 -->
    <div class="online-sec">
      <div class="online-sec-title">加入联机</div>
      <p class="online-desc">输入主机分享的房间码，加入后游戏内多人游戏 → 局域网标签页点 "Elytra 联机" 进入。</p>

      <div class="online-row">
        <span class="online-label">房间码</span>
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
        <span class="online-room-code">Elytra 联机</span>
        <span class="online-hint">游戏内局域网标签页已出现，点击进入（本地端口 {{ onlineLocalPort }}）</span>
      </div>

      <div v-if="onlineErrorGuest" class="online-error">{{ errMsg(onlineErrorGuest) }}</div>

      <div class="online-actions">
        <button
          v-if="onlineStatus === 'joining'"
          class="online-btn online-btn-stop"
          @click="handleStop"
        >
          <Unplug :size="14" />
          停止联机
        </button>
        <button
          v-else-if="onlineStatus === 'idle'"
          class="online-btn online-btn-primary"
          :disabled="busy || !roomCodeInput.trim()"
          @click="handleJoin"
        >
          <Link2 :size="14" />
          {{ busy ? "加入中..." : "加入联机" }}
        </button>
      </div>
    </div>

    <!-- 玩家列表 -->
    <div v-if="onlinePlayers.length > 0" class="online-sec">
      <div class="online-sec-title">玩家（{{ onlinePlayers.length }}）</div>
      <ul class="online-players">
        <li v-for="p in onlinePlayers" :key="p.machine_id" class="online-player">
          <span class="online-player-dot" :class="p.kind === 'HOST' ? 'host' : 'guest'" />
          <span class="online-player-name">{{ p.name }}</span>
          <span class="online-player-tag">{{ p.kind === "HOST" ? "主机" : p.kind === "LOCAL" ? "我" : "房客" }}</span>
        </li>
      </ul>
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
