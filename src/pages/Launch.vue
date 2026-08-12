<script setup lang="ts">
// 启动页容器：左列（启动进度 / 账号区 + 启动区）编排 + 右列占位 + 错误弹窗。
// 业务逻辑在 store（launch/auth/accountUi/avatar/alert），本文件仅做初始化编排。
import { onMounted } from "vue";
import { FileText } from "@lucide/vue";
import AlertDialog from "../component/AlertDialog.vue";
import LaunchProgress from "../component/LaunchProgress.vue";
import AccountSection from "../component/AccountSection.vue";
import LaunchSection from "../component/LaunchSection.vue";
import CapeDialog from "../component/CapeDialog.vue";
import { alertState } from "../store/alert";
import { renderOfflineAvatar, renderPremiumAvatar } from "../store/avatar";
import {
  msAccount, username, loadMsAccount, loadLastUsername, ensureLoginListener,
} from "../store/auth";
import { launchState, loadVersions, ensureGameExitListener } from "../store/launch";
import { selectedVersion } from "../store/selectedVersion";

onMounted(async () => {
  await loadLastUsername();
  await loadMsAccount();
  renderOfflineAvatar(username.value);
  // 账号变化（登录/退出）时渲染正版头像；初始已保存的账号也立即渲染
  // （watch(msAccount) 已移入 auth store 模块级，路由往返不累积注册）
  renderPremiumAvatar(msAccount.value?.uuid ?? "");

  await ensureLoginListener();
  await ensureGameExitListener();
  await loadVersions();
});
</script>

<template>
  <div class="launch">
    <div class="left">
      <Transition name="left-switch" mode="out-in">
        <LaunchProgress v-if="launchState.phase" key="progress" />
        <div v-else key="normal" class="left-normal">
          <AccountSection />
          <LaunchSection />
        </div>
      </Transition>
    </div>

    <div class="right">
      <div v-if="!selectedVersion" class="right-hint">
        <FileText :size="48" opacity="0.3" />
        <p>选择一个游戏版本</p>
      </div>
      <div v-else class="right-placeholder"></div>
    </div>

    <AlertDialog
      v-model="alertState.show"
      title="启动失败"
      :message="alertState.msg"
      :copyable="alertState.copyable"
    />

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
