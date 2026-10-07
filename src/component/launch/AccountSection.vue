<script setup lang="ts">
// 账号区：正版/离线切换 + 用户区（头像/用户名/登录/操作菜单）。
// 菜单跟随鼠标 Teleport 到 body；window click 关闭菜单（组件挂载周期内注册）。
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { ShieldCheck, User, UserRound, Pencil, LogOut, Users, ChevronDown, Link2, RotateCcw, RefreshCw } from "@lucide/vue";
import {
  authMode, msAccount, msAccounts, offlineAccounts, username, setAuthMode, msLoginState,
  startMsLogin, cancelMsLogin, switchMsAccount, removeMsAccount,
  saveOfflineAccount, removeOfflineAccount,
  externalAccount, loadExternalAccounts, switchExternalRole, refreshExternalAccount,
  type MsAccountInfo,
} from "../../store/auth";
import { avatarState, scheduleOfflineRender, renderAccountHead, renderOfflineHead, accountHeads, offlineHeads, renderSkinHead3D, renderExternalAvatar, renderPremiumAvatar } from "../../store/avatar";
import { accountUi } from "../../store/accountUi";
import { showToast } from "../../utils/toast";
import { invoke } from "@tauri-apps/api/core";
import { showError } from "../../store/alert";
import { settingsTab } from "../../store/settingsTab";
import { useRouter } from "vue-router";
import MenuPopover from "../common/MenuPopover.vue";

const router = useRouter();

const avatarUrl = computed(() =>
  authMode.value === "premium"
    ? avatarState.premiumAvatar
    : authMode.value === "thirdparty"
      ? avatarState.externalAvatar
      : avatarState.offlineAvatar,
);

/** 头像加载失败后手动重试（force 绕过去重缓存）。 */
function retryAvatar() {
  avatarState.avatarError = false;
  if (authMode.value === "premium") {
    void renderPremiumAvatar(msAccount.value?.uuid ?? "", true);
  } else if (authMode.value === "thirdparty") {
    void renderExternalAvatar(externalAccount.value?.api_url ?? "", externalAccount.value?.uuid ?? "", true);
  }
}

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
const closeHandler = () => {
  accountUi.closeMenus();
  showRoles.value = false;
};

function goThirdPartyLogin() {
  settingsTab.value = "thirdparty";
  void router.push("/settings");
}
/** 点皮肤头像 → 皮肤衣柜（3D 预览 + 皮肤/披风管理）。 */
function openSkinStudio() {
  accountUi.closeMenus();
  void router.push("/skin-studio");
}
/** 第三方已登录时的附加操作（多角色切换 / 皮肤下载）。 */
const showRoles = ref(false);
/** 用户区 hover 状态（显示操作按钮；模板中直接赋值 ref 解包不写回，用函数显式 .value）。 */
const extHover = ref(false);
function onExtEnter() { extHover.value = true; }
function onExtLeave() { extHover.value = false; }
/** 角色头像缓存（profile_id → dataURL），面板打开时懒加载。 */
const roleHeads = reactive<Record<string, string>>({});
/** 拉取角色皮肤头（2D 大头照）。 */
async function loadRoleHead(apiUrl: string, roleId: string) {
  if (roleHeads[roleId]) return;
  try {
    const b64 = await invoke<string>("fetch_external_skin", { apiUrl, uuid: roleId });
    roleHeads[roleId] = await renderSkinHead3D(b64);
  } catch {
    // 无皮肤：保持占位
  }
}
// 打开角色面板：先 refresh 账号（自动读取最新角色列表，覆盖皮肤站增删角色/改名的极端场景），
// 再懒加载全部角色头像
watch(showRoles, async (open) => {
  if (!open) return;
  const acc = externalAccount.value;
  if (!acc) return;
  try {
    await refreshExternalAccount(acc.api_url, acc.uuid);
  } catch (e) {
    showError(`角色列表刷新失败：${String(e)}`, true);
    showRoles.value = false;
    return;
  }
  const fresh = externalAccount.value;
  if (!fresh) return;
  for (const r of fresh.profiles ?? []) {
    void loadRoleHead(fresh.api_url, r.id);
  }
});

/** 刷新角色和皮肤：refresh 账号（最新 token + 角色列表同步）→ 重拉皮肤渲染头像。 */
async function refreshExternalSkin() {
  const acc = externalAccount.value;
  if (!acc) return;
  try {
    await refreshExternalAccount(acc.api_url, acc.uuid);
    const fresh = externalAccount.value;
    if (fresh) void renderExternalAvatar(fresh.api_url, fresh.uuid, true);
    showToast("角色与皮肤已刷新");
  } catch (e) {
    showError(`刷新失败：${String(e)}`, true);
  }
}

/** 切换角色（refresh + selectedProfile）。 */
async function doSwitchRole(profileId: string, profileName: string) {
  const acc = externalAccount.value;
  if (!acc) return;
  showRoles.value = false;
  try {
    await switchExternalRole(acc.api_url, acc.uuid, profileId, profileName);
    showToast(`已切换到角色 ${profileName}`);
  } catch (e) {
    showError(`切换角色失败：${String(e)}`, true);
  }
}



onMounted(() => {
  window.addEventListener("click", closeHandler);
  void loadExternalAccounts();
});onUnmounted(() => {
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

/** 正版 access_token 仅约 24h 有效；过期后各入口会经后端 valid_access_token 自动静默续签。
 *  这里只把状态摆明：正常时不打扰界面，临近（<5 分钟）或已过期才标警示，
 *  消除「账号还在列表里、却各个功能失败」的困惑。 */
function credHint(acc: MsAccountInfo): string {
  const left = acc.expires_at - Date.now() / 1000;
  if (left <= 0) return "登录凭证已过期，将在使用时自动续签";
  const hours = Math.floor(left / 3600);
  if (hours >= 1) return `凭证有效，约 ${hours} 小时后到期（自动续签）`;
  const mins = Math.max(1, Math.floor(left / 60));
  return `凭证有效，约 ${mins} 分钟后到期（自动续签）`;
}

/** 凭证是否临近过期或已过期（用于显示警示标记）。 */
function credWarn(acc: MsAccountInfo): boolean {
  return acc.expires_at - Date.now() / 1000 < 300;
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
  if (acc) void removeAccount(acc.uuid);
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
        <button
          class="auth-btn"
          :class="{ active: authMode === 'thirdparty' }"
          @click="setAuthMode('thirdparty')"
        >
          <Link2 :size="16" />
          <span>第三方</span>
        </button>
      </div>
    </div>

    <div class="section s-user">
      <div
        class="user-area"
        :class="{ 'menu-open': accountUi.editMenu.visible || accountUi.accountsMenu.visible }"
        @mouseenter="onExtEnter"
        @mouseleave="onExtLeave"
      >
        <div class="user-main">
          <div class="avatar clickable" v-tooltip="'皮肤衣柜'" @click="openSkinStudio">
            <img
              v-if="avatarUrl && !avatarState.avatarError"
              :src="avatarUrl"
              alt="avatar"
              draggable="false"
              @error="avatarState.avatarError = true"
            />
            <UserRound v-else :size="34" />
            <!-- 头像加载失败（网络原因）→ hover 显示刷新重试 -->
            <button
              v-if="avatarState.avatarError && authMode !== 'offline'"
              class="avatar-retry"
              title="重新加载头像"
              @click.stop="retryAvatar"
            >
              <RefreshCw :size="14" />
            </button>
          </div>
          <div v-if="authMode === 'premium'" class="uname" :class="{ 'uname-empty': !msAccount }">
            {{ msAccount?.name || '未登录' }}
          </div>
          <div v-else-if="authMode === 'thirdparty'" class="uname" :class="{ 'uname-empty': !externalAccount }">
            <template v-if="externalAccount">
              <div class="ext-name">{{ externalAccount.name }}</div>
              <div class="ext-actions" :class="{ show: extHover || showRoles }">
                <button class="ext-icon" title="切换角色" @click.stop="showRoles = !showRoles">
                  <RotateCcw :size="12" />
                </button>
                <button class="ext-icon" title="刷新角色和皮肤" @click="refreshExternalSkin">
                  <RefreshCw :size="12" />
                </button>
                <Transition name="ext-pop">
                <div v-if="showRoles" class="ext-roles" @click.stop>
                  <template v-if="externalAccount.profiles?.length">
                    <div
                      v-for="r in externalAccount.profiles"
                      :key="r.id"
                      class="ext-role"
                      :class="{ current: r.id === externalAccount.uuid }"
                      @click="doSwitchRole(r.id, r.name)"
                    >
                      <img v-if="roleHeads[r.id]" :src="roleHeads[r.id]" class="ext-role-head" alt="" />
                      <span v-else class="ext-role-head ext-role-ph"></span>
                      <span class="ext-role-name">{{ r.name }}</span>
                    </div>
                  </template>
                  <div v-else class="ext-role-empty">暂无角色信息</div>
                </div>
                </Transition>
              </div>
            </template>
            <div v-else class="ext-status">
              <span class="ext-hint">未登录第三方账号</span>
              <button class="ext-goto" @click="goThirdPartyLogin">去登录</button>
            </div>
          </div>
          <div v-else class="offline-name">
            <input
              v-model="username"
              class="name-input"
              type="text"
              placeholder="输入用户名"
              maxlength="16"
              autocomplete="off"
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
                    <UserRound v-else :size="20" class="acct-icon" />
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
                <span class="acct-name" :title="credHint(acc)">{{ acc.name }}</span>
                <span
                  v-if="credWarn(acc)"
                  class="acct-cred-warn"
                  :title="credHint(acc)"
                >!</span>
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
.ext-login {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}
.ext-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.ext-input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  outline: none;
}
.ext-input:focus {
  border-color: var(--color-brand);
}
.ext-preset {
  flex: none;
  max-width: 180px;
  height: 28px;
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  outline: none;
}
.ext-error {
  flex: 1;
  font-size: 11px;
  color: var(--color-danger);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.acct-server {
  flex: none;
  max-width: 140px;
  font-size: 10.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ext-hint {
  font-size: 12px;
  color: var(--text-tertiary);
}
.ext-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  text-align: center;
}
.ext-actions {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  margin-top: 8px;
  position: relative;
  /* 默认隐藏（占位不塌缩），hover 用户区显示 */
  opacity: 0;
  transition: opacity 0.15s;
}
.ext-actions.show {
  opacity: 1;
}
.ext-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}
.ext-icon:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.ext-pop-enter-active,
.ext-pop-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.ext-pop-enter-from,
.ext-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.ext-act {
  padding: 3px 10px;
  font-size: 11.5px;
  border: 1px solid var(--border-strong);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}
.ext-act:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.ext-roles {
  position: absolute;
  top: calc(100% + 4px);
  left: 50%;
  transform: translateX(-50%);
  z-index: 20;
  display: flex;
  flex-direction: column;
  min-width: 120px;
  padding: 4px;
  border: 1px solid var(--border-default);
  border-radius: 8px;
  background: var(--surface-elevated);
  box-shadow: var(--shadow-md);
}
.ext-role {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: 5px;
  min-width: 170px;
}
.ext-role:hover {
  background: var(--color-brand-bg);
  color: var(--color-brand);
}
.ext-role-head {
  flex: none;
  width: 22px;
  height: 22px;
  border-radius: 4px;
  object-fit: contain;
  image-rendering: pixelated;
}
.ext-role-ph {
  background: var(--surface-hover);
}
.ext-role-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ext-role-empty {
  padding: 8px 12px;
  font-size: 11.5px;
  color: var(--text-tertiary);
  white-space: nowrap;
  text-align: center;
}
.ext-role.current {
  color: var(--color-brand);
  font-weight: 600;
}

.ext-status {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  margin-top: 4px;
}
.ext-goto {
  position: relative;
  z-index: 2;
  padding: 6px 18px;
  font-size: 12.5px;
  border: 1px solid var(--color-brand);
  border-radius: 6px;
  background: var(--color-brand);
  color: var(--text-inverse);
  cursor: pointer;
}
.ext-goto:hover {
  border-color: var(--color-brand);
}

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
  flex-wrap: nowrap;
}

.auth-btn {
  flex: 1 1 0;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 6px 6px;
  white-space: nowrap;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.auth-btn span {
  flex: none;
  white-space: nowrap;
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
  position: relative;
  width: 96px;
  height: 96px;
  border-radius: 0;
  overflow: hidden;
  color: var(--color-brand);
  display: flex;
  align-items: center;
  justify-content: center;
  /* 占位图标时无背景；方形"头+带肩"头像完整等比显示 */
  background-color: transparent;
}

.avatar svg {
  width: 100%;
  height: 100%;
}

/* 头像即皮肤衣柜入口：hover 品牌色描边提示可点（z-index 盖过 .user-area::before 热区） */
.avatar.clickable {
  z-index: 1;
  cursor: pointer;
  border-radius: var(--radius-md);
  transition: box-shadow 0.15s;
}

.avatar.clickable:hover {
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

.avatar img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  display: block;
  image-rendering: pixelated;
}

/* 头像加载失败：右下角刷新重试按钮（hover 头像区显示） */
.avatar-retry {
  position: absolute;
  right: 4px;
  bottom: 4px;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border-default);
  border-radius: 50%;
  background: var(--surface-card);
  color: var(--text-secondary);
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s, color 0.15s, border-color 0.15s;
}

.avatar:hover .avatar-retry,
.avatar-retry:focus-visible {
  opacity: 1;
}

.avatar-retry:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
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
  /* expand 目标高度 = 固定高度（共享骨架见 styles/transitions.css） */
  --expand-h: 180px;
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
  object-fit: contain;
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

/* 凭证临近过期/已过期标记：正版 token 约 24h 有效，过期后会自动续签，仅作提醒不阻断操作 */
.acct-cred-warn {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--color-danger);
  color: var(--text-inverse);
  font-size: 10px;
  font-weight: 700;
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
  visibility: hidden; /* 隐藏态彻底移出命中测试（WebView2 下 pointer-events 过渡不可靠） */
  transition: opacity 0.12s, visibility 0.12s;
}

.acct-row:hover .acct-del {
  opacity: 1;
  visibility: visible;
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
