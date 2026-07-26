<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";

const router = useRouter();

type AuthMode = "premium" | "offline";

const authMode = ref<AuthMode>("offline");
const username = ref("");
const selectedVersion = ref("暂未选择");
const hasInstalledVersions = ref(false);

function switchMode(mode: AuthMode) {
  authMode.value = mode;
}

function onLaunchClick() {
  if (!hasInstalledVersions.value) {
    router.push("/download");
  }
}

onMounted(async () => {
  try {
    const list: string[] = await invoke("get_installed_versions");
    hasInstalledVersions.value = list.length > 0;
    if (hasInstalledVersions.value) {
      selectedVersion.value = list[list.length - 1];
    }
  } catch {
    hasInstalledVersions.value = false;
  }
});
</script>

<template>
  <div class="launch">
    <div class="left">
      <!-- auth -->
      <div class="section s-auth">
        <div class="auth-toggle">
          <button
            class="auth-btn"
            :class="{ active: authMode === 'premium' }"
            @click="switchMode('premium')"
          >
            <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <path d="M8 1L3 5v3.5C3 12.3 5.3 15.5 8 16c2.7-.5 5-3.7 5-7.5V5L8 1z" />
            </svg>
            <span>正版</span>
          </button>
          <button
            class="auth-btn"
            :class="{ active: authMode === 'offline' }"
            @click="switchMode('offline')"
          >
            <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <path d="M8 2a4 4 0 100 8 4 4 0 000-8zm-6 9c0-1.3 2.7-2 6-2s6 .7 6 2v1H2v-1z" />
            </svg>
            <span>离线</span>
          </button>
        </div>
      </div>

      <!-- user -->
      <div class="section s-user">
        <div class="user-area">
          <div class="user-main">
            <div class="avatar">
              <svg viewBox="0 0 40 40" width="44" height="44" fill="currentColor">
                <circle cx="20" cy="20" r="20" />
                <circle cx="20" cy="15" r="6" fill="var(--surface-elevated)" opacity="0.8" />
                <ellipse cx="20" cy="30" rx="10" ry="7" fill="var(--surface-elevated)" opacity="0.8" />
              </svg>
            </div>
            <div v-if="authMode === 'premium'" class="uname">Steve</div>
            <input
              v-else
              v-model="username"
              class="name-input"
              type="text"
              placeholder="输入用户名"
              maxlength="16"
            />
          </div>

          <div v-if="authMode === 'premium'" class="user-actions">
            <button class="act-btn">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M14 5h-3V2a1 1 0 00-1-1H6a1 1 0 00-1 1v3H2a1 1 0 00-1 1v8a1 1 0 001 1h12a1 1 0 001-1V6a1 1 0 00-1-1zM6 2h4v3H6V2zm7 12H3V7h10v7z" />
              </svg>
              <span>皮肤</span>
            </button>
            <button class="act-btn">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M13.7 5.3l-1-1a1 1 0 00-1.4 0L4 11.6V14h2.4l7.3-7.3a1 1 0 000-1.4zM11 4l2 2-1.5 1.5L9.5 5.5 11 4zM3 12l1-1 1 1-1 1-1-1z" />
              </svg>
              <span>修改信息</span>
            </button>
            <button class="act-btn">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M11 4L9.5 5.5l1 1L13 4.5V3h-3.5L8 1.5 6.5 3H3v1.5l2.5 2 1-1L5.5 5H11v-1zM3 12l1.5-1.5-1-1L1 11.5V13h3.5L6 14.5 7.5 13H11v-1.5l-2.5-2-1 1 1.5 1.5H3v1z" />
              </svg>
              <span>切换账号</span>
            </button>
          </div>
        </div>
      </div>

      <!-- launch -->
      <div class="section s-launch">
        <div class="launch-area">
          <button class="launch-btn" @click="onLaunchClick">
            <template v-if="hasInstalledVersions">
              <span class="launch-label">启动游戏</span>
              <span class="launch-version">{{ selectedVersion }}</span>
            </template>
            <template v-else>
              <span class="launch-label">下载游戏</span>
              <span class="launch-version">未找到可用的游戏版本</span>
            </template>
          </button>
          <div class="launch-actions">
            <button class="secondary-btn">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z" />
              </svg>
              <span>版本选择</span>
            </button>
            <button class="secondary-btn">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M13.7 5.3l-1-1a1 1 0 00-1.4 0L4 11.6V14h2.4l7.3-7.3a1 1 0 000-1.4zM11 4l2 2-1.5 1.5L9.5 5.5 11 4zM3 12l1-1 1 1-1 1-1-1z" />
              </svg>
              <span>版本设置</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="right">
      <div v-if="selectedVersion === '暂未选择'" class="right-hint">
        <svg viewBox="0 0 48 48" width="48" height="48" fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.3">
          <rect x="8" y="10" width="32" height="28" rx="3"/>
          <line x1="16" y1="20" x2="32" y2="20"/>
          <line x1="16" y1="26" x2="28" y2="26"/>
          <line x1="16" y1="32" x2="24" y2="32"/>
        </svg>
        <p>选择一个游戏版本</p>
      </div>
      <div v-else class="right-placeholder"></div>
    </div>
  </div>
</template>

<style scoped>
.launch {
  display: flex;
  height: 100%;
}

/* ===== left ===== */
.left {
  flex: 0 0 35%;
  min-width: 280px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  display: flex;
  flex-direction: column;
  padding: 20px 24px;
  gap: 16px;
}

.section {
  display: flex;
  align-items: center;
}

.s-auth { /* natural height */ }
.s-user { flex: 1; justify-content: center; }
.s-launch { align-items: flex-start; }

/* auth toggle — like titlebar nav */
.auth-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  width: 100%;
}

.auth-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 20px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.auth-btn:hover {
  color: var(--text-secondary);
  background-color: var(--surface-hover);
}

.auth-btn.active {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* user section */
.user-area {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.user-main {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}

.avatar {
  width: 56px;
  height: 56px;
  border-radius: 50%;
  overflow: hidden;
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.avatar svg {
  width: 100%;
  height: 100%;
}

.uname {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.name-input {
  width: 100%;
  border: none;
  border-bottom: 2px solid var(--border-default);
  background: transparent;
  padding: 4px 0;
  font-size: 15px;
  font-weight: 500;
  color: var(--text-primary);
  text-align: center;
  outline: none;
  transition: border-color 0.15s;
}

.name-input:focus { border-color: var(--color-brand); }
.name-input::placeholder {
  color: var(--text-tertiary);
  font-weight: 400;
}

/* premium hover actions */
.user-actions {
  display: flex;
  gap: 8px;
  overflow: hidden;
  max-height: 0;
  opacity: 0;
  transition: all 0.2s ease;
}

.user-area:hover .user-actions {
  max-height: 40px;
  opacity: 1;
}

.act-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}

.act-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* launch section */
.launch-area {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.launch-btn {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  padding: 16px;
  border-radius: var(--radius-lg);
  border: none;
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 4px 16px var(--color-brand-glow);
}

.launch-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 6px 20px var(--color-brand-glow);
}

.launch-btn:active:not(:disabled) { transform: translateY(0); }
.launch-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  box-shadow: none;
}

.launch-label {
  font-size: 16px;
  font-weight: 600;
  line-height: 1.4;
}

.launch-version {
  font-size: 12px;
  opacity: 0.8;
  line-height: 1.3;
}

.launch-actions {
  display: flex;
  gap: 8px;
}

.secondary-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 0;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background-color: var(--surface-elevated);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.secondary-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

/* ===== right ===== */
.right {
  flex: 1;
  background-color: var(--surface-page);
  display: flex;
  align-items: center;
  justify-content: center;
}

.right-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--text-tertiary);
  user-select: none;
}

.right-hint p {
  font-size: 14px;
}

.right-placeholder {
  width: 100%;
  height: 100%;
}
</style>
