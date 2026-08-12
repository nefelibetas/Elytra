<script setup lang="ts">
// 账号区：正版/离线切换 + 用户区（头像/用户名/登录/操作菜单）。
// 菜单跟随鼠标 Teleport 到 body；window click 关闭菜单（组件挂载周期内注册）。
import { computed, onMounted, onUnmounted, watch } from "vue";
import { ShieldCheck, User, UserRound, Shirt, Pencil, LogOut } from "@lucide/vue";
import {
  authMode, msAccount, username, setAuthMode, msLoginState, startMsLogin, logoutMs,
} from "../store/auth";
import { avatarState, scheduleOfflineRender } from "../store/avatar";
import { accountUi } from "../store/accountUi";

const avatarUrl = computed(() =>
  authMode.value === "premium" ? avatarState.premiumAvatar : avatarState.offlineAvatar,
);

/** 用户名输入防抖 1.337s 后刷新离线头像 */
watch(username, (v) => scheduleOfflineRender(v));

onMounted(() => window.addEventListener("click", accountUi.closeMenus));
onUnmounted(() => window.removeEventListener("click", accountUi.closeMenus));
</script>

<template>
  <div class="account-sec">
    <div class="section s-auth">
      <div class="auth-toggle">
        <button
          class="auth-btn"
          :class="{ active: authMode === 'premium' }"
          @click="setAuthMode('premium')"
        >
          <ShieldCheck :size="16" />
          <span>正版</span>
        </button>
        <button
          class="auth-btn"
          :class="{ active: authMode === 'offline' }"
          @click="setAuthMode('offline')"
        >
          <User :size="16" />
          <span>离线</span>
        </button>
      </div>
    </div>

    <div class="section s-user">
      <div class="user-area" :class="{ 'menu-open': accountUi.skinMenu.visible || accountUi.editMenu.visible }">
        <div class="user-main">
          <div class="avatar">
            <img
              v-if="avatarUrl && !avatarState.avatarError"
              :src="avatarUrl"
              alt="avatar"
              width="68"
              draggable="false"
              @error="avatarState.avatarError = true"
            />
            <UserRound v-else :size="44" />
          </div>
          <div v-if="authMode === 'premium'" class="uname" :class="{ 'uname-empty': !msAccount }">
            {{ msAccount?.name || '未登录' }}
          </div>
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
          <div v-if="msLoginState === 'waiting'" class="ms-login-waiting">
            <span class="ls-spin"></span>
            <span>请在浏览器中完成登录…</span>
          </div>
          <template v-else-if="msAccount">
            <!-- 衣服：皮肤/披风菜单 -->
            <button class="icon-btn" title="皮肤与披风" @click.stop="accountUi.openSkinMenu($event)">
              <Shirt :size="14" />
            </button>
            <!-- 编辑：账号信息菜单（修改密码 / 修改玩家名） -->
            <button class="icon-btn" title="编辑信息" @click.stop="accountUi.openEditMenu($event)">
              <Pencil :size="14" />
            </button>
            <!-- 登出 -->
            <button class="icon-btn" title="退出登录" @click="logoutMs">
              <LogOut :size="14" />
            </button>
          </template>
          <button v-else class="act-btn" @click="startMsLogin">
            <ShieldCheck :size="14" />
            <span>登录微软账号</span>
          </button>
        </div>

        <!-- 皮肤/披风菜单（跟随鼠标，出现在右下方） -->
        <Teleport to="body">
          <div
            v-if="accountUi.skinMenu.visible"
            class="skin-menu"
            :style="{ left: accountUi.skinMenu.x + 'px', top: accountUi.skinMenu.y + 'px' }"
            @click.stop
          >
            <div class="menu-item" @click="accountUi.uploadSkin">修改皮肤</div>
            <div class="menu-item" @click="accountUi.refreshSkin">刷新皮肤</div>
            <div class="menu-item" @click="accountUi.saveSkin">保存皮肤文件</div>
            <div class="menu-sep"></div>
            <div class="menu-item" @click="accountUi.openCapeDialog">修改披风</div>
            <div class="menu-item" @click="accountUi.refreshCape">刷新披风列表</div>
          </div>
        </Teleport>

        <!-- 编辑信息菜单（修改密码 / 修改玩家名） -->
        <Teleport to="body">
          <div
            v-if="accountUi.editMenu.visible"
            class="skin-menu"
            :style="{ left: accountUi.editMenu.x + 'px', top: accountUi.editMenu.y + 'px' }"
            @click.stop
          >
            <div class="menu-item" @click="accountUi.openPasswordPage">修改密码</div>
            <div class="menu-item" @click="accountUi.openNameChangePage">修改玩家名</div>
          </div>
        </Teleport>
      </div>
    </div>
  </div>
</template>

<style scoped>
.account-sec {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.section {
  display: flex;
  align-items: center;
}

.s-auth { /* natural height */ }
.s-user { flex: 1; justify-content: center; }

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
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

/* 扩大 hover 热区：鼠标在头像/用户名四周也能保持操作行显示（不占布局、不挡点击） */
.user-area::before {
  content: "";
  position: absolute;
  inset: -20px -32px;
}

.user-main {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}

.avatar {
  width: 72px;
  height: 72px;
  border-radius: 0;
  overflow: hidden;
  color: var(--color-brand);
  /* 占位图标时无背景，与周围一致；图片时由 img 铺满 */
  background-color: transparent;
}

.avatar svg {
  width: 100%;
  height: 100%;
}

.avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  image-rendering: pixelated;
}

.uname {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.uname-empty {
  color: var(--text-tertiary);
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
  /* 盖过 .user-area::before hover 热区，保证可点击聚焦 */
  position: relative;
  z-index: 1;
}

.name-input:focus { border-color: var(--color-brand); }
.name-input::placeholder {
  color: var(--text-tertiary);
  font-weight: 400;
}

/* premium hover actions：始终占位（不影响头像/用户名位置），仅切换透明度 */
.user-actions {
  position: relative;
  z-index: 1; /* 盖过 .user-area::before 热区，保证按钮可点击 */
  display: flex;
  gap: 8px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.2s ease;
}

.user-area:hover .user-actions,
.user-area.menu-open .user-actions {
  opacity: 1;
  pointer-events: auto;
}

/* 图标操作按钮（衣服 / 编辑 / 登出） */
.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}

.icon-btn:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

/* 皮肤/披风菜单 */
.skin-menu {
  position: fixed;
  z-index: 1000;
  min-width: 164px;
  padding: 6px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.menu-item {
  padding: 8px 12px;
  font-size: 13px;
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s;
}

.menu-item:hover {
  background: var(--color-brand);
  color: #fff;
}

.menu-sep {
  height: 1px;
  margin: 5px 8px;
  background: var(--border-default);
}

.ms-login-waiting {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
}

.ls-spin {
  width: 14px;
  height: 14px;
  border: 2px solid var(--border-strong);
  border-top-color: var(--color-brand);
  border-radius: 50%;
  animation: ls-rotate 0.8s linear infinite;
}

@keyframes ls-rotate {
  to { transform: rotate(360deg); }
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
</style>
