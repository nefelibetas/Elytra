<script setup lang="ts">
// 版本设置-导出 tab：启动脚本（PCL 风格 .bat/.sh）+ 整合包（MRPACK zip）。
import { computed } from "vue";
import { useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { CodeXml, Package } from "@lucide/vue";
import { loadLastUsername } from "../../store/auth/auth";
import { exportLaunchScript, exportModpack as exportModpackCommand } from "../../store/task/versionSetup";
import { save } from "@tauri-apps/plugin-dialog";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import {
  authMode, msAccount, loadMsAccount, username as offlineUsername,
} from "../../store/auth/auth";

const route = useRoute();
const { t } = useI18n();

/** 平台判定：Windows → .bat，其余 → .sh（模板展示与导出共用）。 */
const isWin = /windows/i.test(navigator.userAgent);
const scriptExt = isWin ? "bat" : "sh";

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

async function exportScript() {
  try {
    const path = await save({
      defaultPath: t("settings.version.setup.export.scriptFileName", { version: versionName.value, ext: scriptExt }),
      filters: [{ name: t("settings.version.setup.export.scriptFilter"), extensions: [scriptExt] }],
    });
    if (!path) return; // 用户取消

    // auth store 的 AuthMode 为 premium|offline；后端命令参数用 online|offline
    let username = "";
    let accessToken = "";
    let uuid = "";
    let xuid = "";
    if (authMode.value === "premium") {
      if (!msAccount.value) await loadMsAccount();
      const acc = msAccount.value;
      if (!acc) {
        showToast(t("settings.version.setup.export.notLoggedIn"));
        return;
      }
      username = acc.name;
      accessToken = acc.access_token;
      uuid = acc.uuid;
      xuid = acc.xuid;
    } else {
      username =
        offlineUsername.value.trim() ||
        (await loadLastUsername()) ||
        "Steve";
    }

    await exportLaunchScript({
      savePath: path,
      versionDirName: versionName.value,
      authMode: authMode.value === "premium" ? "online" : "offline",
      username,
      accessToken,
      uuid,
      xuid,
    });
    showToast(t("settings.version.setup.export.scriptExported"));
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 导出整合包：mods/config/shaderpacks/resourcepacks/datapacks 打包为标准 MRPACK（Modrinth v1，overrides 物理打包 + 空 files，存档不打包）。 */
async function exportModpack() {
  try {
    const path = await save({
      defaultPath: `${versionName.value}.zip`,
      filters: [{ name: t("settings.version.setup.export.modpackFilter"), extensions: ["zip"] }],
    });
    if (!path) return; // 用户取消
    await exportModpackCommand(versionName.value, path);
    showToast(t("settings.version.setup.export.modpackExported"));
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vs-page">
    <div class="setup-card">
      <div class="setup-card-title">{{ t("settings.version.setup.export.scriptTitle") }}</div>
      <div class="setup-desc">
        {{ t("settings.version.setup.export.scriptDesc", { ext: scriptExt.toUpperCase() }) }}
      </div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="exportScript">
          <CodeXml :size="15" />
          <span>{{ t("settings.version.setup.export.scriptButton") }}</span>
        </button>
      </div>
    </div>

    <div class="setup-card">
      <div class="setup-card-title">{{ t("settings.version.setup.export.modpackTitle") }}</div>
      <div class="setup-desc">
        {{ t("settings.version.setup.export.modpackDesc") }}
      </div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="exportModpack">
          <Package :size="15" />
          <span>{{ t("settings.version.setup.export.modpackButton") }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.vs-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.setup-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
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
