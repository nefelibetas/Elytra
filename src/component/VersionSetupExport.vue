<script setup lang="ts">
// 版本设置-导出 tab：导出启动脚本（PCL 风格 .bat/.sh）+ 导出整合包（mods/config 等 → zip）。
// 启动脚本逻辑从原概览页迁移；整合包为版本内容打包。
import { computed } from "vue";
import { useRoute } from "vue-router";
import { CodeXml, Package } from "@lucide/vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import {
  authMode, msAccount, loadMsAccount, username as offlineUsername,
} from "../store/auth";

const route = useRoute();

/** 平台判定：Windows → .bat，其余 → .sh（模板展示与导出共用）。 */
const isWin = /windows/i.test(navigator.userAgent);
const scriptExt = isWin ? "bat" : "sh";

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

/** 导出启动脚本：保存对话框选路径 → 后端按 PCL 风格生成平台对应脚本。 */
async function exportScript() {
  try {
    const path = await save({
      defaultPath: `启动 ${versionName.value}.${scriptExt}`,
      filters: [{ name: "启动脚本", extensions: [scriptExt] }],
    });
    if (!path) return; // 用户取消

    // 收集当前登录态，生成可直接启动的命令
    // 注意：auth store 的 AuthMode 是 "premium" | "offline"，后端命令参数约定是 "online" | "offline"
    let username = "";
    let accessToken = "";
    let uuid = "";
    let xuid = "";
    if (authMode.value === "premium") {
      if (!msAccount.value) await loadMsAccount();
      const acc = msAccount.value;
      if (!acc) {
        showToast("未登录正版账号，请先在启动页登录");
        return;
      }
      username = acc.name;
      accessToken = acc.access_token;
      uuid = acc.uuid;
      xuid = acc.xuid;
    } else {
      // 优先当前输入框用户名（未启动过也正确），回退上次保存，最后 Steve
      username =
        offlineUsername.value.trim() ||
        (await invoke<string>("get_last_offline_username")) ||
        "Steve";
    }

    await invoke("export_launch_script", {
      savePath: path,
      versionDirName: versionName.value,
      authMode: authMode.value === "premium" ? "online" : "offline",
      username,
      accessToken,
      uuid,
      xuid,
    });
    showToast("启动脚本已导出");
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 导出整合包：把版本的 mods/config/shaderpacks/resourcepacks 打包为 zip。 */
async function exportModpack() {
  try {
    const path = await save({
      defaultPath: `${versionName.value}.zip`,
      filters: [{ name: "整合包", extensions: ["zip"] }],
    });
    if (!path) return; // 用户取消
    await invoke("export_modpack", {
      versionName: versionName.value,
      savePath: path,
    });
    showToast("整合包已导出");
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vs-page">
    <!-- 导出启动脚本 -->
    <div class="setup-card">
      <div class="setup-card-title">导出启动脚本</div>
      <div class="setup-desc">
        生成可直接双击启动游戏的 {{ scriptExt.toUpperCase() }} 脚本，
        携带当前版本与登录信息，无需启动器即可启动。
      </div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="exportScript">
          <CodeXml :size="15" />
          <span>导出启动脚本</span>
        </button>
      </div>
    </div>

    <!-- 导出整合包 -->
    <div class="setup-card">
      <div class="setup-card-title">导出整合包</div>
      <div class="setup-desc">
        将当前版本的 mods / config / shaderpacks / resourcepacks 打包为 ZIP，
        可用于分享或备份（不含游戏本体与存档）。
      </div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="exportModpack">
          <Package :size="15" />
          <span>导出整合包</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 导出页：占满右侧剩余空间，内容超出自身滚动 */
.vs-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

/* 卡片 */
.setup-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  max-width: 560px;
}

.setup-card + .setup-card {
  margin-top: 12px;
}

.setup-card-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  padding: 12px 16px 4px;
  letter-spacing: 0.5px;
}

.setup-desc {
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--text-secondary);
  padding: 4px 16px 0;
}

.setup-btn-row {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 10px 16px 14px;
  flex-wrap: wrap;
}

.setup-btn {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  cursor: pointer;
  transition: all 0.15s;
}

.setup-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}
</style>
