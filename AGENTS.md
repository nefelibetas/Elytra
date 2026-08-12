# Repository Guidelines

## Project Overview

ElytraLauncher — 基于 **Tauri v2（Rust 后端）+ Vue 3（TypeScript 前端）** 的 Minecraft 启动器。功能：游戏版本下载（官方/国内镜像多源）、ModLoader 安装（Fabric/Forge/NeoForge/OptiFine；Quilt 暂复用 Fabric 流程待修，LiteLoader 仅版本查询）、微软正版登录（OAuth PKCE）、离线模式与离线皮肤（对齐 PCL2 机制）、JDK 自动扫描、下载任务管理、启动参数构建与游戏进程管理。

后端 61 个 `#[tauri::command]`（32 async / 29 sync），前端 6 个页面，无状态管理库（模块级 `reactive()` 单例）。

## Architecture & Data Flow

```
前端 Vue3 (src/)  ──invoke(snake_case)──▶  commands/（薄胶水层）
    ▲                                          │ 读 persist 配置、调领域函数
    │ task-progress / ms-login-complete        ▼
    │ 事件推送                      downloader/（下载流水线）auth/（微软登录）
    │                                config/（纯数据模型）task/（TaskManager）
    └────────────── 返回 Result<T, String> 或裸值
```

- **后端分层**：`commands/` 只做胶水（读配置 → 调领域函数 → `map_err` 展平 String）；领域逻辑按域归位——下载编排在 `task/runner.rs`，启动在 `config/launcher/`（`args.rs` 命令构建 / `env.rs` 环境准备 / `skin.rs` 离线皮肤），账号在 `auth/`（`microsoft.rs` OAuth / `mojang.rs` 官方 API / `account.rs` 回调与披风上传），下载引擎 `downloader/`，纯数据 `config/`；统一持久化 `persist/`（exe 旁 `Elytra/Settings.toml`，TOML 分 section，全局 Mutex 串行化）；任务由 `task/` TaskManager 单例（`tauri::async_runtime::spawn` + `task-progress` 事件推送）；跨域小工具 `util.rs`（纯函数）
- **前端数据流**：`invoke` 返回 taskId → 预注册任务 → 后端推 `task-progress` 事件增量更新 `dlState` → done 时触发回调并移除；`memState` 每 2s 轮询内存
- **配置联动**：`gamedirs::set_active_game_dir` 会写 downloader section 的 `minecraft_dir`，launch/downloader 都读该字段
- **下载源模式三个**：`file_source_mode`（client/libraries/assets/natives/ModLoader）、`manifest_source_mode`（版本清单）、`community_source_mode`（Modrinth 官方 vs MCIM 镜像，管 Fabric API 版本列表与下载；`is_mirror_url` 按 URL 判定镜像并限制并发）。natives 新旧两代：旧式（≤1.12，lwjgl 2.x）由 `process_natives` 下载 classifier JAR、启动前 `ensure_old_style_natives` 解压到 `.minecraft/natives/<ver>/`（`.elytra-ready` marker 标记完成）并设 `-Djava.library.path`；新式（1.13+）native JAR 作为普通库进 classpath，仅需创建 natives 目录，LWJGL 自行提取

## Key Directories

|路径|职责|
|---|---|
|`src-tauri/src/commands/`|8 个命令模块：jdk / downloader / loader / task / account / gamedirs / launch / mods（真薄胶水；`launch.rs` 额外含进程管理，编排均已下沉）|
|`src-tauri/src/config/`|纯数据模型：`launcher/`（`model.rs` 配置 + `args.rs` 命令构建（`collect_jvm_args`/`build_game_args` 拆分）+ `env.rs` 环境准备 + `memory.rs`（GC 参数 + `system_memory_mb()` 系统内存统一入口，供启动显示与自动分配共用）+ `platform.rs` + `skin.rs` 离线皮肤）、`downloader/model.rs`（多源与回退，`source.rs` 为 DownloaderConfig 网络 impl 分离文件：`fetch_with_fallback`/`fetch_json_with_fallback` 主源 4s 超时降级 + **按主源 URL 的内存 TTL 缓存 10 分钟**（maven-metadata/loader 列表等低频变化数据））、`offline_skin.rs`、`game_dirs.rs`|
|`src-tauri/src/downloader/`|下载引擎：manifest / version / library / assets / fetcher / hash / model / extract / mods / mod_loader（`TARGET_OS`、`matches_rules`/`native_classifier_matches`/`os_classifier_key` 平台判定与规则过滤的唯一实现、`extract_jar`/`extract_entry_to` ZIP 解压）|
|`src-tauri/src/downloader/mods.rs`|Mod 双源搜索域（Modrinth/CurseForge）：`search_mods(source, query, game_version, loader, category, offset, page_size)`——`category` 为项目类型（shader=6552/resourcepack=12/datapack=6945/modpack=4471 映射 CurseForge classId + Modrinth project_type facet），`page_size` 每源条数（聚合模式前端传 15）|
|`src-tauri/src/downloader/mod_loader/`|ModLoader 引擎：`mod.rs`（公共纯函数 `maven_path`/`merge_loader_profile`/`expand_inheritance` + 事件钩子 LoaderEvent/LoaderHook + re-export）、`fabric.rs`（profile 式 Fabric/Quilt）、`forge.rs`（installer 式共用流程 `download_and_extract` + Forge 专属 + 共享工具）、`neoforge.rs`（installer 式 NeoForge 专属）、`optifine.rs`、`version_query.rs`（版本列表查询：maven XML 解析 + 语义化排序 + Fabric API）|
|`src-tauri/src/auth/`|微软 OAuth PKCE（`microsoft.rs`）+ 官方 API 客户端（`mojang.rs`：玩家查询/皮肤纹理）+ 账号服务（`account.rs`：登录回调/披风/上传）+ MsAccount（`model.rs`）|
|`src-tauri/src/jdk/`|JDK 扫描/版本解析/存储（`scanner.rs`/`model.rs`/`mod.rs`：`java_bin_path`/`parse_java_major`）|
|`src-tauri/src/persist/`|TOML section 存取（`read<T>/save<T>/remove`）|
|`src-tauri/src/task/`|`mod.rs`（TaskManager 单例 + PathLock 路径互斥 + `pub use model::*` 门面）+ `model.rs`（任务规格 + TaskProgress/StepInfo/TaskStatus 进度模型）+ `steps.rs`（StepSpec/步骤分析）+ `runner.rs`（run_task 编排，`resolve_target_dir`/`init_local_scan` 拆段）+ `repair.rs`（scan_missing/run_repair_task，`repair_step_indices`/`repair_jar` 拆段）+ `progress.rs`（EmitLoop 心跳/进度上报）+ `files.rs`（批量文件下载任务）|
|`src-tauri/src/util.rs`|跨域公共纯函数：`url_file_name`（URL 取文件名）、`mc_version_from_dir`（版本目录名 → MC 版本，含 loader 标记定位与 `{id}_n` 剥离）|
|`src/pages/`|Launch（启动页容器：左列进度/账号/启动区编排 + 右列占位，业务在 store/组件）、Download（下载容器：仅导航+tab 编排，业务在 store/组件）、Settings（设置容器：仅导航+tab 编排，业务在 store/组件）、DownloadDetail（任务管理）、VersionSelect（版本选择容器：左目录+右列表编排，业务在 store/gameDirs 与组件）、VersionSetup（版本设置容器：仅导航+tab 编排，业务在组件）|
|`src/store/`|`download.ts`（dlState）、`memory.ts`（memState）、`modSearch.ts`（Mod 搜索聚合/分页/热门预加载；导出 `searchAllSources`/`cleanText`/`toDisplayItem` 供分类复用）、`categorySearch.ts`（分类资源搜索工厂 `createCategorySearch(category)` 生成 shader/resourcepack/datapack/modpack 四个独立实例 + TotalPages）、`versionDownload.ts`（vdState：manifest/版本选择/loader/Fabric API/提交下载，状态跨路由保留）、`launch.ts`（启动流程/已装版本/游戏退出监听）、`accountUi.ts`（账号菜单/皮肤/披风）、`auth.ts`（账号模式/微软登录流程/事件监听，含模块级 watch(msAccount) 头像联动）、`avatar.ts`（正版+离线头像渲染）、`alert.ts`（全局错误弹窗）、`selectedVersion.ts`（版本选择 localStorage 单例）、`jdkConfig.ts`（JDK 扫描/选择）、`launchOptions.ts`（启动选项/内存）、`offlineSkinConfig.ts`（离线皮肤）、`downloaderConfig.ts`（下载源/线程/限速）、`gameDirs.ts`（版本选择页：目录增删改/激活/右键菜单 + 版本列表）——模块级 reactive/ref 单例；`useConfigSection`（utils）可在 store 模块顶层调用（纯 ref，无生命周期依赖）|
|`src/component/`|TitleBar / DownloadFAB / DownloadTab（hover 刷新按钮，emit refresh）/ VersionCard / VersionItem / AlertDialog / DialogModal / ScrollTopFab（长列表滚动回顶，自动绑最近滚动祖先，右下 88px 避让全局 DownloadFAB）/ VanillaList（原版版本列表页）/ VanillaDetail（版本详情+loader 选择页）/ ModSearchBar / ModResultList / ShaderSearchBar / ShaderResultList / ResourcepackSearchBar / ResourcepackResultList / DatapackSearchBar / DatapackResultList / ModpackSearchBar / ModpackResultList（社区资源搜索：搜索表单 + 结果卡片，四分类复制自 Mod 版改绑 store 与文案）/ IconImage（图标占位组件：Image() 探针驱动 + 6s 超时重试 2 次 + 成功后占位 svg 从 DOM 移除——透明 PNG 透出占位的坑）/ LaunchProgress（启动进度面板）/ AccountSection（正版/离线切换+用户区+菜单）/ LaunchSection（启动按钮区）/ CapeDialog（披风选择）/ SettingsOfflineSkin / SettingsLaunchOptions（启动选项+内存）/ SettingsJdk（Java+高级项）/ SettingsDownload（下载源）/ GameDirList（版本选择左列：目录+右键菜单）/ GameVersionList（版本选择右列：分类卡片）/ VersionSetupOverview（版本设置概览：快捷方式/补全/导出/删除）——页面组件直接 import store，无 props 链；Settings 系列组件用非 scoped style @import `src/styles/settings-shared.css`（stg-/lo- 前缀类全局唯一）|
|`src/utils/`|versionClassify（目录名 `{mc}-{loader}-{ver}` 解析）、versionIcons、format（fmtCount/fmtSize/fmtModDate/fmtDateTime 收口）|

## Development Commands

```bash
pnpm tauri dev      # 主开发命令：vite :1420 + Rust dev 窗口（Rust 改动需手动重启才生效）
pnpm tauri build    # 发布构建（vue-tsc + vite + cargo release + bundle）
pnpm dev            # 仅 vite dev server（:1420 strictPort）
pnpm build          # vue-tsc --noEmit（全量类型检查）+ vite build → dist/
cd src-tauri && cargo check    # Rust 编译检查
cd src-tauri && cargo clippy   # Rust lint
cd src-tauri && cargo test --lib -- --test-threads=1   # 单元测试（77 个；并行会偶发竞争——persist 共用 Settings.toml + 网络测试抖动）
```

**测试红线**：`download_snapshot_full` 已标 `#[ignore]`（真实下载数百 MB 到 `D:\download_test`），`cargo test` 全量默认不触发；显式 `cargo test --test downloader_test -- --ignored` 才跑。`fetch_manifest_works` 需网络（单发小请求），离线环境会失败。

## Code Conventions & Common Patterns

### Rust 后端
- **命令命名**：snake_case、动词开头（`get_/save_/set_/fetch_/scan_/launch_/spawn_/start_/cancel_/remove_`…）
- **返回**：可失败 → `Result<T, String>`；纯读取 → 裸 `T`/`Option<T>`；错误在模块边界 `.map_err(|e| e.to_string())`
- **async 分布**：网络/IO 重命令 async；配置读写/本地小操作同步
- **注册**：新命令 = `commands/<file>.rs` 加 `#[tauri::command] pub fn`，再在 `lib.rs` 的 `generate_handler![commands::<file>::<fn>, ...]` 追加**全限定路径**——`commands/mod.rs` 只有 `pub mod`，re-export 不能代替
- **错误消息**：中文，约定"首行短文案 + `\n` 详细技术信息"
- **持久化**：配置一律 `persist::read::<T>("section").unwrap_or_default()`；结构体实现 `Default`；`save` 前读改写（全局锁串行化）
- **下载**：`fetch_with`（3 次重试+退避）/`fetch_fallback`（多源 URL 列表）/`fetch_json_with_fallback`（主源 4s 超时降级 + 10 分钟 TTL 缓存）；`file_sha1_matches` 整文件读内存；全局 `http()` client 带 2s connect_timeout（镜像 302 到不可达对象存储节点的防挂死）
- **版本目录**：`versions/{id}` 冲突时 `{id}_{n}`；目录内 jar/json 文件名可能 ≠ 目录名，一律扫描兜底

### 前端 Vue/TS
- **IPC**：命令名与参数/返回值字段全 snake_case；**TS interface 手写镜像 Rust struct，无自动生成**——改 Rust 字段必须同步前端
- **跨组件缓存必须放普通 `<script lang="ts">` 块**——`<script setup>` 顶层变量每次组件重建（路由切换）重置（Launch.vue 头像缓存、Download.vue manifest 缓存为范例）
- **状态**：模块级 `reactive()` 单例导出，页面直接 import；无 Pinia/Vuex
- **事件监听幂等**：`ensureListen()` 用 listening 标志防重复注册
- **样式**：CSS 变量统一在 App.vue `:root`（`--color-brand`/`--text-*`/`--surface-*`/`--radius-*` 等），组件页面一律 `var()` 引用、`<style scoped>`；图标用内联 SVG（`fill=currentColor`），无图标库
- **错误提示两级**：toast（`window.__showToast`，2.5s）vs AlertDialog（复杂错误，带复制按钮）
- **展开/收起必须有动画**：一律用 `<Transition name="expand">` + `.expand-enter-to/.expand-leave-from { max-height: 1000px }` 显式目标值；`max-height` 写在 active 类上会因同特异性覆盖 from 态导致**无动画**（Download.vue loader/mod 展开的既有坑，勿回退）
- **社区资源搜索（Mod/光影/资源包/数据包/整合包）**：搜索表单 + 结果列表组件成对复制（绑各自 store），无 props 链；聚合模式（来源=全部）每源只取 `AGG_PAGE_SIZE=15` 条（页大小减半控图片量），合并按标题去重后展示 ≤15 条；进入下载页 onMounted 并发预加载全部社区 tab 热门（`ensureAutoSearch` 幂等）；左侧 Tab 刷新按钮 = 各 store 的 `reset()`（清条件重搜热门）
- **路由**：hash 路由（`createWebHashHistory`），`meta.transition` 驱动页面动画

## Important Files

|文件|说明|
|---|---|
|`src-tauri/src/lib.rs`|入口：`task::init()`、后台预扫 JDK、**唯一** generate_handler! 注册点（61 命令）|
|`src-tauri/src/main.rs`|仅 `elytra_lib::run()`|
|`src-tauri/src/commands/launch.rs`|启动命令 + 进程管理（spawn/日志/优先级/GPU/内存优化）；环境准备 `config/launcher/env.rs`、离线皮肤 `config/launcher/skin.rs`、命令构建 `config/launcher/args.rs`|
|`src-tauri/src/config/launcher/args.rs`|`${key}` 占位符替换、rules/features 匹配、classpath、JDK22+ 标志|
|`src-tauri/src/config/downloader/model.rs`|多源 URL 模板、`mirror_url` BMCLAPI 重写、过慢降级|
|`src-tauri/src/auth/microsoft.rs`|微软 OAuth（授权码/refresh/XBL/XSTS/MC login）、xuid 提取、回调服务 127.0.0.1:27205|
|`src-tauri/src/persist/store.rs`|配置文件路径 = `current_exe 目录/Elytra/Settings.toml`（非标准配置目录）|
|`src-tauri/src/task/mod.rs`|TaskManager 单例；取消时 `cleanup_path` 整删（慎用 set_cleanup_path）|
|`src-tauri/tauri.conf.json`|窗口 **840×540**、无边框透明、`beforeDevCommand: pnpm dev`、devUrl :1420|
|`src-tauri/capabilities/default.json`|main 窗口权限：core:default + window 控制子集 + opener + dialog|
|`src/store/download.ts`|下载任务状态机（事件驱动核心）|

## Runtime/Tooling Preferences

- **包管理器必须 pnpm**：`beforeDevCommand` 硬编码 `pnpm dev`；注意 `pnpm-lock.yaml` 被 gitignore（依赖版本不锁定）
- **工具链很新**：vite 8、typescript 6、vue-tsc 3、vue-router 5——新增依赖前确认兼容
- **窗口**：840×540、`decorations:false` + `transparent:true` + `resizable:false`——前端自绘 TitleBar（`startDragging`）
- **Rust 变更需重启 `pnpm tauri dev`**（vite `watch.ignored` 忽略 src-tauri）；前端改动 HMR 即时生效
- **reqwest 0.13 无 `.form()`**：表单需手工 urlencode（auth 里 `form_body`）
- **MS_CLIENT_ID 固定 PrismLauncher 的**（自建 Azure 应用 2025 起 403）；回调端口 27205 写死
- **lib 名 `elytra_lib`**：Windows 下 bin/lib 同名会 Cargo 8519，保留 `_lib` 后缀
- **版本号两处**：tauri.conf.json（1.0.0）与 Cargo.toml（0.1.0）不一致，改动需同步
- **capabilities 是 v2 权限门**：新增 JS 侧 window/fs/dialog API 必须先在 `default.json` 加权限
- 新增依赖前评估：库名、用途、更轻量替代，确认后再引入
- 敏感信息（token/client secret）不硬编码、不打印日志、不提交 git

## Testing & QA

- **无前端测试**；Rust 侧 77 个单元测试，全部 `#[test]`（网络型用 `#[tokio::test]`）
- **单元测试只测纯函数**：PKCE、URL 组装、启动命令构建（`build_launch_command` + `minimal_version_json()` 助手）、离线皮肤 UUID 算法（与 Java `UUID.hashCode() & 1` 一致性）、内存分配公式、util（URL 文件名/版本目录解析）、maven XML 解析与语义化排序、Mod 双源搜索、修复步骤规划、persist 读写回环
- **断言风格**：`assert_eq!` 精确值为主（argv 位置、公式值、错误码消息）；行为不变量用 `assert!` + 中文 message
- **平台隔离**：Windows 专属逻辑（BAT 脚本渲染）用 `#[cfg(target_os = "windows")]`
- **persist 测试隔离**：测试进程下配置落在 `target/debug/deps/Elytra/Settings.toml`，与正式运行隔离
- **安全命令**：`cargo test --lib`（无网络、无大写入）——日常验证用这个；`cargo test` 全量同样安全（下载测试已 ignore）；**并行跑会偶发竞争**（persist 共用 Settings.toml + 网络测试抖动），日常用 `-- --test-threads=1` 串行化
- **危险命令**：`cargo test --test downloader_test -- --ignored`——真实下载最新 snapshot 数百 MB 到 `D:\download_test`，避免
- 集成测试无 mock/本地 fixture，走真实官方源；`downloader/hash.rs` 与 version JSON 解析目前无单测（仅被集成测试间接覆盖）

## Known Debt（已知遗留，待处理）

- **Quilt 实现有错（用户自修，规划中）**：`task/runner.rs` 中 `"quilt"` 复用 `install_fabric`（Quilt 会装错）；`install_optifine` 生成的 version.json 用 `inheritsFrom` + `file://` URL，与 `mainClass: launchwrapper` 兼容性存疑
- **已解决**：`installer.rs` 的 24 个 clippy warning 与 `.ok().expect()` 随 ModLoader 重构（`mod_loader/` 拆分）一并清理；`resolve_jar` Strategy 4 的 ponytail 残留注释已消除（改用 `util::mc_version_from_dir`）
- **待产品决策**：`GcType::Custom` 语义为"不追加 GC 参数（用 JVM 默认）"，前端 label 已注明，行为符合设计（未来可扩展为自定义参数输入框）
- **已解决**：`LauncherVisibility`（启动器可见性）2026-08 已删除（前后端同步移除）；`OfflineSkinModel::Auto` 已实现 PNG 高度自动识别（32px→slim）

## Gotchas

1. `generate_handler!` 路径必须与模块树一致，`commands/mod.rs` 无 re-export
2. 同步命令的阻塞操作（`remove_dir_all`、rename、explorer）跑在主线程——重活改 async/`spawn_blocking`
4. 平台判定统一走 `downloader/extract.rs` 的 `TARGET_OS`（cfg 编译期）与 `matches_rules`/`native_classifier_matches`/`os_classifier_key`——下载过滤（library.rs）与启动 classpath（launcher/args.rs）共用，新增平台要在 extract.rs 一处改
5. `task::init()` 必须在 `run()` 开头调用，否则 `task::global()` panic
6. 事件名 `task-progress` / `ms-login-complete` / `game-exited` 前后端字符串要一致
7. App.vue 拦截 F12/DevTools/全选/Tab/ESC 快捷键——调试注意
8. 页面已全部容器化（业务在 store/组件，单文件 ≤134 行），改动前先 grep 定位组件/store 归属
9. 版本选择持久化：localStorage 双键 `elytra.selectedVersion` + `elytra.selectedVersionDir`
10. AGENTS.md 本身被 `.gitignore` 忽略（本地文件）；README.md 是空文件
