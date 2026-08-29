<script setup lang="ts">
// 启动页容器：左列（启动进度 / 账号区 + 启动区）编排 + 右列占位 + 错误弹窗。
// 业务逻辑在 store（launch/auth/accountUi/avatar/alert），本文件仅做初始化编排。
import { onMounted, ref } from "vue";
import { FileText } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { errMsg } from "../utils/error";
import AlertDialog from "../component/AlertDialog.vue";
import LaunchProgress from "../component/LaunchProgress.vue";
import AccountSection from "../component/AccountSection.vue";
import LaunchSection from "../component/LaunchSection.vue";
import CapeDialog from "../component/CapeDialog.vue";
import VersionDetailCard from "../component/VersionDetailCard.vue";
import { alertState } from "../store/alert";
import { activePath } from "../store/gameDirs";
import { renderOfflineAvatar, renderPremiumAvatar } from "../store/avatar";
import {
  msAccount, username, loadMsAccounts, loadOfflineAccounts, loadLastUsername, ensureLoginListener,
} from "../store/auth";
import { launchState, loadVersions, ensureGameExitListener } from "../store/launch";
import { selectedVersion } from "../store/selectedVersion";

onMounted(async () => {
  await loadLastUsername();
  await loadMsAccounts();
  await loadOfflineAccounts();
  void renderOfflineAvatar(username.value);
  // 账号变化（登录/退出）时渲染正版头像；初始已保存的账号也立即渲染
  // （watch(msAccount) 已移入 auth store 模块级，路由往返不累积注册）
  void renderPremiumAvatar(msAccount.value?.uuid ?? "");

  await ensureLoginListener();
  await ensureGameExitListener();
  await loadVersions();
});

// ── 游戏日志 dialog ──
const logDialog = ref(false);
const logText = ref("");

/** 打开游戏日志弹窗：读取当前激活目录（.minecraft）logs 下的日志尾部。 */
async function openLog() {
  if (!activePath.value) return;
  try {
    logText.value = await invoke<string>("get_game_log", { gameDir: activePath.value });
  } catch (e) {
    logText.value = errMsg(e);
  }
  logDialog.value = true;
}
</script>

<template>
  <div class="launch">
    <div class="left">
      <Transition name="left-switch" mode="out-in">
        <LaunchProgress v-if="launchState.phase" key="progress" />
        <div v-else key="normal" class="left-normal">
          <AccountSection />
          <LaunchSection />
          <button class="log-btn" @click="openLog">游戏日志</button>
        </div>
      </Transition>
    </div>

    <div class="right">
      <Transition name="right-switch" mode="out-in">
        <div v-if="!selectedVersion" key="hint" class="right-hint">
          <FileText :size="48" opacity="0.3" />
          <p>选择一个游戏版本</p>
        </div>
        <VersionDetailCard v-else key="card" />
      </Transition>
    </div>

    <AlertDialog
      v-model="alertState.show"
      title="启动失败"
      :message="alertState.msg"
      :copyable="alertState.copyable"
    />

    <AlertDialog v-model="logDialog" title="游戏日志" width="lg">
      <pre class="log-pre">{{ logText }}</pre>
      <template #actions>
        <button class="log-close-btn" @click="logDialog = false">关闭</button>
      </template>
    </AlertDialog>

    <CapeDialog />
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

.left-normal {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.left-switch-enter-active,
.left-switch-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}

.left-switch-enter-from {
  opacity: 0;
  transform: translateY(10px);
}

.left-switch-leave-to {
  opacity: 0;
  transform: translateY(-10px);
}

/* ===== right ===== */
.right {
  flex: 1;
  background-color: var(--surface-page);
  display: flex;
  align-items: stretch;
  justify-content: flex-start;
  padding: 16px;
  overflow: hidden;
}
.right-hint {
  width: 100%; /* 撑满 .right（flex 行方向不拉伸主轴，宽度塌缩成内容会水平偏左）
                 —— 让 justify-content:center 真正水平居中 */
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-tertiary);
  user-select: none;
}

.right-hint p {
  font-size: 14px;
}

/* 右侧 hint ↔ 版本卡片切换动画 */
.right-switch-enter-active,
.right-switch-leave-active {
  transition: opacity 0.18s ease;
}

.right-switch-enter-from,
.right-switch-leave-to {
  opacity: 0;
}
/* 游戏日志按钮（参考 LaunchSection 的 secondary-btn） */
.log-btn {
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

.log-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

/* 游戏日志 dialog：等宽、可滚动内容区 */
.log-pre {
  margin: 0;
  max-height: 55vh;
  overflow: auto;
  font-family: Consolas, "Cascadia Code", "Courier New", monospace;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-all;
  user-select: text;
}

.log-close-btn {
  padding: 5px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.log-close-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
}
</style>
