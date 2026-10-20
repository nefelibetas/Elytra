<script setup lang="ts">
// Java 运行时卡片：选择器 + 扫描/自定义目录 + 下载缺失 JDK + 高级项（JVM 参数/GC/复选框）
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { RefreshCw, FolderPlus, Download } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import {
  jdkList, autoMode, activeJdk, loading, jdkError,
  refreshJdks, selectCustomDir, selectAuto, selectJdk,
  installJdk, loadConfig,
} from "../../store/config/jdkOptions";
import {
  loJvmArgs, loGameArgs, loGc, loDisableWrapper, loDisableAgent, loHighGpu,
  gcOptions, saveLaunchOptions,
} from "../../store/config/launchOptions";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import CheckToggle from "../common/CheckToggle.vue";

onMounted(loadConfig);

const { t } = useI18n();

/** 下载目标 Java 主版本（Mojang runtime 组件覆盖 8/17/21/25）。 */
const downloadMajor = ref(17);

const jdkDownloadOptions: SelOption[] = [
  { value: "8", label: "Java 8" },
  { value: "17", label: "Java 17" },
  { value: "21", label: "Java 21" },
  { value: "25", label: "Java 25" },
];

async function handleInstallJdk() {
  try {
    // 提交任务中心（成功/完成 toast 由 store 内处理）
    await installJdk(downloadMajor.value);
  } catch (e) {
    showToast(errMsg(e));
  }

}
/** JDK 选择下拉：自动 + 扫描到的 JDK，path 作唯一 value。 */
const AUTO_VALUE = "__auto__";

const jdkOptions = computed<SelOption[]>(() => [
  { value: AUTO_VALUE, label: t("settings.jdk.autoLabel"), desc: t("settings.jdk.autoDesc") },
  ...jdkList.value.map(j => ({
    value: j.path,
    label: j.version,
    desc: t("settings.jdk.optionDesc", { vendor: j.vendor, bits: j.bits, path: j.path }),
  })),
]);

const gcSelectOptions = computed<SelOption[]>(() =>
  gcOptions.map(g => ({ value: g.value, label: t(g.labelKey) })),
);

/** 当前选中值：自动模式固定 AUTO_VALUE；手动为 activeJdk 的 path。 */
const jdkValue = computed(() => (autoMode.value ? AUTO_VALUE : activeJdk.value?.path ?? ""));

/** 下拉选择：切自动或按 path 选中 JDK（store 内保存配置）。 */
function onJdkPick(value: string) {
  if (value === AUTO_VALUE) {
    selectAuto();
    return;
  }
  const hit = jdkList.value.find(j => j.path === value);
  if (hit) selectJdk(hit);
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.jdk.title") }}</h3>
      <span class="stg-card-sub">{{ t("settings.jdk.subtitle") }}</span>
    </div>

    <div class="jdk-toolbar">
      <div class="jdk-toolbar-row">
        <div class="jdk-picker-wrap">
          <DropSelect
            :model-value="jdkValue"
            :options="jdkOptions"
            width="100%"
            @update:model-value="onJdkPick"
          />
        </div>

        <button class="tool-btn" :disabled="loading" @click="refreshJdks">
          <RefreshCw :size="14" />
          {{ t("settings.jdk.refresh") }}
        </button>
        <button class="tool-btn" :disabled="loading" @click="selectCustomDir">
          <FolderPlus :size="14" />
          {{ t("settings.jdk.customDir") }}
        </button>
      </div>

      <div class="jdk-dl-group">
        <span class="jdk-dl-label">{{ t("settings.jdk.downloadMissing") }}</span>
        <div class="jdk-dl-controls">
          <DropSelect
            :model-value="String(downloadMajor)"
            :options="jdkDownloadOptions"
            width="88px"
            @update:model-value="downloadMajor = Number($event)"
          />
          <button class="tool-btn jdk-dl-btn" :disabled="loading" @click="handleInstallJdk">
            <Download :size="14" />
            {{ loading ? t("settings.jdk.downloading") : t("settings.jdk.download") }}
          </button>
        </div>
      </div>
    </div>

    <LoadingSpinner v-if="loading" :text="t('settings.jdk.processing')" />
    <div v-else-if="jdkError" class="err">{{ jdkError }}</div>

    <div class="jdk-divider"></div>

    <!-- Java 高级项 -->
    <div class="launch-opts jdk-opts">
      <div class="lo-row lo-top">
        <label class="lo-label" for="lo-jvm">{{ t("settings.jdk.jvmArgs") }}</label>
        <textarea
          id="lo-jvm"
          v-model="loJvmArgs"
          class="lo-textarea"
          rows="4"
          wrap="off"
          :placeholder="t('settings.jdk.jvmArgsPlaceholder')"
          spellcheck="false"
          @change="saveLaunchOptions"
          autocomplete="off"
        ></textarea>
      </div>

      <div class="lo-row lo-top">
        <label class="lo-label" for="lo-game">{{ t("settings.jdk.gameArgs") }}</label>
        <textarea
          id="lo-game"
          v-model="loGameArgs"
          class="lo-textarea"
          rows="2"
          wrap="off"
          :placeholder="t('settings.jdk.gameArgsPlaceholder')"
          spellcheck="false"
          @change="saveLaunchOptions"
          autocomplete="off"
        ></textarea>
      </div>

      <div class="lo-row">
        <label class="lo-label" for="lo-gc">{{ t("settings.jdk.memoryManagement") }}</label>
        <DropSelect
          id="lo-gc"
          :model-value="loGc"
          :options="gcSelectOptions"
          width="180px"
          @update:model-value="loGc = $event; saveLaunchOptions()"
        />
      </div>

      <div class="lo-checks">
        <CheckToggle v-model="loDisableWrapper" @change="saveLaunchOptions">{{ t("settings.jdk.disableWrapper") }}</CheckToggle>
        <CheckToggle v-model="loDisableAgent" @change="saveLaunchOptions">{{ t("settings.jdk.disableAgent") }}</CheckToggle>
        <CheckToggle v-model="loHighGpu" @change="saveLaunchOptions">{{ t("settings.jdk.highGpu") }}</CheckToggle>
      </div>
    </div>
  </div>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
/* ===== JDK 工具栏 ===== */
.jdk-toolbar {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.jdk-toolbar-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.jdk-dl-group {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 10px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

.jdk-dl-controls {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}

.jdk-dl-label {
  flex: 1;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.jdk-dl-btn {
  padding: 7px 14px;
}

.jdk-picker-wrap {
  flex: 1;
  min-width: 0;
  position: relative;
}


.jdk-divider {
  height: 1px;
  background: var(--border-default);
  margin: 16px 0;
}
</style>
