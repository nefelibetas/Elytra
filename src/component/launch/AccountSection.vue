<script setup lang="ts">
// 账号区：正版/离线切换 + 用户区；菜单 Teleport 到 body，window click 关闭（挂载期注册）。
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ShieldCheck, User, UserRound, Pencil, LogOut, Users, ChevronDown, Link2, RotateCcw, RefreshCw } from "@lucide/vue";
import {
  authMode, msAccount, msAccounts, offlineAccounts, username, setAuthMode, msLoginState,
  startMsLogin, cancelMsLogin, switchMsAccount, removeMsAccount,
  saveOfflineAccount, removeOfflineAccount,
  externalAccount, loadExternalAccounts, switchExternalRole, refreshExternalAccount,
  type MsAccountInfo,
} from "../../store/auth/auth";
import { avatarState, scheduleOfflineRender, renderAccountHead, renderOfflineHead, accountHeads, offlineHeads, renderSkinHead3D, renderExternalAvatar, renderPremiumAvatar } from "../../store/auth/avatar";
import { accountUi } from "../../store/auth/accountUi";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { invoke } from "@tauri-apps/api/core";
import { settingsTab } from "../../store/app/settingsTab";
import { showError } from "../../store/app/alert";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useRouter } from "vue-router";
import MenuPopover from "../common/MenuPopover.vue";

const router = useRouter();
const { t } = useI18n();

const avatarUrl = computed(() =>
  authMode.value === "premium"
    ? avatarState.premiumAvatar
    : authMode.value === "external"
      ? avatarState.externalAvatar
      : avatarState.offlineAvatar,
);

/** 头像加载失败后手动重试（force 绕过去重缓存）。 */
function retryAvatar() {
  avatarState.avatarError = false;
  if (authMode.value === "premium") {
    void renderPremiumAvatar(msAccount.value?.uuid ?? "", true);
  } else if (authMode.value === "external") {
    void renderExternalAvatar(externalAccount.value?.api_url ?? "", externalAccount.value?.uuid ?? "", true);
  }
}

/** 用户名输入防抖 1.337s 后刷新离线头像 */
watch(username, (v) => scheduleOfflineRender(v));

// 用户名输入防抖 1s 存历史（直接点启动时 launch.ts 兜底保存）
let offlineSaveTimer: number | undefined;
watch(username, (v) => {
  window.clearTimeout(offlineSaveTimer);
  if (v.trim()) {
    offlineSaveTimer = window.setTimeout(() => saveOfflineAccount(v.trim()), 1000);
  }
});

const closeHandler = () => {
  accountUi.closeMenus();
  showRoles.value = false;
};

function goExternalLogin() {
  settingsTab.value = "external";
  void router.push("/settings");
}
/** 可进启动器衣柜：仅已登录的正版账号；准入须与衣柜页一致，否则点进去即被弹回。 */
const canOpenSkinStudio = computed(() => authMode.value === "premium" && !!msAccount.value);
const canOpenSiteCloset = computed(() => authMode.value === "external" && !!externalAccount.value);

/** 站点衣柜地址：由 API 地址推 origin（Blessing Skin 路径固定 `/user/closet`）。 */
const siteClosetUrl = computed(() => {
  const api = externalAccount.value?.api_url;
  if (!api) return "";
  try {
    return `${new URL(api).origin}/user/closet`;
  } catch {
    return "";
  }
});

const avatarTip = computed(() => {
  if (canOpenSiteCloset.value) return t("launch.auth.siteCloset");
  if (canOpenSkinStudio.value) return t("launch.auth.wardrobe");
  if (authMode.value === "external") return t("launch.auth.externalTip");
  return authMode.value === "offline"
    ? t("launch.auth.offlineTip")
    : t("launch.auth.premiumTip");
});

function openSkinStudio() {
  accountUi.closeMenus();
  if (canOpenSiteCloset.value) {
    void openUrl(siteClosetUrl.value).catch((e) =>
      showError(t("launch.auth.openClosetFailed", { error: errMsg(e) }), true),
    );
    return;
  }
  if (!canOpenSkinStudio.value) return;
  void router.push("/skin-studio");
}
const showRoles = ref(false);
/** 用户区 hover 状态（模板 ref 解包不写回，用函数显式改 .value）。 */
const extHover = ref(false);
function onExtEnter() { extHover.value = true; }
function onExtLeave() { extHover.value = false; }
/** 角色头像缓存（profile_id → dataURL），面板打开时懒加载。 */
const roleHeads = reactive<Record<string, string>>({});
async function loadRoleHead(apiUrl: string, roleId: string) {
  if (roleHeads[roleId]) return;
  try {
    const b64 = await invoke<string>("fetch_external_skin", { apiUrl, uuid: roleId });
    roleHeads[roleId] = await renderSkinHead3D(b64);
  } catch {
  }
}
// 打开面板：先 refresh 账号（同步最新角色列表），再懒加载角色头像
watch(showRoles, async (open) => {
  if (!open) return;
  const acc = externalAccount.value;
  if (!acc) return;
  try {
    await refreshExternalAccount(acc.api_url, acc.uuid);
  } catch (e) {
    showError(t("launch.auth.refreshRolesFailed", { error: errMsg(e) }), true);
    showRoles.value = false;
    return;
  }
  const fresh = externalAccount.value;
  if (!fresh) return;
  for (const r of fresh.profiles ?? []) {
    void loadRoleHead(fresh.api_url, r.id);
  }
});

async function refreshExternalSkin() {
  const acc = externalAccount.value;
  if (!acc) return;
  try {
    await refreshExternalAccount(acc.api_url, acc.uuid);
    const fresh = externalAccount.value;
    if (fresh) void renderExternalAvatar(fresh.api_url, fresh.uuid, true);
    showToast(t("launch.auth.refreshed"));
  } catch (e) {
    showError(t("launch.auth.refreshFailed", { error: errMsg(e) }), true);
  }
}

async function doSwitchRole(profileId: string, profileName: string) {
  const acc = externalAccount.value;
  if (!acc) return;
  showRoles.value = false;
  try {
    await switchExternalRole(acc.api_url, acc.uuid, profileId, profileName);
    showToast(t("launch.auth.switchedRole", { name: profileName }));
  } catch (e) {
    showError(t("launch.auth.switchRoleFailed", { error: errMsg(e) }), true);
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
  const seq = ++headLoadSeq; // 丢弃过期结果（组件重建/重复打开）
  for (const acc of msAccounts.value) {
    renderAccountHead(acc.uuid).then((h) => {
      if (seq === headLoadSeq && h) accountHeads[acc.uuid] = h;
    });
  }
}

// ── 离线历史头像（Steve/Alex 默认皮肤头部，哈希决定模型，与主头像一致）──

async function ensureOfflineHeads() {
  for (const name of offlineAccounts.value) {
    if (offlineHeads[name]) continue;
    renderOfflineHead(name).then((h) => {
      if (h) offlineHeads[name] = h;
    });
  }
}

/** 正版 access_token 约 24h 有效，过期后经后端 valid_access_token 自动续签；<5 分钟或已过期才标警示。 */
function credHint(acc: MsAccountInfo): string {
  const left = acc.expires_at - Date.now() / 1000;
  if (left <= 0) return t("launch.auth.tokenExpired");
  const hours = Math.floor(left / 3600);
  if (hours >= 1) return t("launch.auth.tokenHours", { hours });
  const mins = Math.max(1, Math.floor(left / 60));
  return t("launch.auth.tokenMinutes", { mins });
}

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
    showError(t("launch.auth.switchAccountFailed", { error: errMsg(e) }), true);
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
    showError(t("launch.auth.removeAccountFailed", { error: errMsg(e) }), true);
  }
  accountUi.closeAccountsMenu();
}

/** 移除当前账号（登出按钮：从列表移除，而非清空全部）。 */
function removeCurrentMsAccount() {
  const acc = msAccount.value;
  if (acc) void removeAccount(acc.uuid);
}

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
          <span>{{ t("launch.auth.premium") }}</span>
        </button>
        <button
          class="auth-btn"
          :class="{ active: authMode === 'offline' }"
          @click="setAuthMode('offline')"
        >
          <User :size="16" />
          <span>{{ t("launch.auth.offline") }}</span>
        </button>
        <button
          class="auth-btn"
          :class="{ active: authMode === 'external' }"
          @click="setAuthMode('external')"
        >
          <Link2 :size="16" />
          <span>{{ t("launch.auth.external") }}</span>
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
          <div
            class="avatar"
            :class="{ clickable: canOpenSkinStudio || canOpenSiteCloset }"
            v-tooltip="avatarTip"
            @click="openSkinStudio"
          >
            <img
              v-if="avatarUrl && !avatarState.avatarError"
              :src="avatarUrl"
              alt="avatar"
              draggable="false"
              @error="avatarState.avatarError = true"
            />
            <UserRound v-else :size="34" />
            <!-- 头像加载失败 → hover 显示重试 -->
            <button
              v-if="avatarState.avatarError && authMode !== 'offline'"
              class="avatar-retry"
              :title="t('launch.auth.reloadAvatar')"
              @click.stop="retryAvatar"
            >
              <RefreshCw :size="14" />
            </button>
          </div>
          <div v-if="authMode === 'premium'" class="uname" :class="{ 'uname-empty': !msAccount }">
            {{ msAccount?.name || t("launch.auth.notLoggedIn") }}
          </div>
          <div v-else-if="authMode === 'external'" class="uname" :class="{ 'uname-empty': !externalAccount }">
            <template v-if="externalAccount">
              <div class="ext-name">{{ externalAccount.name }}</div>
              <div class="ext-actions" :class="{ show: extHover || showRoles }">
                <button class="ext-icon" :title="t('launch.auth.switchRole')" @click.stop="showRoles = !showRoles">
                  <RotateCcw :size="12" />
                </button>
                <button class="ext-icon" :title="t('launch.auth.refreshRole')" @click="refreshExternalSkin">
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
                  <div v-else class="ext-role-empty">{{ t("launch.auth.noRoles") }}</div>
                </div>
                </Transition>
              </div>
            </template>
            <div v-else class="ext-status">
              <span class="ext-hint">{{ t("launch.auth.externalNotLoggedIn") }}</span>
              <button class="ext-goto" @click="goExternalLogin">{{ t("launch.auth.loginExternal") }}</button>
            </div>
          </div>
          <div v-else class="offline-name">
            <input
              v-model="username"
              class="name-input"
              type="text"
              :placeholder="t('launch.auth.usernamePlaceholder')"
              maxlength="16"
              autocomplete="off"
            />
            <button
              class="icon-btn hist-btn"
              :class="{ open: accountUi.offlineMenu.visible }"
              v-tooltip="t('launch.auth.history')"
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
                      :title="t('launch.auth.removeUsername')"
                      @click.stop="removeOfflineAccount(name)"
                    >×</button>
                  </div>
                </template>
                <div v-else class="menu-item acct-empty">{{ t("launch.auth.noOfflineNames") }}</div>
              </div>
            </Transition>
          </div>
        </div>

        <div v-if="authMode === 'premium'" class="user-actions">
          <div v-if="msLoginState === 'waiting'" class="ms-login-waiting">
            <span class="ls-spin"></span>
            <span>{{ t("launch.auth.completeInBrowser") }}</span>
            <button class="ms-cancel" @click="cancelMsLogin">{{ t("launch.auth.cancel") }}</button>
          </div>
          <template v-else-if="msAccount">
            <button
              class="icon-btn"
              v-tooltip="t('launch.auth.accountList')"
              @click.stop="accountUi.openAccountsMenu($event); ensureAccountHeads()"
            >
              <Users :size="14" />
            </button>
            <button class="icon-btn" v-tooltip="t('launch.auth.editInfo')" @click.stop="accountUi.openEditMenu($event)">
              <Pencil :size="14" />
            </button>
            <!-- 登出：仅从列表移除当前账号 -->
            <button class="icon-btn" v-tooltip="t('launch.auth.removeCurrent')" @click="removeCurrentMsAccount">
              <LogOut :size="14" />
            </button>
          </template>
          <button v-else class="act-btn" @click="startMsLogin">
            <ShieldCheck :size="14" />
            <span>{{ t("launch.auth.loginMicrosoft") }}</span>
          </button>
        </div>

        <MenuPopover
          :visible="accountUi.editMenu.visible"
          :x="accountUi.editMenu.x"
          :y="accountUi.editMenu.y"
        >
          <div class="menu-item" @click="accountUi.openPasswordPage">{{ t("launch.auth.changePassword") }}</div>
          <div class="menu-item" @click="accountUi.openNameChangePage">{{ t("launch.auth.changeName") }}</div>
        </MenuPopover>

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
                  :title="t('launch.auth.removeAccount')"
                  @click.stop="removeAccount(acc.uuid)"
                >×</button>
              </div>
            </template>
            <div v-else class="menu-item acct-empty">{{ t("launch.auth.noAccounts") }}</div>
            <div class="menu-sep"></div>
            <div class="menu-item" @click="accountUi.closeAccountsMenu(); startMsLogin()">{{ t("launch.auth.loginNew") }}</div>
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

/* 扩大 hover 热区，不占布局、不挡点击 */
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
  /* 占位图标无背景；头像等比完整显示 */
  background-color: transparent;
}

.avatar svg {
  width: 100%;
  height: 100%;
}

/* 衣柜入口：hover 描边提示可点（z-index 盖过热区） */
.avatar.clickable {
  z-index: 1;
  cursor: pointer;
  border-radius: var(--radius-md);
  transition: box-shadow 0.15s;
}

.avatar.clickable:hover {
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

/* 离线模式衣柜无入口：由 openSkinStudio() 内部守卫拦住，此处不给样式。 */

.avatar img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  display: block;
  image-rendering: pixelated;
}

/* 头像加载失败：右下角重试按钮（hover 头像区显示） */
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
  /* 盖过热区，保证可点击聚焦 */
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

/* 箭头按钮：双类提特异性以覆盖 icon-btn 边框 */
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

/* 历史用户名面板：锚定输入框下方，固定高度超出滚动 */
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
  /* expand 目标高度（共享骨架见 styles/transitions.css） */
  --expand-h: 180px;
}

/* premium hover actions：始终占位，仅切换透明度；hover / :focus-within / 菜单展开任一显示。 */
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

/* 图标操作按钮 */
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

/* 账号列表菜单行 + 删除按钮（菜单壳样式在 MenuPopover） */

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

/* 账号列表 2D 皮肤头像（8×8 头部放大） */
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

/* 凭证临近/已过期标记（token 约 24h，自动续签，仅提醒不阻断） */
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
  visibility: hidden; /* 隐藏态移出命中测试（WebView2 pointer-events 过渡不可靠） */
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

/* 双类提特异性：压过 MenuPopover 的 :slotted(.menu-item:hover) */
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
