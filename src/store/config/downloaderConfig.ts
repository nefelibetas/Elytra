// 下载设置 store：下载源模式、线程数、速度限制。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useConfigSection } from "../../utils/useConfigSection";
import type { DownloaderConfig } from "../../types/config/DownloaderConfig";

/** 下拉选项文案存 key，渲染侧再翻译（避免模块级 t()）。 */
export interface OptionKeys {
  value: string;
  labelKey: string;
  descKey: string;
}

export const sourceModeOptions: OptionKeys[] = [
  { value: "auto", labelKey: "settings.download.sourceMode.auto.label", descKey: "settings.download.sourceMode.auto.desc" },
  { value: "officialOnly", labelKey: "settings.download.sourceMode.officialOnly.label", descKey: "settings.download.sourceMode.officialOnly.desc" },
  { value: "mirrorOnly", labelKey: "settings.download.sourceMode.mirrorOnly.label", descKey: "settings.download.sourceMode.mirrorOnly.desc" },
];

// 网络代理模式：auto 检测系统代理（Win 注册表 / Linux 环境变量或 gsettings）；system 强制系统代理；direct 强制直连（TUN 由网络层接管）
export const networkModeOptions: OptionKeys[] = [
  { value: "auto", labelKey: "settings.download.networkMode.auto.label", descKey: "settings.download.networkMode.auto.desc" },
  { value: "system", labelKey: "settings.download.networkMode.system.label", descKey: "settings.download.networkMode.system.desc" },
  { value: "direct", labelKey: "settings.download.networkMode.direct.label", descKey: "settings.download.networkMode.direct.desc" },
];

export const communitySourceOptions: OptionKeys[] = [
  { value: "auto", labelKey: "settings.download.communityMode.auto.label", descKey: "settings.download.communityMode.auto.desc" },
  { value: "officialOnly", labelKey: "settings.download.communityMode.officialOnly.label", descKey: "settings.download.communityMode.officialOnly.desc" },
  { value: "mirrorOnly", labelKey: "settings.download.communityMode.mirrorOnly.label", descKey: "settings.download.communityMode.mirrorOnly.desc" },
];

// JDK（managed runtime）下载源三态：auto 并发探测三源（Mojang/Adoptium/Azul）最快优先、不可达兜底；officialOnly 固定 Mojang→Azul；mirrorOnly 固定 Mojang（文件走镜像）→Adoptium 仅 TUNA（元数据走官方 API）。
export const jdkSourceOptions: OptionKeys[] = [
  { value: "auto", labelKey: "settings.download.jdkMode.auto.label", descKey: "settings.download.jdkMode.auto.desc" },
  { value: "officialOnly", labelKey: "settings.download.jdkMode.officialOnly.label", descKey: "settings.download.jdkMode.officialOnly.desc" },
  { value: "mirrorOnly", labelKey: "settings.download.jdkMode.mirrorOnly.label", descKey: "settings.download.jdkMode.mirrorOnly.desc" },
];

// 社区翻译显示模式（默认仅中文，en 后端跳过翻译请求）；标题译名来自内置表，简介翻译来自 MCIM
export const translateDisplayOptions: OptionKeys[] = [
  { value: "zh", labelKey: "settings.download.translateMode.zh.label", descKey: "settings.download.translateMode.zh.desc" },
  { value: "en", labelKey: "settings.download.translateMode.en.label", descKey: "settings.download.translateMode.en.desc" },
];

const section = useConfigSection();
export const dlError = section.error;

export const dlGameMode = ref("auto");
export const dlCommunityMode = ref("auto");
export const dlJdkMode = ref("auto");
export const dlConcurrency = ref(64);
export const dlMaxSpeed = ref(0); // bytes/s，0 = 不限
// 镜像源并发上限（须与后端 constants.rs MIRROR_CONCURRENCY 同步）：官方源不限流，镜像恒为 16（实测跨文件 32 并发无 429）
export const MIRROR_CONCURRENCY = 16;
// 仅镜像模式下并发由镜像源规定，输入锁定
export const concurrencyLocked = computed(() => dlGameMode.value === "mirrorOnly");
export const dlDetailOpen = ref(false);
/** 下载源主开关：三分项一致时显示统一值，否则 "custom"；纯前端聚合，不改后端三个独立字段。 */
export type SourceMasterValue = "auto" | "officialOnly" | "mirrorOnly" | "custom";
const sourceModes = [dlGameMode, dlCommunityMode, dlJdkMode];
export const dlSourceMaster = computed<SourceMasterValue>(() => {
  const first = sourceModes[0].value;
  if (sourceModes.every((m) => m.value === first)) return first as SourceMasterValue;
  return "custom";
});

/** 主开关联动：设统一值同步各细分（custom 经细分修改自然聚合）。 */
export function setSourceMaster(v: SourceMasterValue) {
  if (v === "custom") return;
  dlGameMode.value = v;
  dlCommunityMode.value = v;
  dlJdkMode.value = v;
  void saveDownloadConfig();
}
export const dlMinecraftDir = ref(".minecraft");
export const dlNetworkMode = ref("auto");
export const dlTranslateMode = ref<"zh" | "en">("zh");
export const dlShowReleaseOnly = ref(true);

export async function loadDownloadConfig() {
  try {
    const cfg = await invoke<DownloaderConfig>("get_downloader_config");
    // 旧配置 preferOfficial/preferChina 后端已迁移为 auto；此处兜底归一
    const normalize = (v?: string) =>
      v === "officialOnly" || v === "mirrorOnly" ? v : "auto";
    dlGameMode.value = normalize(cfg.game_source_mode);
    dlCommunityMode.value = normalize(cfg.community_source_mode);
    dlJdkMode.value = normalize(cfg.jdk_source_mode);
    // 网络代理模式（旧配置无此字段 → 自动检测）
    dlNetworkMode.value =
      cfg.network_mode === "system" || cfg.network_mode === "direct" ? cfg.network_mode : "auto";
    // 旧配置默认线程数为 8，统一迁移到 64
    dlConcurrency.value = (cfg.concurrency === 8 ? 64 : (cfg.concurrency ?? 64));
    dlConcurrency.value = Math.min(256, Math.max(1, dlConcurrency.value));
    dlMinecraftDir.value = cfg.minecraft_dir || ".minecraft";
    const mode = cfg.translate_display;
    if (mode === "zh" || mode === "both" || mode === "en") {
      // both（中英对照）已移除，旧配置按仅中文处理
      dlTranslateMode.value = mode === "both" ? "zh" : mode;
    }
    dlShowReleaseOnly.value = cfg.show_release_only ?? true;
    dlMaxSpeed.value = cfg.max_speed ?? 0;
  } catch {
  }
}

export async function saveDownloadConfig() {
  await section.run(async () => {
    await invoke("save_downloader_config", {
      config: {
        game_source_mode: dlGameMode.value,
        community_source_mode: dlCommunityMode.value,
        jdk_source_mode: dlJdkMode.value,
        network_mode: dlNetworkMode.value,
        concurrency: dlConcurrency.value,
        minecraft_dir: dlMinecraftDir.value,
        translate_display: dlTranslateMode.value,
        show_release_only: dlShowReleaseOnly.value,
        max_speed: dlMaxSpeed.value,
      },
    });
  });
}

export function setGameMode(v: string) {
  dlGameMode.value = v;
  void saveDownloadConfig();
}

export function setCommunityMode(v: string) {
  dlCommunityMode.value = v;
  void saveDownloadConfig();
}

export function setJdkMode(v: string) {
  if (v === "auto" || v === "officialOnly" || v === "mirrorOnly") {
    dlJdkMode.value = v;
  }
  void saveDownloadConfig();
}

export function setTranslateMode(v: string) {
  if (v === "zh" || v === "en") {
    dlTranslateMode.value = v;
  }
  void saveDownloadConfig();
}

export function setNetworkMode(v: string) {
  if (v === "auto" || v === "system" || v === "direct") {
    dlNetworkMode.value = v;
  }
  void saveDownloadConfig();
}
