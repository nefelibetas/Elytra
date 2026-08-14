// 下载设置 store（管理游戏下载源）：文件/版本列表/社区源模式、线程数、速度限制。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useConfigSection } from "../utils/useConfigSection";

export const sourceModeOptions = [
  { value: "auto", label: "智能模式", desc: "按资源类型自动选源：Loader 资源优先国内镜像，其余按健康度自动排序" },
  { value: "preferOfficial", label: "优先官方源", desc: "官方源优先，加载缓慢时使用国内镜像源" },
  { value: "preferChina", label: "优先镜像源", desc: "国内镜像源优先，失败时回退官方源" },
  { value: "officialOnly", label: "官方源", desc: "仅使用官方源，不自动回退" },
  { value: "chinaOnly", label: "国内镜像源", desc: "仅使用国内镜像源，不自动回退" },
];

// 社区下载源：Modrinth 官方 API/CDN vs MCIM 国内镜像（管理 Fabric API 等社区资源）
export const communitySourceOptions = [
  { value: "preferOfficial", label: "优先Modrinth/CurseForge官方", desc: "优先Modrinth/CurseForge官方，加载缓慢时使用MCIM镜像站" },
  { value: "preferChina", label: "优先MCIM镜像站", desc: "优先MCIM镜像站，失败时回退Modrinth/CurseForge官方" },
  { value: "officialOnly", label: "仅Modrinth/CurseForge", desc: "仅使用Modrinth/CurseForge官方" },
  { value: "chinaOnly", label: "仅MCIM镜像", desc: "仅使用MCIM镜像站" },
];

export function sourceModeLabel(v: string) {
  return sourceModeOptions.find((o) => o.value === v)?.label ?? v;
}

// 社区搜索结果翻译显示模式（默认仅中文；en 模式后端跳过翻译请求）
// 标题译名来自内置表（热门 Mod 社区常用名），简介翻译来自 MCIM
export const translateDisplayOptions = [
  { value: "zh", label: "仅中文", desc: "标题与简介显示中文（默认）" },
  { value: "en", label: "仅英文", desc: "不请求翻译，显示英文原文" },
];

export function translateModeLabel(v: string) {
  return translateDisplayOptions.find((o) => o.value === v)?.label ?? v;
}

interface DownloaderConfig {
  file_source_mode: string;
  manifest_source_mode: string;
  community_source_mode?: string;
  concurrency: number;
  max_speed: number; // bytes/s，0 = 无限制
  minecraft_dir: string;
  translate_display?: string;
  show_release_only?: boolean;
}

const section = useConfigSection();
export const dlError = section.error;

export const dlFileMode = ref("preferOfficial");
export const dlManifestMode = ref("preferOfficial");
export const dlCommunityMode = ref("preferOfficial");
export const dlConcurrency = ref(64);
// 镜像源并发上限（后端 constants.rs MIRROR_CONCURRENCY，改动需同步）：
// 官方源不限流、镜像源（BMCLAPI）30 并发即触发 429，镜像请求恒为 8 并发
export const MIRROR_CONCURRENCY = 8;
// 仅镜像模式下并发数由镜像源规定，输入锁定（偏好/官方模式并发给官方源配置）
export const concurrencyLocked = computed(() => dlFileMode.value === "chinaOnly");
export const dlMaxSpeed = ref(0); // MB/s，0 = 无上限
export const dlFileOpen = ref(false);
export const dlManifestOpen = ref(false);
export const dlCommunityOpen = ref(false);
export const dlTranslateOpen = ref(false);
export const dlMinecraftDir = ref(".minecraft");
export const dlTranslateMode = ref<"zh" | "en">("zh");
/** 社区资源版本列表仅显示正式版（默认开启，过滤快照/预发布/候选版本） */
export const dlShowReleaseOnly = ref(true);

export async function loadDownloadConfig() {
  try {
    const cfg = await invoke<DownloaderConfig>("get_downloader_config");
    dlFileMode.value = cfg.file_source_mode ?? "preferOfficial";
    dlManifestMode.value = cfg.manifest_source_mode ?? "preferOfficial";
    dlCommunityMode.value = cfg.community_source_mode ?? "preferOfficial";
    // 旧配置默认线程数为 8，统一迁移到 64
    dlConcurrency.value = (cfg.concurrency === 8 ? 64 : (cfg.concurrency ?? 64));
    dlConcurrency.value = Math.min(256, Math.max(1, dlConcurrency.value));
    const mb = Math.round((cfg.max_speed ?? 0) / (1024 * 1024));
    dlMaxSpeed.value = Math.min(100, Math.max(0, mb));
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
        concurrency: dlConcurrency.value,
        max_speed: dlMaxSpeed.value * 1024 * 1024,
        minecraft_dir: dlMinecraftDir.value,
        translate_display: dlTranslateMode.value,
        show_release_only: dlShowReleaseOnly.value,
      },
    });
  });
}

export function setFileMode(v: string) {
  dlFileMode.value = v;
  dlFileOpen.value = false;
  saveDownloadConfig();
}

export function setCommunityMode(v: string) {
  dlCommunityMode.value = v;
  dlCommunityOpen.value = false;
  saveDownloadConfig();
}

export function setManifestMode(v: string) {
  dlManifestMode.value = v;
  dlManifestOpen.value = false;
  saveDownloadConfig();
}

export function setTranslateMode(v: string) {
  if (v === "zh" || v === "en") {
    dlTranslateMode.value = v;
  }
  dlTranslateOpen.value = false;
  saveDownloadConfig();
}
