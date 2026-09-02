<script setup lang="ts">
// 联机设置卡片：仅配置（协调服务器 / 自定义广播名）。
// 会话操作（开启/加入/停止）已移至"工具 → 联机"（OnlineTool.vue），
// store 为模块级单例，两处共享状态互不冲突。
import { onMounted, ref } from "vue";
import { Save, ExternalLink } from "@lucide/vue";
import {
  onlinePublicNodes, onlineAdvertiseMotd, onlineNickname,
  loadOnlineConfig, saveOnlineConfig, saveAdvertiseMotd, saveNickname,
} from "../../store/online";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

onMounted(() => {
  void loadOnlineConfig();
});

/** 协调服务器保存中。 */
const savingConfig = ref(false);
/** 广播名保存中。 */
const savingMotd = ref(false);
/** 昵称保存中。 */
const savingNickname = ref(false);

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

async function handleSaveMotd() {
  savingMotd.value = true;
  try {
    await saveAdvertiseMotd();
    showToast("联机广播名已保存");
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
    showToast("联机昵称已保存");
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
      <h3>联机设置</h3>
      <div class="stg-card-sub">
        会话操作（开启/加入联机）在 工具 → 联机；此处配置协调服务器与广播名
      </div>
    </div>

    <!-- 协调服务器（跨网联机） -->
    <div class="online-sec">
      <div class="online-sec-title">协调服务器（跨网联机）</div>
      <p class="online-desc">跨网联机需连同一个协调服务器才能互相发现。默认内置社区节点（含陶瓦 Terracotta 同款节点，与其他 Scaffolding 客户端互通），也可填自己的地址（每行一个，如 <span class="online-mono">tcp://1.2.3.4:11010</span>），留空仅支持同一局域网。</p>
      <a
        class="online-link"
        href="https://easytier.cn"
        target="_blank"
        rel="noopener"
      >
        如何搭建/寻找协调服务器（EasyTier 教程）
        <ExternalLink :size="12" />
      </a>
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

    <!-- 自定义联机广播名 -->
    <div class="online-sec">
      <div class="online-sec-title">联机广播名（可选）</div>
      <p class="online-desc">游戏内"局域网"标签页显示的服务器名，支持 § 颜色代码（如 <span class="online-mono">§6Elytra §f联机</span>）。留空 = 自动使用主机世界名；可用"工具 → MOTD 生成器"生成。</p>
      <input
        v-model="onlineAdvertiseMotd"
        class="online-input"
        placeholder="留空自动（host 世界名），支持 § 格式码"
      />
      <div class="online-actions">
        <button class="online-btn" :disabled="savingMotd" @click="handleSaveMotd">
          <Save :size="14" />
          {{ savingMotd ? "保存中..." : "保存" }}
        </button>
      </div>
    </div>

    <!-- 联机昵称 -->
    <div class="online-sec">
      <div class="online-sec-title">联机昵称（可选）</div>
      <p class="online-desc">联机时向其他玩家展示的名字（Scaffolding 协议 <span class="online-mono">c:player_ping</span> 的 name 字段）。留空 = 系统用户名。</p>
      <input
        v-model="onlineNickname"
        class="online-input"
        placeholder="留空使用系统用户名"
      />
      <div class="online-actions">
        <button class="online-btn" :disabled="savingNickname" @click="handleSaveNickname">
          <Save :size="14" />
          {{ savingNickname ? "保存中..." : "保存" }}
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
