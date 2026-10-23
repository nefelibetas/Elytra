<script setup lang="ts">
// 联机设置卡片：仅配置（协调服务器/广播名/昵称）。
// 会话操作在启动页右栏联机条（OnlinePanel.vue）；store 为模块级单例，两处共享状态。
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { Save, ExternalLink } from "@lucide/vue";
import {
  onlinePublicNodes, onlineAdvertiseMotd, onlineNickname,
  loadOnlineConfig, saveOnlineConfig, saveAdvertiseMotd, saveNickname,
} from "../../store/config/online";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

const { t } = useI18n();

onMounted(() => {
  void loadOnlineConfig();
});

const savingConfig = ref(false);
const savingMotd = ref(false);
const savingNickname = ref(false);

async function handleSaveConfig() {
  savingConfig.value = true;
  try {
    await saveOnlineConfig();
    showToast(t("settings.online.savedConfig"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingConfig.value = false;
  }
}

async function handleSaveMotd() {
  savingMotd.value = true;
  try {
    await saveAdvertiseMotd();
    showToast(t("settings.online.savedMotd"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingMotd.value = false;
  }
}

async function handleSaveNickname() {
  savingNickname.value = true;
  try {
    await saveNickname();
    showToast(t("settings.online.savedNickname"));
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    savingNickname.value = false;
  }
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.online.title") }}</h3>
      <div class="stg-card-sub">
        {{ t("settings.online.subtitle") }}
      </div>
    </div>

    <!-- 协调服务器（跨网联机） -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.coordTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.coordDesc", { example: t("settings.online.coordExample") }) }}</p>
      <a
        class="online-link"
        href="https://easytier.cn"
        target="_blank"
        rel="noopener"
      >
        {{ t("settings.online.coordGuide") }}
        <ExternalLink :size="12" />
      </a>
      <textarea
        v-model="onlinePublicNodes"
        class="online-textarea"
        rows="3"
        :placeholder="t('settings.online.nodesPlaceholder')"
        autocomplete="off"
      ></textarea>
      <div class="online-actions">
        <button class="online-btn" :disabled="savingConfig" @click="handleSaveConfig">
          <Save :size="14" />
          {{ savingConfig ? t("settings.online.saving") : t("common.action.save") }}
        </button>
      </div>
    </div>

    <!-- 自定义联机广播名 -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.motdTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.motdDesc", { example: t("settings.online.motdExample") }) }}</p>
      <input
        v-model="onlineAdvertiseMotd"
        class="online-input"
        :placeholder="t('settings.online.motdPlaceholder')"
        autocomplete="off"
      />
      <div class="online-actions">
        <button class="online-btn" :disabled="savingMotd" @click="handleSaveMotd">
          <Save :size="14" />
          {{ savingMotd ? t("settings.online.saving") : t("common.action.save") }}
        </button>
      </div>
    </div>

    <!-- 联机昵称 -->
    <div class="online-sec">
      <div class="online-sec-title">{{ t("settings.online.nicknameTitle") }}</div>
      <p class="online-desc">{{ t("settings.online.nicknameDesc") }}</p>
      <input
        v-model="onlineNickname"
        class="online-input"
        :placeholder="t('settings.online.nicknamePlaceholder')"
        autocomplete="off"
      />
      <div class="online-actions">
        <button class="online-btn" :disabled="savingNickname" @click="handleSaveNickname">
          <Save :size="14" />
          {{ savingNickname ? t("settings.online.saving") : t("common.action.save") }}
        </button>
      </div>
    </div>
  </div>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
.online-sec {
  padding: 12px 0;
  border-top: 1px solid var(--border-default);
}
.online-sec:first-of-type {
  border-top: none;
  padding-top: 0;
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
  display: inline-flex;
  align-items: center;
  gap: 4px;
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
.online-input {
  width: 100%;
  box-sizing: border-box;
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
</style>
