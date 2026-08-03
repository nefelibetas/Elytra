# Repository Guidelines

## Project Overview

ElytraLauncher — 基于 **Tauri v2（Rust 后端）+ Vue 3（TypeScript 前端）** 的 Minecraft 启动器。功能：游戏版本下载（官方/国内镜像多源）、ModLoader 安装（Fabric/Forge/NeoForge/OptiFine；Quilt 暂复用 Fabric 流程待修，LiteLoader 仅版本查询）、微软正版登录（OAuth PKCE）、离线模式与离线皮肤（对齐 PCL2 机制）、JDK 自动扫描、下载任务管理、启动参数构建与游戏进程管理。

后端 54 个 `#[tauri::command]`（26 async / 28 sync），前端 6 个页面，无状态管理库（模块级 `reactive()` 单例）。

## Architecture & Data Flow

```
前端 Vue3 (src/)  ──invoke(snake_case)──▶  commands/（薄胶水层）
    ▲                                          │ 读 persist 配置、调领域函数
    │ task-progress / ms-login-complete        ▼
    │ 事件推送                      downloader/（下载流水线）auth/（微软登录）
    │                                config/（纯数据模型）task/（TaskManager）
    └────────────── 返回 Result<T, String> 或裸值
```

- **后端分层**：`commands/` 只做胶水（读配置 → 调领域函数 → `map_err` 展平 String）；领域逻辑按域归位——下载编排在 `task/runner.rs`，启动在 `config/launcher/`（`args.rs` 命令构建 / `env.rs` 环境准备 / `skin.rs` 离线皮肤），账号在 `auth/`（`microsoft.rs` OAuth / `mojang.rs` 官方 API / `account.rs` 回调与披风上传），下载引擎 `downloader/`，纯数据 `config/`；统一持久化 `persist/`（exe 旁 `Elytra/Settings.toml`，TOML 分 section，全局 Mutex 串行化）；任务由 `task/` TaskManager 单例（`tauri::async_runtime::spawn` + `task-progress` 事件推送）
- **前端数据流**：`invoke` 返回 taskId → 预注册任务 → 后端推 `task-progress` 事件增量更新 `dlState` → done 时触发回调并移除；`memState` 每 2s 轮询内存
- **配置联动**：`gamedirs::set_active_game_dir` 会写 downloader section 的 `minecraft_dir`，launch/downloader 都读该字段
- **下载链路**：manifest → version JSON/client jar → libraries → assets → natives；多源回退（主源 5s 超时降级"过慢"）+ sha1 校验 + 限速 + 并发。natives 新旧两代：旧式（≤1.12，lwjgl 2.x）由 `process_natives` 下载 classifier JAR、启动前 `ensure_old_style_natives` 解压到 `.minecraft/natives/<ver>/`（`.elytra-ready` marker 标记完成）并设 `-Djava.library.path`；新式（1.13+）native JAR 作为普通库进 classpath，仅需创建 natives 目录，LWJGL 自行提取

## Key Directories

| 路径 | 职责 |
|---|---|
| `src-tauri/src/commands/` | 8 个命令模块：jdk / downloader / loader / installer / task / account / gamedirs / launch（真薄胶水；`launch.rs` 额外含进程管理，编排均已下沉） |
| `src-tauri/src/config/` | 纯数据模型：`jdk/`（JavaEnv/scanner/storage）、`launcher/`（`model.rs` 配置 + `args.rs` 命令构建 + `env.rs` 环境准备 + `skin.rs` 离线皮肤）、`downloader/model.rs`（多源与回退）、`offline_skin.rs`、`game_dirs.rs` |
| `src-tauri/src/downloader/` | 下载引擎：manifest / version / library / assets / installer / fetcher / hash / model / extract（`TARGET_OS`、`matches_rules`/`native_classifier_matches`/`os_classifier_key` 平台判定与规则过滤的唯一实现、`extract_jar` natives 解压；`source.rs` 是空壳，URL 逻辑在 config 侧） |
| `src-tauri/src/auth/` | 微软 OAuth PKCE（`microsoft.rs`）+ 官方 API 客户端（`mojang.rs`：玩家查询/皮肤纹理）+ 账号服务（`account.rs`：登录回调/披风/上传）+ MsAccount（`model.rs`） |
| `src-tauri/src/persist/` | TOML section 存取（`read<T>/save<T>/remove`） |
| `src-tauri/src/task/` | TaskManager 单例 + 进度事件（`mod.rs`）+ 下载任务编排（`runner.rs`：run_task / run_repair_task / EmitLoop） |
| `src/pages/` | Launch（启动/登录/头像）、Download（下载/loader 选择）、Settings（Java/启动选项/离线皮肤/下载源）、DownloadDetail（任务管理）、VersionSelect（版本选择）、VersionSetup（版本设置） |
| `src/store/` | `download.ts`（dlState）、`memory.ts`（memState）——模块级 reactive 单例 |
| `src/component/` | TitleBar / DownloadFAB / DownloadTab / VersionCard / VersionItem / AlertDialog / DialogModal |
| `src/utils/` | versionClassify（目录名 `{mc}-{loader}-{ver}` 解析）、versionIcons |

## Development Commands

```bash
pnpm tauri dev      # 主开发命令：vite :1420 + Rust dev 窗口（Rust 改动需手动重启才生效）
pnpm tauri build    # 发布构建（vue-tsc + vite + cargo release + bundle）
pnpm dev            # 仅 vite dev server（:1420 strictPort）
pnpm build          # vue-tsc --noEmit（全量类型检查）+ vite build → dist/
cd src-tauri && cargo check    # Rust 编译检查
cd src-tauri && cargo clippy   # Rust lint
cd src-tauri && cargo test --lib   # 单元测试（安全，27 个）
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
- **下载**：`fetch_with`（3 次重试+退避）/`fetch_fallback`（多源 URL 列表）/`fetch_json_with_fallback`（主源 5s 超时降级）；`file_sha1_matches` 整文件读内存
- **版本目录**：`versions/{id}` 冲突时 `{id}_{n}`；目录内 jar/json 文件名可能 ≠ 目录名，一律扫描兜底

### 前端 Vue/TS
- **IPC**：命令名与参数/返回值字段全 snake_case；**TS interface 手写镜像 Rust struct，无自动生成**——改 Rust 字段必须同步前端
- **跨组件缓存必须放普通 `<script lang="ts">` 块**——`<script setup>` 顶层变量每次组件重建（路由切换）重置（Launch.vue 头像缓存、Download.vue manifest 缓存为范例）
- **状态**：模块级 `reactive()` 单例导出，页面直接 import；无 Pinia/Vuex
- **事件监听幂等**：`ensureListen()` 用 listening 标志防重复注册
- **样式**：CSS 变量统一在 App.vue `:root`（`--color-brand`/`--text-*`/`--surface-*`/`--radius-*` 等），组件页面一律 `var()` 引用、`<style scoped>`；图标用内联 SVG（`fill=currentColor`），无图标库
- **错误提示两级**：toast（`window.__showToast`，2.5s）vs AlertDialog（复杂错误，带复制按钮）
- **路由**：hash 路由（`createWebHashHistory`），`meta.transition` 驱动页面动画

## Important Files

| 文件 | 说明 |
|---|---|
| `src-tauri/src/lib.rs` | 入口：`task::init()`、后台预扫 JDK、**唯一** generate_handler! 注册点（67 命令） |
| `src-tauri/src/main.rs` | 仅 `elytra_lib::run()` |
| `src-tauri/src/commands/launch.rs` | 启动命令 + 进程管理（spawn/日志/优先级/GPU/内存优化）；环境准备 `config/launcher/env.rs`、离线皮肤 `config/launcher/skin.rs`、命令构建 `config/launcher/args.rs` |
| `src-tauri/src/config/launcher/args.rs` | `${key}` 占位符替换、rules/features 匹配、classpath、JDK22+ 标志 |
| `src-tauri/src/config/downloader/model.rs` | 多源 URL 模板、`mirror_url` BMCLAPI 重写、过慢降级 |
| `src-tauri/src/auth/microsoft.rs` | 微软 OAuth（授权码/refresh/XBL/XSTS/MC login）、xuid 提取、回调服务 127.0.0.1:27205 |
| `src-tauri/src/persist/store.rs` | 配置文件路径 = `current_exe 目录/Elytra/Settings.toml`（非标准配置目录） |
| `src-tauri/src/task/mod.rs` | TaskManager 单例；取消时 `cleanup_path` 整删（慎用 set_cleanup_path） |
| `src-tauri/tauri.conf.json` | 窗口 **840×540**、无边框透明、`beforeDevCommand: pnpm dev`、devUrl :1420 |
| `src-tauri/capabilities/default.json` | main 窗口权限：core:default + window 控制子集 + opener + dialog |
| `src/store/download.ts` | 下载任务状态机（事件驱动核心） |

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

- **无前端测试**；Rust 侧 27 个单元测试 + 2 个集成测试，全部 `#[test]`（集成用 `#[tokio::test]`）
- **单元测试只测纯函数**：PKCE、URL 组装、启动命令构建（`build_cmd` + `minimal_version_json()` 助手）、离线皮肤 UUID 算法（与 Java `UUID.hashCode() & 1` 一致性）、内存分配公式、修复步骤规划、persist 读写回环
- **断言风格**：`assert_eq!` 精确值为主（argv 位置、公式值、错误码消息）；行为不变量用 `assert!` + 中文 message
- **平台隔离**：Windows 专属逻辑（BAT 脚本渲染）用 `#[cfg(target_os = "windows")]`
- **persist 测试隔离**：测试进程下配置落在 `target/debug/deps/Elytra/Settings.toml`，与正式运行隔离
- **安全命令**：`cargo test --lib`（无网络、无大写入）——日常验证用这个；`cargo test` 全量同样安全（下载测试已 ignore）
- **危险命令**：`cargo test --test downloader_test -- --ignored`——真实下载最新 snapshot 数百 MB 到 `D:\download_test`，避免
- 集成测试无 mock/本地 fixture，走真实官方源；`downloader/hash.rs` 与 version JSON 解析目前无单测（仅被集成测试间接覆盖）

## Known Debt（已知遗留，待处理）

- **ModLoader 实现有错（用户自修，规划中）**：`task/runner.rs` 中 `"quilt"` 复用 `install_fabric`（Quilt 会装错）；`downloader/installer.rs` 的 `resolve_jar` Strategy 4 有 `// ponytail: only tries first manifest page` 抄来残留；`install_optifine` 生成的 version.json 用 `inheritsFrom` + `file://` URL，与 `mainClass: launchwrapper` 兼容性存疑
- **installer.rs 24 个 clippy warning**：`resolve_version_json`/`resolve_jar` 大量 `.ok().expect()`（ZIP 损坏时 panic 而非 Err）+ borrowed expr + 过参数——随 ModLoader 重构一并清理
- **待产品决策**：`GcType::Custom` 语义为"不追加 GC 参数（用 JVM 默认）"，前端 label 已注明，行为符合设计（未来可扩展为自定义参数输入框）
- **已解决**：`LauncherVisibility`（启动器可见性）2026-08 已删除（前后端同步移除）；`OfflineSkinModel::Auto` 已实现 PNG 高度自动识别（32px→slim）

## Gotchas

1. `generate_handler!` 路径必须与模块树一致，`commands/mod.rs` 无 re-export
2. 同步命令的阻塞操作（`remove_dir_all`、rename、explorer）跑在主线程——重活改 async/`spawn_blocking`
4. 平台判定统一走 `downloader/extract.rs` 的 `TARGET_OS`（cfg 编译期）与 `matches_rules`/`native_classifier_matches`/`os_classifier_key`——下载过滤（library.rs）与启动 classpath（launcher/args.rs）共用，新增平台要在 extract.rs 一处改
5. `task::init()` 必须在 `run()` 开头调用，否则 `task::global()` panic
6. 事件名 `task-progress` / `ms-login-complete` / `game-exited` 前后端字符串要一致
7. App.vue 拦截 F12/DevTools/全选/Tab/ESC 快捷键——调试注意
8. 长页面单文件巨大（Launch 1566 行、Settings 1815 行），改动前先 grep 定位
9. 版本选择持久化：localStorage 双键 `elytra.selectedVersion` + `elytra.selectedVersionDir`
10. AGENTS.md 本身被 `.gitignore` 忽略（本地文件）；README.md 是空文件
