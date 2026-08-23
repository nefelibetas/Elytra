// 启动选项 store（对齐 PCL2 启动设置）：窗口/优先级/内存/参数/GC 等。
import { ref, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { memState } from "./memory";
import { useConfigSection } from "../utils/useConfigSection";

export interface LauncherOptions {
  memory: "Auto" | { Custom: number };
  window_title: string;
  process_priority: "Low" | "Normal" | "High" | "RealTime";
  window_size: "Default" | { Custom: [number, number] };
  disable_launch_wrapper: boolean;
  disable_lwjgl_unsafe_agent: boolean;
  use_high_performance_gpu: boolean;
  pre_launch_memory_optimize: boolean;
  game_language: string;
  jvm_args: string[];
  game_args: string[];
  pre_launch_cmd: string;
  gc_type: string;
  verify_ssl: boolean;
}

const section = useConfigSection();
export const launchError = section.error;

export const loWindowTitle = ref("");
export const loPriority = ref<"Low" | "Normal" | "High" | "RealTime">("Normal");
export const loWinMode = ref<"Default" | "Custom">("Default");
export const loWinW = ref(1280);
export const loWinH = ref(720);
export const loGameLang = ref("auto_zh_CN");

// 内存分配（状态由全局 startMemoryMonitor 持续刷新）
export const loMemMode = ref<"Auto" | "Custom">("Auto");
export const loMemMb = ref(4096);
export const loMemOptimize = ref(false);

export const memUsedMb = computed(() => Math.max(0, memState.totalMb - memState.availMb));
export const memGameMb = computed(() => {
  // 自动配置：显示 0 mod 基准值（启动时后端按实际 mod 数量动态调整）
  const g = loMemMode.value === "Custom"
    ? loMemMb.value
    : Math.max(2048, Math.floor(memState.availMb * 0.60));
  return Math.min(g, memState.availMb || Number.MAX_SAFE_INTEGER);
});
export const memUsedPct = computed(() =>
  memState.totalMb ? (memUsedMb.value / memState.totalMb) * 100 : 0);
export const memGamePct = computed(() => {
  const remain = Math.max(0, memState.totalMb - memUsedMb.value);
  const g = Math.min(memGameMb.value, remain);
  return memState.totalMb ? (g / memState.totalMb) * 100 : 0;
});

// 自定义值超出最新剩余内存 → 动态收紧到可用范围内
watch(
  () => memState.availMb,
  (avail) => {
    if (avail > 0 && loMemMb.value > avail) {
      loMemMb.value = Math.max(1024, avail);
    }
  },
);

// Java 高级项
export const loJvmArgs = ref("");
export const loGameArgs = ref("");
export const loPreCmd = ref("");
export const loGc = ref("TunedG1Gc");
export const loDisableWrapper = ref(false);
export const loDisableAgent = ref(false);
export const loHighGpu = ref(false);
export const loVerifySsl = ref(false);

export const gcOptions: { value: string; label: string }[] = [
  { value: "PreferZgc", label: "首选 ZGC（实验性）" },
  { value: "PreferGenerationalZgc", label: "首选分代 ZGC（实验性）" },
  { value: "StandardG1Gc", label: "标准 G1" },
  { value: "TunedG1Gc", label: "调优 G1（默认）" },
  { value: "Custom", label: "自定义（不追加 GC 参数）" },
];

export async function loadLaunchOptions() {
  try {
    const cfg = await invoke<LauncherOptions>("get_launch_options");
    loWindowTitle.value = cfg.window_title ?? "";
    loPriority.value = cfg.process_priority;
    if (cfg.window_size === "Default") {
      loWinMode.value = "Default";
    } else {
      loWinMode.value = "Custom";
      loWinW.value = cfg.window_size.Custom[0];
      loWinH.value = cfg.window_size.Custom[1];
    }
    loJvmArgs.value = (cfg.jvm_args ?? []).join("\n");
    loGameArgs.value = (cfg.game_args ?? []).join("\n");
    loPreCmd.value = cfg.pre_launch_cmd ?? "";
    loGc.value = cfg.gc_type ?? "TunedG1Gc";
    loDisableWrapper.value = !!cfg.disable_launch_wrapper;
    loDisableAgent.value = !!cfg.disable_lwjgl_unsafe_agent;
    loVerifySsl.value = !!cfg.verify_ssl;
    // 高性能显卡默认开启：旧配置/旧后端缺字段时按 true 处理
    loHighGpu.value = cfg.use_high_performance_gpu ?? true;
    loGameLang.value = cfg.game_language ?? "auto_zh_CN";
    loMemMode.value = cfg.memory === "Auto" ? "Auto" : "Custom";
    if (cfg.memory !== "Auto") loMemMb.value = (cfg.memory as any).Custom ?? 4096;
    loMemOptimize.value = !!cfg.pre_launch_memory_optimize;
    // 内存快照由全局监听提供；首帧未就绪时补拉一次
    if (!memState.availMb) {
      try {
        const [total, avail] = await invoke<[number, number]>("get_memory_info");
        memState.totalMb = total;
        memState.availMb = avail;
      } catch {
        // 忽略
      }
    }
    if (loMemMode.value === "Auto") {
      loMemMb.value = Math.max(2048, Math.min(Math.floor(memState.availMb * 0.60), memState.availMb || Number.MAX_SAFE_INTEGER));
    }
  } catch {
    // 无配置时保持默认
  }
}

/** 保存启动选项（保留 jvm_args 等其余字段，仅覆盖用户可改项）。 */
export async function saveLaunchOptions() {
  await section.run(async () => {
    const cfg = await invoke<LauncherOptions>("get_launch_options");
    cfg.window_title = loWindowTitle.value.trim();
    cfg.process_priority = loPriority.value;
    cfg.window_size =
      loWinMode.value === "Custom"
        ? { Custom: [loWinW.value, loWinH.value] }
        : "Default";
    cfg.jvm_args = loJvmArgs.value.split(/\n/).map(s => s.trim()).filter(Boolean);
    cfg.game_args = loGameArgs.value.split(/\n/).map(s => s.trim()).filter(Boolean);
    cfg.pre_launch_cmd = loPreCmd.value.trim();
    cfg.memory = loMemMode.value === "Custom" ? { Custom: loMemMb.value } : "Auto";
    cfg.pre_launch_memory_optimize = loMemOptimize.value;
    cfg.gc_type = loGc.value;
    cfg.disable_launch_wrapper = loDisableWrapper.value;
    cfg.disable_lwjgl_unsafe_agent = loDisableAgent.value;
    cfg.verify_ssl = loVerifySsl.value;
    cfg.use_high_performance_gpu = loHighGpu.value;
    cfg.game_language = loGameLang.value;
    await invoke("save_launch_options", { config: cfg });
  });
}
