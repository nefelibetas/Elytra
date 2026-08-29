<script setup lang="ts">
// 联机设置卡片：EasyTier 虚拟局域网。host 开房间拿码，guest 输码加入，
// 加入后本地端口转发并模拟 MC 局域网广播，游戏内"局域网"标签页点 "Elytra 联机" 进入
// （MC 26.2 对手动 IP 连接校验会话，离线账号需走局域网发现路径）。
// 顺序自由：可先点"开启联机"（等待态），再在游戏里"对局域网开放"；
// 检测到游戏端口后自动生成完整房间码。
// 同网直连：host 局域网 IP 自动编入房间码（@IP 段），guest 解析后直连 host；
// 跨网联机需配置协调服务器（easytier 官方公共节点已下线）。
// 会话模式：host 正版登录开局域网 → 服务器验证会话，朋友需正版；离线 host → 任意可进。
import { onMounted, ref } from "vue";
import { Copy, Link2, Save, Unplug } from "@lucide/vue";
import {
  onlinePublicNodes, onlineStatus, onlineRoomCode, onlineLocalPort,
  onlineErrorHost, onlineErrorGuest,
  startHost, joinRoom, stopOnline, loadOnlineStatus, loadOnlineConfig,
  saveOnlineConfig, ensureOnlineListen,
} from "../store/online";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";

onMounted(() => {
  ensureOnlineListen();
  void loadOnlineStatus();
  void loadOnlineConfig();
});

/** guest 输入的房间码。 */
const roomCodeInput = ref("");
const busy = ref(false);
/** 协调服务器保存中。 */
const savingConfig = ref(false);

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
  try {
    await navigator.clipboard.writeText(onlineRoomCode.value);
    showToast("房间码已复制");
  } catch {
    showToast("复制失败，请手动复制");
  }
}

async function handleSaveConfig() {
  savingConfig.value = true;
  try {
    await saveOnlineConfig();
    showToast("协调服务器已保存");
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingConfig.value = false;
  }
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>联机</h3>
      <div class="stg-card-sub">EasyTier 虚拟局域网：无需公网服务器，房间码互连</div>
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

      <!-- 等待态：提示 + 停止 -->
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
          placeholder="U/XXXX-XXXX-XXXX-XXXX:端口"
          :disabled="onlineStatus !== 'idle'"
          @keyup.enter="handleJoin"
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
    <!-- 协调服务器（跨网联机） -->
    <div class="online-sec">

      <div class="online-sec-title">协调服务器（跨网联机）</div>
      <p class="online-desc">跨网联机需连同一个协调服务器才能互相发现。默认内置社区节点，也可填自己的地址（每行一个，如 <span class="online-mono">tcp://1.2.3.4:11010</span>），留空仅支持同一局域网。</p>
      <a
        class="online-link"
        href="https://easytier.cn"
        target="_blank"
        rel="noopener"
      >如何搭建/寻找协调服务器（EasyTier 教程）→</a>
      <textarea
        v-model="onlinePublicNodes"
        class="online-textarea"
        rows="3"
        placeholder="tcp://服务器地址:11010"
      ></textarea>
      <div class="online-actions">
        <button class="online-btn" :disabled="savingConfig" @click="handleSaveConfig">
          <Save :size="14" />
          {{ savingConfig ? "保存中..." : "保存" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style>
@import "../styles/settings-shared.css";
</style>

<style scoped>
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
.online-link {
  display: inline-block;
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
.online-textarea {
  width: 100%;
  box-sizing: border-box;
  height: 64px;
  padding: 8px 10px;
  font-size: 13px;
  font-family: "Cascadia Mono", "Consolas", monospace;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  resize: none;
}
.online-textarea:focus {
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
/* 等待态提示 */
.online-waiting {
  padding: 8px 0;
  font-size: 12.5px;
  color: var(--text-tertiary);
  font-style: italic;
}
/* 按钮区：始终右下角 */
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
  color: var(--text-danger, #e5484d);
}
.online-btn-primary {
  color: #fff;
  background-color: var(--color-brand);
  border-color: var(--color-brand);
}
.online-btn-primary:hover:not(:disabled) {
  filter: brightness(1.1);
  color: #fff;
}
.online-error {
  padding: 6px 0;
  font-size: 12px;
  color: var(--text-danger, #e5484d);
}
</style>
