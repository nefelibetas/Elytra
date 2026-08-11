// JDK 配置 store：扫描/选择/保存 Java 运行时。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useConfigSection } from "../utils/useConfigSection";

export interface JdkInfo {
  path: string;
  version: string;
  vendor: string;
  bits: number;
}

type JdkSelection = { mode: "auto" } | { mode: "manual"; path: string };

interface JdkConfig {
  jdk_selection: JdkSelection;
  discovered_jdks: JdkInfo[];
}

const section = useConfigSection();
export const jdkError = section.error;

export const jdkList = ref<JdkInfo[]>([]);
export const autoMode = ref(true);
export const activeJdk = ref<JdkInfo | null>(null);
export const loading = ref(false);
export const pickerOpen = ref(false);

export async function loadConfig() {
  try {
    const cfg = await invoke<JdkConfig>("get_jdk_config");
    const sel = cfg.jdk_selection;
    jdkList.value = cfg.discovered_jdks ?? [];
    autoMode.value = sel.mode === "auto";
    activeJdk.value =
      sel.mode === "manual"
        ? cfg.discovered_jdks.find((e) => e.path === sel.path) ?? null
        : null;
  } catch {
    // 忽略
  }
}

export async function saveConfig() {
  await section.run(async () => {
    await invoke("save_jdk_config", {
      config: {
        jdk_selection: autoMode.value
          ? { mode: "auto" }
          : { mode: "manual", path: activeJdk.value?.path ?? "" },
        discovered_jdks: jdkList.value,
      },
    });
  });
}

export async function refreshJdks() {
  loading.value = true;
  await section.run(async () => {
    jdkList.value = await invoke<JdkInfo[]>("auto_scan_jdk");
  });
  loading.value = false;
}

export async function selectCustomDir() {
  loading.value = true;
  await section.run(async () => {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;
    jdkList.value = await invoke<JdkInfo[]>("scan_custom_jdk", { path: dir });
  });
  loading.value = false;
}

export function selectAuto() {
  autoMode.value = true;
  activeJdk.value = null;
  pickerOpen.value = false;
  saveConfig();
}

export function selectJdk(env: JdkInfo) {
  autoMode.value = false;
  activeJdk.value = env;
  pickerOpen.value = false;
  saveConfig();
}

export const pickerLabel = computed(() => {
  if (autoMode.value) return "自动选择";
  return activeJdk.value ? activeJdk.value.version : "未选择";
});

export const pickerSub = computed(() => {
  if (autoMode.value) return "根据游戏版本需求自动匹配";
  return activeJdk.value?.path ?? "";
});
