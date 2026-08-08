<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { showToast } from "../utils/toast";
import { useConfigSection } from "../utils/useConfigSection";
import { SKIN_TYPE_OPTIONS, type OfflineSkinType, type OfflineSkinModel } from "../utils/offlineSkin";

interface JdkInfo { path: string; version: string; vendor: string; bits: number; }
type JdkSelection = { mode: "auto" } | { mode: "manual"; path: string };
interface JdkConfig { jdk_selection: JdkSelection; discovered_jdks: JdkInfo[]; }

type Tab = "launch" | "download";

const currentTab = ref<Tab>("launch");

// ── Java 配置 ──
const jdkList = ref<JdkInfo[]>([]);
const autoMode = ref(true);
const activeJdk = ref<JdkInfo | null>(null);
const jdkSection = useConfigSection();
const jdkError = jdkSection.error;
const loading = ref(false);
const pickerOpen = ref(false);

async function loadConfig() {
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

async function saveConfig() {
  await jdkSection.run(async () => {
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

onMounted(() => {
  loadConfig();
  loadOfflineSkin();
  loadLaunchOptions();
  loadDownloadConfig();
});

async function refreshJdks() {
  loading.value = true;
  await jdkSection.run(async () => {
    jdkList.value = await invoke<JdkInfo[]>("auto_scan_jdk");
  });
  loading.value = false;
}

async function selectCustomDir() {
  loading.value = true;
  await jdkSection.run(async () => {
    const dir = await open({ directory: true, multiple: false });
    if (!dir) return;
    jdkList.value = await invoke<JdkInfo[]>("scan_custom_jdk", { path: dir });
  });
  loading.value = false;
}

function selectAuto() {
  autoMode.value = true;
  activeJdk.value = null;
  pickerOpen.value = false;
  saveConfig();
}

function selectJdk(env: JdkInfo) {
  autoMode.value = false;
  activeJdk.value = env;
  pickerOpen.value = false;
  saveConfig();
}

const pickerLabel = computed(() => {
  if (autoMode.value) return "自动选择";
  return activeJdk.value ? activeJdk.value.version : "未选择";
});

const pickerSub = computed(() => {
  if (autoMode.value) return "根据游戏版本需求自动匹配";
  return activeJdk.value?.path ?? "";
});

// ── 启动选项（对齐 PCL2 启动设置） ──
interface LauncherOptions {
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

const loWindowTitle = ref("");
const loPriority = ref<"Low" | "Normal" | "High" | "RealTime">("Normal");
const loWinMode = ref<"Default" | "Custom">("Default");
const loWinW = ref(1280);
const loWinH = ref(720);
const loGameLang = ref("auto_zh_CN");

// 内存分配（状态由全局 startMemoryMonitor 持续刷新）
import { memState } from "../store/memory";
const loMemMode = ref<"Auto" | "Custom">("Auto");
const loMemMb = ref(4096);
const loMemOptimize = ref(false);

const memUsedMb = computed(() => Math.max(0, memState.totalMb - memState.availMb));
const memGameMb = computed(() => {
  // 自动配置：显示 0 mod 基准值（启动时后端按实际 mod 数量动态调整）
  const g = loMemMode.value === "Custom"
    ? loMemMb.value
    : Math.max(2048, Math.floor(memState.availMb * 0.60));
  return Math.min(g, memState.availMb || Number.MAX_SAFE_INTEGER);
});
const memUsedPct = computed(() =>
  memState.totalMb ? (memUsedMb.value / memState.totalMb) * 100 : 0);
const memGamePct = computed(() => {
  const remain = Math.max(0, memState.totalMb - memUsedMb.value);
  const g = Math.min(memGameMb.value, remain);
  return memState.totalMb ? (g / memState.totalMb) * 100 : 0;
});
const memLabelLeft = (pct: number) => Math.min(Math.max(pct, 4), 96) + "%";
const fmtGb = (mb: number) => (mb / 1024).toFixed(1) + "GB";

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
const loJvmArgs = ref("");
const loGameArgs = ref("");
const loPreCmd = ref("");
const loGc = ref("TunedG1Gc");
const loDisableWrapper = ref(false);
const loDisableAgent = ref(false);
const loHighGpu = ref(false);
const loVerifySsl = ref(false);

const gcOptions: { value: string; label: string }[] = [
  { value: "PreferZgc", label: "首选 ZGC（实验性）" },
  { value: "PreferGenerationalZgc", label: "首选分代 ZGC（实验性）" },
  { value: "StandardG1Gc", label: "标准 G1" },
  { value: "TunedG1Gc", label: "调优 G1（默认）" },
  { value: "Custom", label: "自定义（不追加 GC 参数）" },
];

const launchSection = useConfigSection();
const launchError = launchSection.error;

async function loadLaunchOptions() {
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
async function saveLaunchOptions() {
  await launchSection.run(async () => {
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

// ── 离线皮肤配置（对齐 PCL2 五种） ──
import AlertDialog from "../component/AlertDialog.vue";

const skinType = ref<OfflineSkinType>("random");
const skinPlayerName = ref("");
const skinCustomPath = ref("");
const skinCustomModel = ref<OfflineSkinModel>("steve");
const skinSection = useConfigSection();
const skinError = skinSection.error;

const skinOptions = SKIN_TYPE_OPTIONS;

async function loadOfflineSkin() {
  try {
    const cfg = await invoke<{
      skin_type: OfflineSkinType;
      player_name: string;
      custom_path: string;
      custom_model: OfflineSkinModel;
    }>("get_offline_skin");
    skinType.value = cfg.skin_type;
    skinPlayerName.value = cfg.player_name;
    skinCustomPath.value = cfg.custom_path;
    skinCustomModel.value = cfg.custom_model;
  } catch {
    // 无配置时保持默认
  }
}

async function saveOfflineSkin() {
  await skinSection.run(async () => {
    await invoke("set_offline_skin", {
      config: {
        skin_type: skinType.value,
        player_name: skinPlayerName.value.trim(),
        custom_path: skinCustomPath.value,
        custom_model: skinCustomModel.value,
      },
    });
  });
}

/** 当前选中选项的详细描述（选中后才展示）。 */
const currentSkinDesc = computed(
  () => skinOptions.find(o => o.value === skinType.value)?.desc ?? "",
);

/** 模型选择 dialog（选完皮肤文件后弹出）。 */
const skinModelDialog = ref(false);

/** 点击皮肤类型胶囊：custom 且未选择文件时自动打开文件选择器。 */
function onSkinTypeChange() {
  if (skinType.value === "custom" && !skinCustomPath.value) {
    saveOfflineSkin();
    pickCustomSkin();
    return;
  }
  saveOfflineSkin();
}

/** 打开文件选择器；选完后弹出「选择皮肤种类」模型 dialog。 */
async function pickCustomSkin() {
  const path = await open({
    multiple: false,
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  if (!path) return;
  skinCustomPath.value = path;
  skinModelDialog.value = true;
}

/** 选择模型（Steve / Alex / 启动器决定）→ 保存并关闭。 */
function pickModel(model: OfflineSkinModel) {
  skinCustomModel.value = model;
  skinModelDialog.value = false;
  saveOfflineSkin();
}

/** 重置自定义皮肤：清空文件选择，自动切回随机。 */
function resetCustomSkin() {
  skinCustomPath.value = "";
  skinType.value = "random";
  skinCustomModel.value = "steve";
  saveOfflineSkin();
}

/** 保存正版玩家的皮肤到用户指定位置。 */
async function savePlayerSkin() {
  const name = skinPlayerName.value.trim();
  if (!name) {
    showToast("请先填写正版玩家用户名");
    return;
  }
  const path = await save({
    defaultPath: `${name}.png`,
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  if (!path) return;
  await skinSection.run(async () => {
    await invoke("save_player_skin", { playerName: name, path });
    showToast(`皮肤已保存：${name}`);
  });
}

/** 刷新：重新校验正版玩家皮肤信息。 */
async function refreshPlayerSkin() {
  const name = skinPlayerName.value.trim();
  if (!name) {
    showToast("请先填写正版玩家用户名");
    return;
  }
  await skinSection.run(async () => {
    const info = await invoke<{ uuid: string; texture_id: string }>(
      "fetch_player_skin",
      { playerName: name },
    );
    showToast(`皮肤信息已刷新：${name}`);
    void info;
  });
}

// ── 下载设置（管理游戏下载源） ──
const sourceModeOptions = [
  { value: "preferOfficial", label: "优先官方源", desc: "官方源优先，加载缓慢时使用国内镜像源" },
  { value: "preferChina", label: "优先镜像源", desc: "国内镜像源优先，失败时回退官方源" },
  { value: "officialOnly", label: "官方源", desc: "仅使用官方源，不自动回退" },
  { value: "chinaOnly", label: "国内镜像源", desc: "仅使用国内镜像源，不自动回退" },
];
function sourceModeLabel(v: string) {
  return sourceModeOptions.find((o) => o.value === v)?.label ?? v;
}
// 社区下载源：Modrinth 官方 API/CDN vs MCIM 国内镜像（管理 Fabric API 等社区资源）
const communitySourceOptions = [
  { value: "preferOfficial", label: "Modrinth 优先", desc: "Modrinth 官方优先，加载缓慢时使用 MCIM 镜像" },
  { value: "preferChina", label: "MCIM 优先", desc: "MCIM 镜像优先，失败时回退 Modrinth" },
  { value: "officialOnly", label: "Modrinth", desc: "仅使用 Modrinth 官方，不自动回退" },
  { value: "chinaOnly", label: "MCIM", desc: "仅使用 MCIM 镜像，不自动回退" },
];

interface DownloaderConfig {
  file_source_mode: string;
  manifest_source_mode: string;
  community_source_mode?: string;
  concurrency: number;
  max_speed: number; // bytes/s，0 = 无限制
  minecraft_dir: string;
}

const dlFileMode = ref("preferOfficial");
const dlManifestMode = ref("preferOfficial");
const dlCommunityMode = ref("preferOfficial");
const dlConcurrency = ref(64);
// 镜像源并发上限（后端 constants.rs MIRROR_CONCURRENCY，改动需同步）：
// 官方源不限流、镜像源（BMCLAPI）30 并发即触发 429，镜像请求恒为 8 并发
const MIRROR_CONCURRENCY = 8;
// 仅镜像模式下并发数由镜像源规定，输入锁定（偏好/官方模式并发给官方源配置）
const concurrencyLocked = computed(() => dlFileMode.value === "chinaOnly");
const dlMaxSpeed = ref(0); // MB/s，0 = 无上限
const dlFileOpen = ref(false);
const dlManifestOpen = ref(false);
const dlCommunityOpen = ref(false);
const dlSection = useConfigSection();
const dlError = dlSection.error;
const dlMinecraftDir = ref(".minecraft");

async function loadDownloadConfig() {
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
  } catch {
    // 无配置时保持默认
  }
}

async function saveDownloadConfig() {
  await dlSection.run(async () => {
    await invoke("save_downloader_config", {
      config: {
        file_source_mode: dlFileMode.value,
        manifest_source_mode: dlManifestMode.value,
        community_source_mode: dlCommunityMode.value,
        concurrency: dlConcurrency.value,
        max_speed: dlMaxSpeed.value * 1024 * 1024,
        minecraft_dir: dlMinecraftDir.value,
      },
    });
  });
}

function setFileMode(v: string) {
  dlFileMode.value = v;
  dlFileOpen.value = false;
  saveDownloadConfig();
}

function setCommunityMode(v: string) {
  dlCommunityMode.value = v;
  dlCommunityOpen.value = false;
  saveDownloadConfig();
}
function setManifestMode(v: string) {
  dlManifestMode.value = v;
  dlManifestOpen.value = false;
  saveDownloadConfig();
}

// ── 未实现 tab 占位 ──
const placeholder = computed(() => {
  if (currentTab.value === "download") return "下载设置（开发中）";
  return "";
});
</script>

<template>
  <div class="stg">
    <nav class="stg-nav">
      <div class="nav-divider"><span>设置</span></div>
      <button
        class="stg-tab"
        :class="{ active: currentTab === 'launch' }"
        @click="currentTab = 'launch'"
      >
        <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
          <path d="M3 2h10a1 1 0 011 1v10a1 1 0 01-1 1H3a1 1 0 01-1-1V3a1 1 0 011-1zm1 2v2h8V4H4zm0 4v4h8V8H4z" />
        </svg>
        启动设置
      </button>
      <button
        class="stg-tab"
        :class="{ active: currentTab === 'download' }"
        @click="currentTab = 'download'"
      >
        <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
          <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 12V3a5 5 0 010 10z" />
        </svg>
        下载设置
      </button>
    </nav>

    <div class="stg-body">
      <Transition name="page" mode="out-in">
        <!-- 启动设置：离线皮肤 -->
        <div v-if="currentTab === 'launch'" key="launch" class="stg-page">
          <div class="stg-card">
            <div class="stg-card-hdr">
              <h3>离线皮肤</h3>
              <div class="stg-card-sub">离线模式使用的皮肤<span class="stg-warn">仅保证于 1.19.2 版本以下可用</span></div>
            </div>
            <div class="skin-opts">
              <label
                v-for="opt in skinOptions"
                :key="opt.value"
                class="skin-opt"
                :class="{ selected: skinType === opt.value }"
              >
                <input type="radio" :value="opt.value" v-model="skinType" @change="onSkinTypeChange" />
                <span class="skin-opt-label">{{ opt.label }}</span>
              </label>
            </div>
            <!-- 选中后显示详细信息 -->
            <div class="skin-detail">
              <div class="skin-desc">{{ currentSkinDesc }}</div>
              <div v-if="skinType === 'player'" class="skin-player-row">
                <input
                  v-model="skinPlayerName"
                  class="skin-input"
                  type="text"
                  placeholder="正版玩家用户名"
                  maxlength="16"
                  @change="saveOfflineSkin"
                />
                <button class="tool-btn" @click="savePlayerSkin">
                  <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                    <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 12V3a5 5 0 010 10z" />
                  </svg>
                  保存皮肤
                </button>
                <button class="tool-btn" @click="refreshPlayerSkin">
                  <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M14 8a6 6 0 01-11 3.5"/>
                    <polygon points="3,14 2.5,10 7,11" stroke="none" fill="currentColor"/>
                    <path d="M2 8a6 6 0 0111-3.5"/>
                    <polygon points="13,2 13.5,6 9,5" stroke="none" fill="currentColor"/>
                  </svg>
                  刷新
                </button>
              </div>
              <div v-if="skinType === 'custom'" class="skin-sub-row">
                <template v-if="skinCustomPath">
                  <button class="tool-btn" @click="pickCustomSkin">
                    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                      <path d="M11.3 1.3a1.8 1.8 0 012.6 2.6L5.6 12.2l-3.5 1 1-3.5 8.2-8.4zm.9 1.7a.5.5 0 00-.7 0L4 10.6l.4 1 1 .4 7.6-7.5a.5.5 0 000-.7l-.8-.8z" />
                    </svg>
                    更改皮肤
                  </button>
                  <button class="tool-btn" @click="resetCustomSkin">
                    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                      <path d="M8 3a5 5 0 105 5h-2a3 3 0 11-3-3v2L4 4l4-3v2z" />
                    </svg>
                    重置皮肤
                  </button>
                  <span class="skin-model-note">模型：{{ skinCustomModel === 'alex' ? 'Alex（纤细）' : skinCustomModel === 'auto' ? '启动器决定' : 'Steve（经典）' }}</span>
                </template>
                <span v-else class="skin-desc">点击上方「自定义」打开文件选择器</span>
              </div>
            </div>
            <div v-if="skinError" class="err">{{ skinError }}</div>
          </div>

          <div class="stg-card">
            <div class="stg-card-hdr">
              <h3>启动选项</h3>
              <div class="stg-card-sub">游戏启动行为设置</div>
            </div>
            <div class="launch-opts">
              <div class="lo-row">
                <label class="lo-label" for="lo-title">游戏窗口标题</label>
                <input
                  id="lo-title"
                  v-model="loWindowTitle"
                  class="lo-input"
                  type="text"
                  placeholder="不修改（默认）"
                  maxlength="64"
                  @change="saveLaunchOptions"
                />
              </div>

              <div class="lo-row">
                <label class="lo-label" for="lo-prio">进程优先级</label>
                <select
                  id="lo-prio"
                  v-model="loPriority"
                  class="lo-select"
                  @change="saveLaunchOptions"
                >
                  <option value="Low">低</option>
                  <option value="Normal">中（平衡）</option>
                  <option value="High">高</option>
                  <option value="RealTime">实时</option>
                </select>
              </div>

              <div class="lo-row">
                <label class="lo-label" for="lo-winsize">窗口大小</label>
                <div class="lo-winsize">
                  <select
                    id="lo-winsize"
                    v-model="loWinMode"
                    class="lo-select"
                    @change="saveLaunchOptions"
                  >
                    <option value="Default">默认（不设置）</option>
                    <option value="Custom">自定义</option>
                  </select>
                  <template v-if="loWinMode === 'Custom'">
                    <input
                      v-model.number="loWinW"
                      class="lo-input lo-num"
                      type="number"
                      min="320"
                      max="7680"
                      placeholder="宽"
                      @change="saveLaunchOptions"
                    />
                    <span class="lo-times">×</span>
                    <input
                      v-model.number="loWinH"
                      class="lo-input lo-num"
                      type="number"
                      min="240"
                      max="4320"
                      placeholder="高"
                      @change="saveLaunchOptions"
                    />
                  </template>
                </div>
              </div>

              <div class="lo-row">
                <label class="lo-label" for="lo-lang">游戏语言</label>
                <select
                  id="lo-lang"
                  v-model="loGameLang"
                  class="lo-select"
                  @change="saveLaunchOptions"
                >
                  <option value="auto_zh_CN">自动设置为中文</option>
                </select>
              </div>

              <div class="lo-row">
                <label class="lo-check">
                  <input v-model="loVerifySsl" type="checkbox" @change="saveLaunchOptions" />
                  <span>在正版登录时验证 SSL 证书</span>
                </label>
              </div>
            </div>
            <div v-if="launchError" class="err">{{ launchError }}</div>
          </div>

          <div class="stg-card">
            <div class="stg-card-hdr">
              <h3>内存分配</h3>
              <div class="stg-card-sub">游戏内存分配与系统内存状态</div>
            </div>
            <div class="launch-opts">
              <div class="lo-row">
                <label class="lo-check">
                  <input type="radio" value="Auto" v-model="loMemMode" @change="saveLaunchOptions" />
                  <span>自动配置</span>
                </label>
              </div>
              <div class="lo-row">
                <label class="lo-check">
                  <input type="radio" value="Custom" v-model="loMemMode" @change="saveLaunchOptions" />
                  <span>自定义</span>
                </label>
                <input
                  v-model.number="loMemMb"
                  class="mem-slider"
                  :class="{ disabled: loMemMode !== 'Custom' }"
                  type="range"
                  :min="1024"
                  :max="memState.availMb || 4096"
                  step="256"
                  :disabled="loMemMode !== 'Custom'"
                  @change="saveLaunchOptions"
                />
                <span class="mem-slider-val" :class="{ disabled: loMemMode !== 'Custom' }">{{ fmtGb(loMemMb) }}</span>
              </div>
              <div class="lo-row">
                <label class="lo-check">
                  <input v-model="loMemOptimize" type="checkbox" @change="saveLaunchOptions" />
                  <span>启动游戏前进行内存优化</span>
                </label>
              </div>
              <div class="mem-status">
                <div class="mem-bar-labels">
                  <span class="mem-bar-label">已使用 {{ fmtGb(memUsedMb) }}</span>
                  <span v-if="memGamePct > 0" class="mem-bar-label" :style="{ left: memLabelLeft(memUsedPct) }">游戏分配 {{ fmtGb(memGameMb) }}</span>
                </div>
                <div class="mem-bar">
                  <div class="mem-seg mem-used" :style="{ width: memUsedPct + '%' }"></div>
                  <div class="mem-seg mem-game" :style="{ width: memGamePct + '%' }"></div>
                  <div class="mem-seg mem-free"></div>
                </div>
                <div class="mem-total">{{ fmtGb(memUsedMb) }} / {{ fmtGb(memState.totalMb) }}</div>
              </div>
            </div>
          </div>

          <div class="stg-card">
            <div class="stg-card-hdr">
              <h3>Java 运行时</h3>
              <span class="stg-card-sub">选择用于启动游戏的 Java 版本，或由启动器自动匹配</span>
            </div>

            <!-- 选择行：下拉 + 右侧按钮 -->
            <div class="jdk-toolbar">
              <div class="jdk-picker-wrap">
                <button class="jdk-picker" :class="{ open: pickerOpen }" @click="pickerOpen = !pickerOpen">
                  <span class="jdk-picker-main">
                    <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
                      <path d="M11 1L5 7v8h6V7L7 3l4-4V1z" />
                    </svg>
                    {{ pickerLabel }}
                  </span>
                  <span class="jdk-picker-sub">{{ pickerSub }}</span>
                  <svg class="jdk-chevron" :class="{ flip: pickerOpen }" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M3 6l5 5 5-5" />
                  </svg>
                </button>

                <!-- 展开列表 -->
                <Transition name="drop">
                  <div v-if="pickerOpen" class="jdk-dropdown">
                    <button class="jdk-opt" :class="{ selected: autoMode }" @click="selectAuto">
                      <div class="jdk-opt-main">
                        <span class="jdk-opt-title">自动选择</span>
                        <span class="jdk-opt-sub">根据游戏版本需求自动匹配</span>
                      </div>
                      <svg v-if="autoMode" viewBox="0 0 14 14" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 7l3.5 3.5L12 3" /></svg>
                    </button>
                    <div v-if="jdkList.length" class="jdk-drop-divider"></div>
                    <button
                      v-for="jdk in jdkList"
                      :key="jdk.path"
                      class="jdk-opt"
                      :class="{ selected: !autoMode && activeJdk && activeJdk.path === jdk.path }"
                      @click="selectJdk(jdk)"
                    >
                      <div class="jdk-opt-main">
                        <span class="jdk-opt-title">{{ jdk.version }}</span>
                        <span class="jdk-opt-sub">{{ jdk.vendor }} · {{ jdk.bits }} 位</span>
                        <span class="jdk-opt-path">{{ jdk.path }}</span>
                      </div>
                      <svg v-if="!autoMode && activeJdk && activeJdk.path === jdk.path" viewBox="0 0 14 14" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 7l3.5 3.5L12 3" /></svg>
                    </button>
                    <div v-if="!jdkList.length" class="jdk-drop-empty">暂无扫描结果，点击右上角「刷新」</div>
                  </div>
                </Transition>
              </div>

              <button class="tool-btn" :disabled="loading" @click="refreshJdks">
                <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M14 8a6 6 0 01-11 3.5"/>
                  <polygon points="3,14 2.5,10 7,11" stroke="none" fill="currentColor"/>
                  <path d="M2 8a6 6 0 0111-3.5"/>
                  <polygon points="13,2 13.5,6 9,5" stroke="none" fill="currentColor"/>
                </svg>
                刷新
              </button>
              <button class="tool-btn" :disabled="loading" @click="selectCustomDir">
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                  <path d="M2 3a1 1 0 011-1h3l2 2h6a1 1 0 011 1v8a1 1 0 01-1 1H3a1 1 0 01-1-1V3z" />
                </svg>
                自定义目录
              </button>
            </div>

            <div v-if="loading" class="loading-hint">正在扫描...</div>
            <div v-else-if="jdkError" class="err">{{ jdkError }}</div>

            <div class="jdk-divider"></div>

            <!-- Java 高级项 -->
            <div class="launch-opts jdk-opts">
            <div class="lo-row lo-top">
              <label class="lo-label" for="lo-jvm">Java 虚拟机参数</label>
              <textarea
                id="lo-jvm"
                v-model="loJvmArgs"
                class="lo-textarea"
                rows="4"
                wrap="off"
                placeholder="每行一个 JVM 参数"
                spellcheck="false"
                @change="saveLaunchOptions"
              ></textarea>
            </div>

            <div class="lo-row lo-top">
              <label class="lo-label" for="lo-game">游戏参数</label>
              <textarea
                id="lo-game"
                v-model="loGameArgs"
                class="lo-textarea"
                rows="2"
                wrap="off"
                placeholder="每行一个游戏参数"
                spellcheck="false"
                @change="saveLaunchOptions"
              ></textarea>
            </div>

            <div class="lo-row">
              <label class="lo-label" for="lo-pre">启动前执行命令</label>
              <input
                id="lo-pre"
                v-model="loPreCmd"
                class="lo-input"
                type="text"
                placeholder="启动游戏前执行的命令"
                @change="saveLaunchOptions"
              />
            </div>

            <div class="lo-row">
              <label class="lo-label" for="lo-gc">内存管理</label>
              <select
                id="lo-gc"
                v-model="loGc"
                class="lo-select"
                @change="saveLaunchOptions"
              >
                <option v-for="g in gcOptions" :key="g.value" :value="g.value">{{ g.label }}</option>
              </select>
            </div>

            <div class="lo-checks">
              <label class="lo-check">
                <input v-model="loDisableWrapper" type="checkbox" @change="saveLaunchOptions" />
                <span>禁用 Java Launch Wrapper</span>
              </label>
              <label class="lo-check">
                <input v-model="loDisableAgent" type="checkbox" @change="saveLaunchOptions" />
                <span>禁用 LWJGL Unsafe Agent</span>
              </label>
              <label class="lo-check">
                <input v-model="loHighGpu" type="checkbox" @change="saveLaunchOptions" />
                <span>使用高性能显卡</span>
              </label>
            </div>
            </div>
          </div>
        </div>

        <!-- 下载设置：管理游戏下载源 -->
        <div v-else-if="currentTab === 'download'" key="download" class="stg-page">
          <div class="stg-card">
            <div class="stg-card-hdr">
              <h3>管理游戏下载源</h3>
              <div class="stg-card-sub">游戏文件与版本列表的下载来源、线程数与速度控制</div>
            </div>

            <!-- 文件下载源：点击展开选项 -->
            <div class="dl-row">
              <span class="dl-label">文件下载源</span>
              <div class="dl-sel" :class="{ open: dlFileOpen }">
                <button class="dl-sel-btn" @click="dlFileOpen = !dlFileOpen">
                  <span>{{ sourceModeLabel(dlFileMode) }}</span>
                  <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
                    <polygon points="3,4 9,4 6,9" />
                  </svg>
                </button>
                <div v-if="dlFileOpen" class="dl-sel-menu">
                  <button
                    v-for="opt in sourceModeOptions"
                    :key="opt.value"
                    class="dl-sel-item"
                    :class="{ active: dlFileMode === opt.value }"
                    @click="setFileMode(opt.value)"
                  >
                    <span class="dl-sel-item-label">{{ opt.label }}</span>
                    <span class="dl-sel-item-desc">{{ opt.desc }}</span>
                  </button>
                </div>
              </div>
            </div>

            <!-- 版本列表源 -->
            <div class="dl-row">
              <span class="dl-label">版本列表源</span>
              <div class="dl-sel" :class="{ open: dlManifestOpen }">
                <button class="dl-sel-btn" @click="dlManifestOpen = !dlManifestOpen">
                  <span>{{ sourceModeLabel(dlManifestMode) }}</span>
                  <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
                    <polygon points="3,4 9,4 6,9" />
                  </svg>
                </button>
                <div v-if="dlManifestOpen" class="dl-sel-menu">
                  <button
                    v-for="opt in sourceModeOptions"
                    :key="opt.value"
                    class="dl-sel-item"
                    :class="{ active: dlManifestMode === opt.value }"
                    @click="setManifestMode(opt.value)"
                  >
                    <span class="dl-sel-item-label">{{ opt.label }}</span>
                    <span class="dl-sel-item-desc">{{ opt.desc }}</span>
                  </button>
                </div>
              </div>
            </div>

            <!-- 社区下载源：Modrinth 官方 vs MCIM 镜像（管理 Fabric API 等社区资源） -->
            <div class="dl-row">
              <span class="dl-label">社区下载源</span>
              <div class="dl-sel" :class="{ open: dlCommunityOpen }">
                <button class="dl-sel-btn" @click="dlCommunityOpen = !dlCommunityOpen">
                  <span>{{ communitySourceOptions.find(o => o.value === dlCommunityMode)?.label ?? dlCommunityMode }}</span>
                  <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
                    <polygon points="3,4 9,4 6,9" />
                  </svg>
                </button>
                <div v-if="dlCommunityOpen" class="dl-sel-menu">
                  <button
                    v-for="opt in communitySourceOptions"
                    :key="opt.value"
                    class="dl-sel-item"
                    :class="{ active: dlCommunityMode === opt.value }"
                    @click="setCommunityMode(opt.value)"
                  >
                    <span class="dl-sel-item-label">{{ opt.label }}</span>
                    <span class="dl-sel-item-desc">{{ opt.desc }}</span>
                  </button>
                </div>
              </div>
            </div>

            <!-- 最大线程数（官方源并发；仅镜像模式由镜像源规定并锁定） -->
            <div class="dl-row">
              <span class="dl-label">最大线程数</span>
              <input
                class="dl-range"
                type="range"
                min="1"
                :max="concurrencyLocked ? MIRROR_CONCURRENCY : 256"
                step="1"
                v-model.number="dlConcurrency"
                @input="saveDownloadConfig"
                :disabled="concurrencyLocked"
              />
              <span class="dl-val">{{ concurrencyLocked ? MIRROR_CONCURRENCY : dlConcurrency }} / 256</span>
            </div>

            <!-- 速度限制：0 = 无上限 -->
            <div class="dl-row">
              <span class="dl-label">速度限制</span>
              <input
                class="dl-range"
                type="range"
                min="0"
                max="100"
                step="1"
                v-model.number="dlMaxSpeed"
                @input="saveDownloadConfig"
              />
              <span class="dl-val">{{ dlMaxSpeed === 0 ? "无上限" : dlMaxSpeed + " MB/s" }} / 100 MB/s</span>
            </div>

            <div v-if="dlError" class="dl-error">{{ dlError }}</div>
          </div>
        </div>

        <!-- 占位 -->
        <div v-else key="placeholder" class="stg-page">
          <div class="stg-card">
            <div class="stg-card-hdr">
              <h3>{{ placeholder }}</h3>
            </div>
          </div>
        </div>
      </Transition>
    </div>

    <!-- 选择皮肤种类 dialog -->
    <AlertDialog v-model="skinModelDialog" title="选择皮肤种类" :divider="false">
      <div class="model-dialog-body">
        <div class="model-row">
          <span class="model-name">Steve（经典模型）</span>
          手臂较粗（4px），经典 Steve 风格，大多数皮肤适用
        </div>
        <div class="model-row">
          <span class="model-name">Alex（纤细模型）</span>
          手臂较细（3px），纤细 Alex 风格，slim 皮肤专用
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn" @click="pickModel('steve')">Steve</button>
        <button class="dialog-btn" @click="pickModel('alex')">Alex</button>
        <button class="dialog-btn primary" @click="pickModel('auto')">启动器决定</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.stg { display: flex; height: 100%; }

/* ===== 左侧导航 ===== */
.stg-nav {
  flex: 0 0 180px; min-width: 160px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 12px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-divider {
  display: flex; align-items: center;
  padding: 8px 12px 6px; user-select: none;
}

.nav-divider span {
  font-size: 11px; font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase; letter-spacing: 0.8px;
}

.stg-tab {
  display: flex; align-items: center; gap: 8px;
  padding: 8px 12px;
  border: none; border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px; font-weight: 500;
  cursor: pointer; transition: all 0.15s;
  text-align: left;
}

.stg-tab:hover {
  color: var(--text-primary);
  background-color: var(--surface-hover);
}

.stg-tab.active {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* ===== 右侧内容 ===== */
.stg-body {
  flex: 1; position: relative;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  background: var(--surface-page);
}

.stg-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  padding: 20px 24px;
}

/* 多卡片上下间距 */
.stg-card + .stg-card {
  margin-top: 16px;
}

.stg-card-hdr {
  margin-bottom: 16px;
}

.stg-card-hdr h3 {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.stg-card-sub {
  font-size: 12px;
  color: var(--text-tertiary);
}

.stg-card-sub .stg-warn {
  margin-left: 6px;
  color: #d9534f; /* 柔和红，非刺眼 */
}

/* ===== JDK 工具栏 ===== */
.jdk-toolbar {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.jdk-picker-wrap {
  flex: 1;
  min-width: 0;
  position: relative;
}

.jdk-picker {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  cursor: pointer;
  transition: all 0.15s;
}

.jdk-picker:hover {
  border-color: var(--color-brand);
}

.jdk-picker.open {
  border-color: var(--color-brand);
  box-shadow: 0 0 0 3px var(--color-brand-bg);
}

.jdk-picker-main {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
}

.jdk-picker-main svg {
  color: var(--color-brand);
}

.jdk-picker-sub {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.jdk-chevron {
  flex-shrink: 0;
  color: var(--text-tertiary);
  transition: transform 0.15s;
}

.jdk-chevron.flip {
  transform: rotate(180deg);
}

.tool-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 16px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}

.tool-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.tool-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* 下拉列表 */
.jdk-dropdown {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  max-height: 280px;
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior-x: none;
  touch-action: pan-y;
  background: var(--surface-card);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(30, 27, 75, 0.12);
  z-index: 100;
  padding: 6px;
}

.jdk-opt {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}

.jdk-opt:hover {
  background: var(--surface-hover);
}

.jdk-opt.selected {
  background: var(--color-brand-bg);
}

.jdk-opt.selected .jdk-opt-title {
  color: var(--color-brand);
}

.jdk-opt-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.jdk-opt-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.jdk-opt-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}

.jdk-opt-path {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.jdk-opt > svg {
  flex-shrink: 0;
  color: var(--color-brand);
}

.jdk-drop-divider {
  height: 1px;
  background: var(--border-default);
  margin: 4px 8px;
}

.jdk-drop-empty {
  padding: 14px;
  text-align: center;
  font-size: 12px;
  color: var(--text-tertiary);
}

/* 状态提示 */
.loading-hint {
  color: var(--text-secondary);
  font-size: 13px;
  padding: 10px 0 0;
}

.err {
  color: #e74c3c;
  font-size: 13px;
  padding: 10px 0 0;
}

/* 下拉动画 */
.drop-enter-active { transition: all 0.15s ease-out; }
.drop-leave-active { transition: all 0.1s ease-in; }
.drop-enter-from { opacity: 0; transform: translateY(-6px); }
.drop-leave-to { opacity: 0; transform: translateY(-6px); }

/* 页面过渡 */
.page-enter-active, .page-leave-active { transition: all 0.15s ease-out; }
.page-enter-from { opacity: 0; transform: translateY(8px); }
.page-leave-to { opacity: 0; transform: translateY(-8px); }

/* ===== 内存分配 ===== */
.mem-slider {
  flex: 1;
  accent-color: var(--color-brand);
  cursor: pointer;
}

.mem-slider.disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.mem-slider-val {
  flex: none;
  width: 72px;
  font-size: 12.5px;
  color: var(--text-secondary);
  text-align: right;
}

.mem-slider-val.disabled {
  opacity: 0.45;
}

.mem-status {
  margin-top: 2px;
}

.mem-bar-labels {
  position: relative;
  height: 16px;
}

.mem-bar-label {
  position: absolute;
  /* 左对齐：文本起始紧贴色条左侧 */
  font-size: 10.5px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.mem-bar {
  display: flex;
  height: 10px;
  border-radius: 5px;
  overflow: hidden;
  background: var(--border-default);
}

.mem-seg {
  height: 100%;
  transition: width 0.4s ease;
}

.mem-used {
  background: #e74c3c;
}

.mem-game {
  background: #059669;
}

.mem-free {
  flex: 1;
}

.mem-total {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}

/* ===== Java 高级项 ===== */
.jdk-divider {
  height: 1px;
  background: var(--border-default);
  margin: 16px 0;
}

.lo-top {
  align-items: flex-start;
}

.lo-top .lo-label {
  padding-top: 6px;
}

.lo-textarea {
  flex: 1;
  min-width: 0;
  padding: 6px 8px;
  font-size: 12.5px;
  line-height: 1.6;
  font-family: inherit;
  color: var(--text-primary);
  background-color: var(--surface-input, var(--surface-card));
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
  resize: none;
  /* 参数文本不折行：超宽时横向滚动 */
  white-space: nowrap;
  overflow-x: auto;
  overflow-y: auto;
  transition: border-color 0.15s;
}

.lo-textarea:hover,
.lo-textarea:focus {
  border-color: var(--color-brand);
}

.lo-checks {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.lo-check {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-primary);
  cursor: pointer;
  user-select: none;
}

.lo-check input[type="checkbox"] {
  accent-color: var(--color-brand);
  cursor: pointer;
}

/* ===== 启动选项 ===== */
.launch-opts {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.lo-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 32px;
}

.lo-label {
  flex: none;
  width: 96px;
  font-size: 13px;
  color: var(--text-secondary);
}

.lo-select {
  flex: 1;
  height: 30px;
  padding: 0 8px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-input, var(--surface-card));
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
  cursor: pointer;
  transition: border-color 0.15s;
}

.lo-select:hover,
.lo-select:focus {
  border-color: var(--color-brand);
}

.lo-input {
  flex: 1;
  height: 30px;
  padding: 0 8px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-input, var(--surface-card));
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
  transition: border-color 0.15s;
}

.lo-input:hover,
.lo-input:focus {
  border-color: var(--color-brand);
}

.lo-winsize {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
}

.lo-winsize .lo-select {
  flex: none;
  width: 140px;
}

.lo-num {
  flex: 1;
  min-width: 0;
}

.lo-times {
  flex: none;
  color: var(--text-tertiary);
  font-size: 13px;
}

/* ===== 离线皮肤 ===== */
/* 横向 chip 选项：关键文字 + 圆形选择器 */
.skin-opts {
  display: flex;
  gap: 8px;
}

.skin-opt {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 14px;
  border: 1px solid var(--border-default);
  border-radius: 999px;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.15s;
}

.skin-opt:hover {
  border-color: var(--color-brand);
}

.skin-opt.selected {
  border-color: var(--color-brand);
  background: var(--color-brand-bg);
}

.skin-opt input[type="radio"] {
  accent-color: var(--color-brand);
  width: 14px;
  height: 14px;
  cursor: pointer;
  flex: none;
  margin: 0;
}

.skin-opt-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.skin-opt.selected .skin-opt-label {
  font-weight: 600;
  color: var(--color-brand);
}

/* 选中后的详细信息 */
.skin-detail {
  margin-top: 14px;
}

.skin-desc {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.6;
}

.skin-sub-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 14px;
  flex-wrap: wrap;
}

.skin-input {
  flex: 1;
  min-width: 180px;
  padding: 8px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}

.skin-input:focus {
  border-color: var(--color-brand);
}

.skin-model-row {
  display: flex;
  gap: 16px;
}

.skin-model-row label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
}

.skin-model-row label.selected {
  color: var(--text-primary);
}

.skin-model-note {
  font-size: 12px;
  color: var(--text-tertiary);
}

.skin-player-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 14px;
}

.skin-player-row .skin-input {
  flex: 1;
  min-width: 180px;
  margin: 0;
}

/* 轻提示 toast */
.toast {
  position: fixed;
  left: 50%;
  bottom: 36px;
  transform: translateX(-50%);
  z-index: 2000;
  max-width: 480px;
  padding: 8px 18px;
  font-size: 13px;
  color: #fff;
  background: rgba(24, 24, 27, 0.92);
  border: 1px solid var(--border-default);
  border-radius: 999px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 模型选择 dialog */
.model-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.model-row {
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.model-name {
  display: block;
  font-weight: 600;
  color: var(--text-primary);
}

.dialog-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.dialog-btn.primary {
  background: var(--color-brand);
  border-color: var(--color-brand);
  color: #fff;
}

.dialog-btn.primary:hover {
  opacity: 0.88;
}

/* ── 下载设置 ── */
.dl-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 9px 0;
}

.dl-label {
  flex: 0 0 88px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
}

/* 点击展开的选择器 */
.dl-sel {
  position: relative;
  flex: 1;
  min-width: 0;
}
.dl-sel-btn {
  width: 100%;
  height: 36px;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 0 12px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: border-color 0.15s;
}
.dl-sel-btn > span {
  /* 固定行高并单行省略：中英混排文本在 CJK 字体下 line box 度量不同，
     会撑高 flex 按钮导致切换选中项时高度抖动 */
  line-height: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}
.dl-sel-btn:hover { border-color: var(--color-brand); }
.dl-sel-btn svg {
  color: var(--text-tertiary);
  transition: transform 0.2s ease;
}
.dl-sel.open .dl-sel-btn svg { transform: rotate(180deg); }

.dl-sel-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 10;
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  overflow: hidden;
}
.dl-sel-item {
  width: 100%;
  height: 52px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 3px;
  padding: 0 12px;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
}
.dl-sel-item + .dl-sel-item {
  border-top: 1px solid var(--border-default);
}
.dl-sel-item:hover { background-color: var(--surface-hover); }
.dl-sel-item.active .dl-sel-item-label { color: var(--color-brand); }
.dl-sel-item-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  /* 固定行高：中英混排的 line box 差异会撑高按钮 */
  line-height: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
.dl-sel-item-desc {
  font-size: 11.5px;
  color: var(--text-tertiary);
  line-height: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

/* 滑块（进度条） */
.dl-range {
  flex: 1;
  min-width: 0;
  accent-color: var(--color-brand);
}
.dl-val {
  flex: 0 0 auto;
  min-width: 86px;
  font-size: 12px;
  color: var(--text-secondary);
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.dl-check {
  margin-top: 6px;
  padding-top: 10px;
  border-top: 1px solid var(--border-default);
}
.dl-error {
  margin-top: 8px;
  font-size: 12px;
  color: #e74c3c;
}
</style>
