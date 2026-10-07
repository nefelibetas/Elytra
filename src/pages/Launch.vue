<script setup lang="ts">
// 启动页容器：左列（启动进度 / 账号区 + 启动区）编排 + 右列占位 + 错误弹窗。
// 业务逻辑在 store（launch/auth/accountUi/avatar/alert），本文件仅做初始化编排。
import { onMounted } from "vue";
import { FileText } from "@lucide/vue";
import AlertDialog from "../component/common/AlertDialog.vue";
import LaunchProgress from "../component/launch/LaunchProgress.vue";
import AccountSection from "../component/launch/AccountSection.vue";
import LaunchSection from "../component/launch/LaunchSection.vue";
import VersionDetailCard from "../component/launch/VersionDetailCard.vue";
import { alertState } from "../store/alert";
import { renderOfflineAvatar, renderPremiumAvatar } from "../store/avatar";
import {
  msAccount, username, ensureAuthReady, loadOfflineAccounts, loadLastUsername, ensureLoginListener,
} from "../store/auth";
import { launchState, loadVersions, ensureGameExitListener } from "../store/launch";
import { selectedVersion } from "../store/selectedVersion";

onMounted(async () => {
  await loadLastUsername();
  await ensureAuthReady(); // 正版 / 第三方账号列表（衣柜判定来源也要等这一份）
  await loadOfflineAccounts();
  void renderOfflineAvatar(username.value);
  // 账号变化（登录/退出）时渲染正版头像；初始已保存的账号也立即渲染
  // （watch(msAccount) 已移入 auth store 模块级，路由往返不累积注册）
  void renderPremiumAvatar(msAccount.value?.uuid ?? "");

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

</style>
