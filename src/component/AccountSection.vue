<script setup lang="ts">
// 账号区：正版/离线切换 + 用户区（头像/用户名/登录/操作菜单）。
// 菜单跟随鼠标 Teleport 到 body；window click 关闭菜单（组件挂载周期内注册）。
import { computed, onMounted, onUnmounted, watch } from "vue";
import { ShieldCheck, User, UserRound, Shirt, Pencil, LogOut, Users, ChevronDown } from "@lucide/vue";
import {
  authMode, msAccount, msAccounts, offlineAccounts, username, setAuthMode, msLoginState,
  startMsLogin, cancelMsLogin, switchMsAccount, removeMsAccount,
  saveOfflineAccount, removeOfflineAccount,
} from "../store/auth";
import { avatarState, scheduleOfflineRender, renderAccountHead, renderOfflineHead, accountHeads, offlineHeads } from "../store/avatar";
import { accountUi } from "../store/accountUi";
import { showError } from "../store/alert";
import MenuPopover from "./MenuPopover.vue";

const avatarUrl = computed(() =>
  authMode.value === "premium" ? avatarState.premiumAvatar : avatarState.offlineAvatar,
);

/** 用户名输入防抖 1.337s 后刷新离线头像 */
watch(username, (v) => scheduleOfflineRender(v));

// 离线用户名输入即存历史：防抖 1s（与头像刷新防抖并存；直接点启动时 launch.ts 兜底保存）
let offlineSaveTimer: number | undefined;
watch(username, (v) => {
  window.clearTimeout(offlineSaveTimer);
  if (v.trim()) {
    offlineSaveTimer = window.setTimeout(() => saveOfflineAccount(v.trim()), 1000);
  }
});

// window click 统一关闭菜单（组件挂载周期内注册；箭头函数保持 this 绑定）
const closeHandler = () => accountUi.closeMenus();

onMounted(() => window.addEventListener("click", closeHandler));
onUnmounted(() => {
  window.clearTimeout(offlineSaveTimer);
  window.removeEventListener("click", closeHandler);
});

// ── 账号列表 2D 头像（打开菜单时并发预加载，会话内缓存）──

let headLoadSeq = 0;

async function ensureAccountHeads() {
  const seq = ++headLoadSeq; // 组件重建/重复打开时丢弃过期结果
  for (const acc of msAccounts.value) {
    renderAccountHead(acc.uuid).then((h) => {
      if (seq === headLoadSeq && h) accountHeads[acc.uuid] = h;
    });
  }
}

// ── 离线历史头像（Steve/Alex 默认皮肤 2D 头部，哈希决定模型，与主头像/游戏内一致）──

async function ensureOfflineHeads() {
  for (const name of offlineAccounts.value) {
    if (offlineHeads[name]) continue;
    renderOfflineHead(name).then((h) => {
      if (h) offlineHeads[name] = h;
    });
  }
}

/** 切换正版账号（防止连点；失败 = refresh_token 失效需重登）。 */
async function switchAccount(uuid: string) {
  if (accountUi.accountsBusy) return;
  accountUi.accountsBusy = true;
  try {
    await switchMsAccount(uuid);
  } catch (e) {
    showError(`切换账号失败：${String(e)}`, true);
  } finally {
    accountUi.accountsBusy = false;
    accountUi.closeAccountsMenu();
  }
}

/** 移除正版账号（当前被移除时回退到上一账号）。 */
async function removeAccount(uuid: string) {
  try {
    await removeMsAccount(uuid);
  } catch (e) {
    showError(`移除账号失败：${String(e)}`, true);
  }
  accountUi.closeAccountsMenu();
}

/** 移除当前账号（登出按钮：从列表移除，而非清空全部）。 */
function removeCurrentMsAccount() {
  const acc = msAccount.value;
  if (acc) removeAccount(acc.uuid);
}

/** 从历史选择离线用户名。 */
function useOfflineName(name: string) {
  username.value = name;
  accountUi.closeOfflineMenu();
}
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
      <div class="user-area" :class="{ 'menu-open': accountUi.skinMenu.visible || accountUi.editMenu.visible || accountUi.accountsMenu.visible }">
        <div class="user-main">
          <div class="avatar">
            <img
              v-if="avatarUrl && !avatarState.avatarError"
              :src="avatarUrl"
              alt="avatar"
              width="52"
              draggable="false"
              @error="avatarState.avatarError = true"
            />
            <UserRound v-else :size="34" />
          </div>
          <div v-if="authMode === 'premium'" class="uname" :class="{ 'uname-empty': !msAccount }">
            {{ msAccount?.name || '未登录' }}
          </div>
          <div v-else class="offline-name">
            <input
              v-model="username"
              class="name-input"
              type="text"
              placeholder="输入用户名"
              maxlength="16"
            />
            <button
              class="icon-btn hist-btn"
              :class="{ open: accountUi.offlineMenu.visible }"
              v-tooltip="'历史用户名'"
              @click.stop="accountUi.openOfflineMenu(); ensureOfflineHeads()"
            >
              <ChevronDown :size="14" />
            </button>
            <Transition name="expand">
              <div v-if="accountUi.offlineMenu.visible" class="offline-pop" @click.stop>
                <template v-if="offlineAccounts.length">
                  <div
                    v-for="name in offlineAccounts"
                    :key="name"
                    class="acct-row"
                    @click="useOfflineName(name)"
                  >
                    <img
                      v-if="offlineHeads[name]"
                      :src="offlineHeads[name]"
                      class="acct-head"
                      alt=""
                      draggable="false"
                    />
                    <UserRound v-else :size="16" class="acct-icon" />
                    <span class="acct-name">{{ name }}</span>
                    <button
                      class="acct-del"
                      title="移除该用户名"
                      @click.stop="removeOfflineAccount(name)"
                    >×</button>
                  </div>
                </template>
                <div v-else class="menu-item acct-empty">暂无历史用户名</div>
              </div>
            </Transition>
          </div>
        </div>

        <div v-if="authMode === 'premium'" class="user-actions">
          <div v-if="msLoginState === 'waiting'" class="ms-login-waiting">
            <span class="ls-spin"></span>
            <span>请在浏览器中完成登录…</span>
            <button class="ms-cancel" @click="cancelMsLogin">取消</button>
          </div>
          <template v-else-if="msAccount">
            <!-- 账号：历史账号列表（切换/移除/登录新账号） -->
            <button
              class="icon-btn"
              v-tooltip="'账号列表'"
              @click.stop="accountUi.openAccountsMenu($event); ensureAccountHeads()"
            >
              <Users :size="14" />
            </button>
            <!-- 衣服：皮肤/披风菜单 -->
            <button class="icon-btn" v-tooltip="'皮肤与披风'" @click.stop="accountUi.openSkinMenu($event)">
              <Shirt :size="14" />
            </button>
            <!-- 编辑：账号信息菜单（修改密码 / 修改玩家名） -->
            <button class="icon-btn" v-tooltip="'编辑信息'" @click.stop="accountUi.openEditMenu($event)">
              <Pencil :size="14" />
            </button>
            <!-- 登出：从列表移除当前账号（其余历史账号保留） -->
            <button class="icon-btn" v-tooltip="'移除当前账号'" @click="removeCurrentMsAccount">
              <LogOut :size="14" />
            </button>
          </template>
          <button v-else class="act-btn" @click="startMsLogin">
            <ShieldCheck :size="14" />
            <span>登录微软账号</span>
          </button>
        </div>

        <!-- 皮肤/披风菜单（跟随鼠标，出现在右下方） -->
        <MenuPopover
          :visible="accountUi.skinMenu.visible"
          :x="accountUi.skinMenu.x"
          :y="accountUi.skinMenu.y"
        >
          <div class="menu-item" @click="accountUi.uploadSkin">修改皮肤</div>
          <div class="menu-item" @click="accountUi.refreshSkin">刷新皮肤</div>
          <div class="menu-item" @click="accountUi.saveSkin">保存皮肤文件</div>
          <div class="menu-sep"></div>
          <div class="menu-item" @click="accountUi.openCapeDialog">修改披风</div>
          <div class="menu-item" @click="accountUi.refreshCape">刷新披风列表</div>
        </MenuPopover>

        <!-- 编辑信息菜单（修改密码 / 修改玩家名） -->
        <MenuPopover
          :visible="accountUi.editMenu.visible"
          :x="accountUi.editMenu.x"
          :y="accountUi.editMenu.y"
        >
          <div class="menu-item" @click="accountUi.openPasswordPage">修改密码</div>
          <div class="menu-item" @click="accountUi.openNameChangePage">修改玩家名</div>
        </MenuPopover>

        <!-- 账号列表菜单（历史正版账号切换/移除/登录新账号） -->
        <MenuPopover
          :visible="accountUi.accountsMenu.visible"
          :x="accountUi.accountsMenu.x"
          :y="accountUi.accountsMenu.y"
          menu-class="acct-menu accounts"
          max-height="320px"
        >
            <template v-if="msAccounts.length">
              <div
                v-for="acc in msAccounts"
                :key="acc.uuid"
                class="acct-row"
                :class="{ current: acc.uuid === msAccount?.uuid }"
                @click="switchAccount(acc.uuid)"
              >
                <img
                  v-if="accountHeads[acc.uuid]"
                  :src="accountHeads[acc.uuid]"
                  class="acct-head"
                  alt=""
                  draggable="false"
                />
                <UserRound v-else :size="16" class="acct-icon" />
                <span class="acct-name">{{ acc.name }}</span>
                <span v-if="acc.uuid === msAccount?.uuid" class="acct-check">✓</span>
                <button
                  class="acct-del"
                  title="移除该账号"
                  @click.stop="removeAccount(acc.uuid)"
                >×</button>
              </div>
            </template>
            <div v-else class="menu-item acct-empty">暂无历史账号</div>
            <div class="menu-sep"></div>
            <div class="menu-item" @click="accountUi.closeAccountsMenu(); startMsLogin()">登录新账号</div>
        </MenuPopover>
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
  width: 56px;
  height: 56px;
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
  font-size: 14px;
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

/* 离线用户名 + 历史按钮同行 */
.offline-name {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 箭头按钮（无边框：icon-btn 的边框在样式表更后被覆盖，用双类提特异性） */
.icon-btn.hist-btn {
  flex-shrink: 0;
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  transition: transform 0.15s ease, color 0.15s;
}

.icon-btn.hist-btn:hover {
  color: var(--color-brand);
  background: transparent;
}

.icon-btn.hist-btn.open {
  transform: rotate(180deg);
  color: var(--color-brand);
}

/* 历史用户名展开面板：锚定输入框下方，固定高度（超出滚动） */
.offline-pop {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 20;
  max-height: 180px;
  overflow-y: auto;
  padding: 6px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
}

/* expand 动画目标值 = 固定高度（写在 enter-to/leave-from，写在 active 会覆盖 from 态导致无动画） */
.expand-enter-active,
.expand-leave-active {
  transition: max-height 0.2s ease, opacity 0.15s ease;
  overflow: hidden;
}

.expand-enter-from,
.expand-leave-to {
  max-height: 0;
  opacity: 0;
}

.expand-enter-to,
.expand-leave-from {
  max-height: 180px;
  opacity: 1;
}

/* premium hover actions：始终占位（不影响头像/用户名位置），仅切换透明度。
   显示条件 hover / 键盘焦点（:focus-within）/ 菜单展开——三者任一即显示。 */
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
.user-area:focus-within .user-actions,
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

/* 账号列表菜单：行 + 删除按钮（菜单壳与 .menu-item/.menu-sep 样式收敛在 MenuPopover） */

.acct-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  font-size: 13px;
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s;
}

.acct-row:hover {
  background: var(--surface-hover);
}

.acct-row.current {
  color: var(--color-brand);
  font-weight: 600;
}

.acct-icon {
  flex-shrink: 0;
  color: var(--text-tertiary);
}

/* 账号列表 2D 皮肤头像（8×8 头部放大，MC 像素风） */
.acct-head {
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  object-fit: cover;
  image-rendering: pixelated;
  display: block;
}

.acct-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 140px;
}

.acct-check {
  color: var(--color-brand);
  font-size: 12px;
}

.acct-del {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
  transition: all 0.12s;
}

.acct-row:hover .acct-del {
  opacity: 1;
}

.acct-del:hover {
  color: var(--color-danger);
  background: rgba(229, 72, 77, 0.12);
}

.menu-item.acct-empty {
  color: var(--text-tertiary);
  cursor: default;
}

/* 双类提特异性：压过 MenuPopover 内 :slotted(.menu-item:hover) 的品牌色反白 */
.menu-item.acct-empty:hover {
  background: transparent;
  color: var(--text-tertiary);
}

.ms-login-waiting {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
}

.ms-cancel {
  padding: 2px 10px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.ms-cancel:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
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
