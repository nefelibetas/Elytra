// 下载设置 store（管理游戏下载源）：文件/版本列表/社区源模式、线程数、速度限制。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useConfigSection } from "../utils/useConfigSection";

export const sourceModeOptions = [
  { value: "auto", label: "智能模式", desc: "程序自动判断更快源：按资源类型与健康度自动选源，官方过慢自动降级镜像" },
  { value: "officialOnly", label: "官方源", desc: "仅使用官方源，不自动回退" },
  { value: "chinaOnly", label: "国内镜像源", desc: "仅使用国内镜像源，不自动回退" },
];

// 网络代理模式：auto 自动检测系统代理（Windows 注册表 / Linux 环境变量或 gsettings）；
// system 强制系统代理；direct 强制直连（TUN 模式由网络层接管，直连也会走代理）
export const networkModeOptions = [
  { value: "auto", label: "自动检测", desc: "自动使用系统代理；未开启代理时直连（TUN 模式自动生效）" },
  { value: "system", label: "系统代理", desc: "强制使用系统代理（检测不到时回退直连）" },
  { value: "direct", label: "直连", desc: "不走应用层代理，直连网络（TUN 模式仍由系统接管）" },
];

// 社区下载源：Modrinth 官方 API/CDN vs MCIM 国内镜像（管理 Fabric API 等社区资源）
export const communitySourceOptions = [
  { value: "auto", label: "智能模式", desc: "优先Modrinth/CurseForge官方，加载缓慢时自动使用MCIM镜像站" },
  { value: "officialOnly", label: "仅Modrinth/CurseForge", desc: "仅使用Modrinth/CurseForge官方" },
  { value: "chinaOnly", label: "仅MCIM镜像", desc: "仅使用MCIM镜像站" },
];

// JDK（managed runtime）下载源三态：auto 并发速率探测三源（Mojang / Adoptium / Azul）
// 最快优先、不可达兜底；officialOnly 固定 Mojang → Azul；chinaOnly 固定
// Mojang（文件走文件源镜像）→ Adoptium 仅 TUNA（元数据例外走官方 API）。
export const jdkSourceOptions = [
  { value: "auto", label: "智能测速", desc: "Mojang / Adoptium / Azul 三源并发测速，最快者优先下载，不可达源自动兜底" },
  { value: "officialOnly", label: "仅官方源", desc: "仅使用 Mojang 官方与 Azul，不启用国内镜像" },
  { value: "chinaOnly", label: "仅国内镜像", desc: "Mojang 走文件源镜像，Adoptium 仅从清华 TUNA 下载（元数据例外走官方 API）" },
];

// 社区搜索结果翻译显示模式（默认仅中文；en 模式后端跳过翻译请求）
// 标题译名来自内置表（热门 Mod 社区常用名），简介翻译来自 MCIM
export const translateDisplayOptions = [
  { value: "zh", label: "仅中文", desc: "标题与简介显示中文（默认）" },
  { value: "en", label: "仅英文", desc: "不请求翻译，显示英文原文" },
];

interface DownloaderConfig {
  file_source_mode: string;
  manifest_source_mode: string;
  community_source_mode?: string;
  network_mode?: string;
  concurrency: number;
  max_speed: number; // bytes/s，0 = 无限制
  minecraft_dir: string;
  translate_display?: string;
  show_release_only?: boolean;
  jdk_source_mode?: string;
}

const section = useConfigSection();
export const dlError = section.error;

export const dlFileMode = ref("auto");
export const dlManifestMode = ref("auto");
export const dlCommunityMode = ref("auto");
export const dlJdkMode = ref("auto");
export const dlConcurrency = ref(64);
// 镜像源并发上限（后端 constants.rs MIRROR_CONCURRENCY，改动需同步）：
// 官方源不限流、镜像源（BMCLAPI）30 并发即触发 429，镜像请求恒为 8 并发
export const MIRROR_CONCURRENCY = 8;
// 仅镜像模式下并发数由镜像源规定，输入锁定（智能/官方模式并发给官方源配置）
export const concurrencyLocked = computed(() => dlFileMode.value === "chinaOnly");
/** 细分源明细折叠区显隐（与主开关下拉解耦；DropSelect 自管开合）。 */
export const dlDetailOpen = ref(false);
/**
 * 下载源主开关：四细分（文件/版本列表/社区/JDK）是否一致时显示统一值，
 * 不一致显示 "custom"。默认收起，展开可见四个细分各自独立设置。
 * 纯前端聚合，不改后端四个独立字段。
 */
export type SourceMasterValue = "auto" | "officialOnly" | "chinaOnly" | "custom";
const sourceModes = [dlFileMode, dlManifestMode, dlCommunityMode, dlJdkMode];
export const dlSourceMaster = computed<SourceMasterValue>(() => {
  const first = sourceModes[0].value;
  if (sourceModes.every((m) => m.value === first)) return first as SourceMasterValue;
  return "custom";
});

/** 主开关联动：设统一值同步四个细分（custom 不经此函数，由细分修改自然聚合）。 */
export function setSourceMaster(v: SourceMasterValue) {
  if (v === "custom") return;
  dlFileMode.value = v;
  dlManifestMode.value = v;
  dlCommunityMode.value = v;
  dlJdkMode.value = v;
  saveDownloadConfig();
}
export const dlMinecraftDir = ref(".minecraft");
export const dlNetworkMode = ref("auto");
export const dlTranslateMode = ref<"zh" | "en">("zh");
/** 社区资源版本列表仅显示正式版（默认开启，过滤快照/预发布/候选版本） */
export const dlShowReleaseOnly = ref(true);

export async function loadDownloadConfig() {
  try {
    const cfg = await invoke<DownloaderConfig>("get_downloader_config");
    // 旧配置 preferOfficial/preferChina 后端已迁移为 auto；此处兜底归一
    const normalize = (v?: string) =>
      v === "officialOnly" || v === "chinaOnly" ? v : "auto";
    dlFileMode.value = normalize(cfg.file_source_mode);
    dlManifestMode.value = normalize(cfg.manifest_source_mode);
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
  } catch {
    // 无配置时保持默认
  }
}

export async function saveDownloadConfig() {
  await section.run(async () => {
    await invoke("save_downloader_config", {
      config: {
        file_source_mode: dlFileMode.value,
        manifest_source_mode: dlManifestMode.value,
        community_source_mode: dlCommunityMode.value,
        jdk_source_mode: dlJdkMode.value,
        network_mode: dlNetworkMode.value,
        concurrency: dlConcurrency.value,
        minecraft_dir: dlMinecraftDir.value,
        translate_display: dlTranslateMode.value,
        show_release_only: dlShowReleaseOnly.value,
      },
    });
  });
}

export function setFileMode(v: string) {
  dlFileMode.value = v;
  saveDownloadConfig();
}

export function setCommunityMode(v: string) {
  dlCommunityMode.value = v;
  saveDownloadConfig();
}

export function setJdkMode(v: string) {
  if (v === "auto" || v === "officialOnly" || v === "chinaOnly") {
    dlJdkMode.value = v;
  }
  saveDownloadConfig();
}

export function setManifestMode(v: string) {
  dlManifestMode.value = v;
  saveDownloadConfig();
}

export function setTranslateMode(v: string) {
  if (v === "zh" || v === "en") {
    dlTranslateMode.value = v;
  }
  saveDownloadConfig();
}

export function setNetworkMode(v: string) {
  if (v === "auto" || v === "system" || v === "direct") {
    dlNetworkMode.value = v;
  }
  saveDownloadConfig();
}
