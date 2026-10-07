# Repository Guidelines

## Project Overview

Elytra Launcher — 基于 **Tauri v2（Rust 后端）+ Vue 3（TypeScript 前端）** 的 Minecraft 启动器。功能：游戏版本下载（官方/国内镜像多源）、ModLoader 安装（Fabric/Quilt/Forge/NeoForge/OptiFine）、微软正版登录（OAuth PKCE）、离线模式（固定随机默认皮肤）、**皮肤衣柜**（3D 全身预览 + 官方皮肤/披风管理 + 本地皮肤收藏）、**JDK 扫描 + managed runtime 自动下载**（Mojang 官方 → Azul 双源回退）、Mod 双源搜索与**依赖解析**、下载任务管理、启动参数构建与游戏进程管理、**局域网联机**（easytier 库内嵌虚拟局域网 + 端口转发，房间码互连，无需公网服务器）。

后端 **115 个 `#[tauri::command]`**（分布于 16 个命令模块），前端 **10 个页面**（`src/pages/`），无状态管理库（模块级 `reactive()` 单例）。

## Architecture & Data Flow

```
前端 Vue3 (src/)  ──invoke(snake_case)──▶  commands/（薄胶水层，16 模块 / 115 命令）
    ▲                                          │ 读 persist 配置、调领域函数
    │ task-progress / ms-login-complete /     ▼
    │ game-exited 事件（emit 顺序队列线程）  downloader/（传输层）  game/（内容与装配）  auth/
    └──────── 返回 Result<T, String> 或裸值   task/（任务编排）launcher/ jdk/ online/
                                              config/（纯数据）persist/（TOML + secrets）
```

- **后端分层**：`commands/` 只做胶水（读配置 → 调领域函数 → `map_err` 展平 String）；领域逻辑按域归位——**传输层 `downloader/`**（`fetcher/{api,core,routes,redirect}.rs` 下载核心与公开入口、`batch.rs` 并发批处理、`probe.rs` 源探测 TTFB/吞吐、`path_lock.rs` 跨任务路径互斥、`hash/progress/proxy/reuse/endpoints`；**不得依赖 `task`/`launcher`/`jdk`/`game`**），**内容与装配 `game/`**（`version/` 版本元数据与本地目录、`resource/` 资源与支持库装配、`loader/` ModLoader 安装、`community/` 社区资源搜索、`modpack.rs` 整合包、`updates/` Mod 更新），任务编排 `task/`（`manager.rs` TaskManager 单例 / `emit.rs` 事件队列 / `spawn.rs` 任务注册 / `runner.rs` 下载主体 / `progress.rs` 进度与终态收口 / `store.rs` SQLite 快照），启动 `launcher/`（`args.rs` 命令构建 / `env.rs` 环境准备 / `process.rs` 进程生命周期与监听 / `script.rs` 启动脚本渲染 / `skin.rs` 离线皮肤 / `platform.rs` 平台调节），账号 `auth/`（`microsoft.rs` OAuth / `mojang.rs` 官方 API / `account.rs` 回调与披风 / `external.rs` 第三方皮肤站 / `model.rs` MsAccount），纯数据 `config/`；统一持久化 `persist/`（`config.rs` section 读写 / `store.rs` 低层文件 IO / `secrets.rs` 密钥；`util::config_dir()` 平台感知：Windows exe 旁 `Elytra/`（便携），macOS `~/Library/Application Support/Elytra`，Linux `$XDG_DATA_HOME/Elytra` 缺省 `~/.local/share/Elytra`——dev/test 进程在 cargo target 下走 exe 旁保持隔离；TOML 分 section，全局 Mutex 串行化）；联机 `online/`（`service.rs` 单例与共享状态 / `host.rs` / `guest.rs` / `session.rs` easytier 会话 / `scaffold/` 协议编解码与房间码）；跨域小工具 `util.rs`（纯函数）
- **事件推送**：`task/emit.rs` 的 `EMIT_TX`（LazyLock + 独立 `task-progress-emit` 线程 + mpsc）**顺序派发全部事件**（task-progress / task-waiting-user / ms-login-complete / game-exited / game-window-visible / online-state）——`app.emit` 若因 webview 未消费而阻塞只占队列线程，任务线程/进度锁永不碰 IPC（卡死与取消失效的根治）
- **锁约定**：全后端 `parking_lot`；**统一 inner→progress 锁序**（cancel/list/fail 先 inner 后 progress），调用 `fail()` 前必须 `drop` progress 锁（防重入死锁）；跨任务路径互斥独立为 `downloader::path_lock`（自持全局表，不挂 TaskManager）；`cancel()` 的 DB 持久化移出锁；DB（sqlite）写统一 2s 超时
- **终态收口唯一入口**：`task/progress.rs` 的 `set_done` / `set_error` / `set_cancelled`——三者内部统一「发终态事件 → 移除任务注册」，且 `set_error`/`set_cancelled` 会取走此前 `set_cleanup_path` 登记的目录做**后台**整删（禁止在任务线程里同步 `remove_dir_all` 大目录）
- **配置联动**：`gamedirs::set_active_game_dir` 写 downloader section 的 `minecraft_dir`，launch/downloader 都读该字段
- **下载源模式三态**：`game_source_mode`（游戏下载源，统管版本列表 / 版本 json / 资源 / 支持库 / ModLoader 构件与安装器 / Mojang JDK runtime：OfficialOnly / MirrorOnly / Auto 默认——按健康度自适应路由，元数据官方优先 4s 降级镜像）、`community_source_mode`（Modrinth 官方 vs MCIM 镜像）；**网络代理三态** `network_mode`（Auto 读系统代理 / System 强制 / Direct 直连）。natives 新旧两代：旧式（≤1.12）`process_natives` 解压到 `.minecraft/natives/<ver>/`（`.elytra-ready` marker）并设 `-Djava.library.path`；新式（1.13+）native JAR 作为普通库进 classpath

## Key Directories

| 路径 | 职责 |
| --- | --- |
| `src-tauri/src/downloader/` | **传输层**（URL → 校验过的字节，不认资源类型）：`fetcher/{api,core,routes,redirect}.rs`（公开 fetch 变体 / 多源下载核心 / 镜像候选与健康度排序 / 黑洞 302 拒绝与安全跟随）、`batch.rs`（并发批处理：本地复用优先 + sha1 校验 + 路径互斥 + 逐文件进度）、`probe.rs`（HEAD 测 TTFB 与流式测吞吐，共用 TTL 缓存）、`path_lock.rs`（跨任务目标路径互斥）、`hash.rs`（sha1/sha512/流式）、`progress.rs`（`ProgressCounters`）、`proxy.rs`（系统代理探测）、`reuse.rs`（本地运行时复用）、`endpoints.rs`（网络拓扑单一事实源） |
| `src-tauri/src/game/` | **内容与装配**（特化场景，依赖 `downloader` 的传输能力）：`version/`（`model.rs` 版本 JSON serde / `meta.rs` 元数据获取 / `manifest.rs` 清单 / `dir.rs` 已安装版本目录域 / `source.rs` 源与镜像重写）、`resource/`（`assets.rs` 资源索引与批量下载 / `library.rs` 支持库 / `extract.rs` rules 匹配与 natives 解压，`TARGET_OS` 唯一实现）、`loader/`（`dispatch.rs` 统一入口 / `ctx.rs` 参数结构 / `shared.rs` profile 合并与继承展开 / fabric·forge·neoforge·optifine·version_query）、`community/`（`api.rs` 出参与分派 / modrinth / curseforge / shared / translation / wiki_entry）、`modpack.rs`（整合包清单解析与安装计划）、`updates/`（Mod 扫描/识别/更新） |
| `src-tauri/src/task/` | 任务系统：`manager.rs`（TaskManager 单例 + 终态收口）、`emit.rs`（`EMIT_TX` 队列线程 + 锁外 emit）、`spawn.rs`（任务注册与 spawn）、`ctx.rs`（`TaskCtx` 句柄结构）、`model.rs`（serde 任务配置 + `TaskProgress`/`StepInfo` 唯一定义）、`steps.rs`（StepSpec 分析）、`progress.rs`（EmitLoop/心跳/`set_done`/`set_error`/`set_cancelled`）、`runner.rs`（完整下载主体）、`repair.rs`（缺失扫描/`count_assets_all`）、`files.rs`（批量下载 + WaitingForUser）、`jdk.rs`（JDK 安装任务）、`modpack.rs`（整合包安装编排）、`store.rs`（SQLite 快照/恢复） |
| `src-tauri/src/config/` | 纯数据模型（无逻辑）：`downloader.rs`（多源/三态/`NetworkMode`/`DownloaderConfig`/`DownloadSource`）、`online.rs`、`game_dirs.rs` |
| `src-tauri/src/auth/` | 微软 OAuth PKCE（`microsoft.rs`）+ 官方 API（`mojang.rs`）+ 账号服务（`account.rs`）+ 第三方皮肤站（`external.rs`，authlib-injector）+ MsAccount（`model.rs`，多账号列表，secrets.bin 加密） |
| `src-tauri/src/jdk/` | `scanner.rs`（本机 JDK 扫描）、`download.rs`（managed runtime：Mojang all.json 逐文件 sha1 → Azul 包 sha256，双源回退 + 解压）、`config.rs`（配置读写/扫描注册/`SUPPORTED_JAVA_MAJORS`）、`model.rs` |
| `src-tauri/src/online/` | **局域网联机**：`service.rs`（单例 + 共享状态 + 快照/终止）、`host.rs`（建网 + 信息服务器 6 协议 + 游戏端口发现与实例重建）、`guest.rs`（加入/中心发现/本地代理）、`session.rs`（easytier 会话：no_tun+smoltcp+KCP 数据面、host 固定虚拟 IP 10.144.144.1、guest DHCP、白名单 ACL）、`model.rs`（状态模型与协议常量）、`scaffold/`（`room.rs` 房间码 base34+模 7、`codec.rs` 帧编解码、`profiles.rs` 玩家表、`server.rs` 信息服务器、`identity.rs` machine_id）、`mc_discover.rs`（组播 224.0.2.60:4445 解析游戏端口）、`mc_advertise.rs`（guest 端模拟 MC LAN 广播，绕过 MC 26.2 离线会话校验） |
| `src-tauri/src/persist/` | `config.rs`（TOML section 读写 + 全局锁）、`store.rs`（低层文件 IO + 路径）、`secrets.rs`（密钥加密：DPAPI / Keychain / Secret Service + ChaCha20-Poly1305） |
| `src-tauri/src/launcher/` | `args.rs`（命令构建）/ `env.rs`（环境准备）/ `process.rs`（进程生命周期与监听）/ `script.rs`（BAT/SH 渲染）/ `skin.rs`（离线皮肤）/ `platform.rs`（平台调节）/ `memory.rs` / `model.rs` |
| `src-tauri/src/commands/` | 16 个命令模块（115 命令）：account(27) / community(3) / downloader(5) / gamedirs(11) / jdk(4) / launch(5) / launch_options(3) / launch_script(1) / loader(6) / offline_account(5) / online(6) / server_status(1) / skins(5，本地皮肤衣柜) / task(14) / version(15) / version_icon(4) —— 薄胶水，逻辑在各领域层 |
| `src/pages/` | Launch / Download（资源中心）/ **DownloadDetail（下载任务中心，顶级路由）** / ResourceDetail（资源详情+依赖链）/ Settings / VersionSelect / VersionSetup（容器化，业务在 store/组件）/ Tools（生成器工具集）/ CustomThemeEditor / SkinStudio（皮肤衣柜：3D 预览 + 皮肤/披风/本地衣柜） |
| `src/store/` | 25 个模块级 reactive/ref 单例：`download.ts`（dlState 任务状态机 + `ensureListen` 幂等 + terminalTaskIds 防幽灵）、`communitySearch.ts`/`categorySearch.ts`（聚合/分页/四分类工厂）、`resourceDetail.ts`、`versionDownload.ts`、`versionDetail.ts`、`launch.ts`、`accountUi.ts`、`auth.ts`、`avatar.ts`（FNV-1a 头像）、`alert.ts`、`theme.ts`（四主题）、`nav.ts`、`downloadTab.ts`、`settingsTab.ts`、`selectedVersion.ts`、`jdkConfig.ts`、`launchOptions.ts`、`downloaderConfig.ts`、`gameDirs.ts`、`online.ts`（联机状态机）、`memory.ts`、`versionIcon.ts`、`events.ts`（镜像 Rust `events.rs`）、`skinStudio.ts`（皮肤衣柜：3D 全身预览帧 + 官方皮肤/披风 + 本地衣柜） |
| `src/component/` | 按域分目录：`common/`（TitleBar / AlertDialog / DialogModal / ScrollTopFab / IconImage / EmptyState / ErrorState / CheckToggle / DropSelect / LoadingSpinner / StepDot / MenuPopover）、`download/`、`launch/`、`settings/`、`tools/`（Give/Potion/Firework/Villager/Motd 生成器 + ModsEditor + ServerStatus/Online 工具）、`version/`、`version-setup/` —— 页面组件直接 import store，无 props 链 |
| `src/utils/` | `error.ts`（**`errMsg()` 取错误首行**，toast/内联错误用）、`format.ts`（fmtCount/fmtSize/fmtModDate/fmtDateTime/fmtSpeed/fmtEta…）、`versionClassify.ts`（目录名 `{mc}-{loader}-{ver}` 解析）、`versionMeta.ts`、`give.ts`/`motd.ts`/`nbt.ts`（生成器算法）、`toast.ts`、`tooltip.ts`、`useConfigSection.ts`、`useSelectable.ts`、`useConfirmDelete.ts` |

## Development Commands

```bash
pnpm tauri dev      # 主开发命令：vite :1420 + Rust dev 窗口（Rust 改动需手动重启才生效）
pnpm tauri build    # 发布构建（vue-tsc + vite + cargo release + bundle）
pnpm dev            # 仅 vite dev server（:1420 strictPort）
pnpm build          # vue-tsc --noEmit（全量类型检查）+ vite build → dist/
cd src-tauri && cargo check              # Rust 编译检查
cd src-tauri && cargo clippy --lib       # Rust lint（保持零代码警告）
cd src-tauri && cargo test --lib -- --test-threads=1   # 单元测试（当前 257 passed + 1 ignored；串行防 persist/网络竞争）
cd src-tauri && cargo test --test online_diag          # 联机集成测试（host/guest 双实例回环，走真实 easytier）
cd src-tauri && cargo test --test e2e_download -- --ignored   # run_task 端到端（真实网络：取消路径秒级 / 完整下载分钟级）
npx vue-tsc --noEmit    # 前端全量类型检查（可在项目根直接跑）
```

**网络测试**（走真实外网单发小请求，离线环境会失败或跳过，可接受）：`fetch_asset_index_end_to_end`（`game/resource/assets.rs`）、`count_assets_all_end_to_end`（`task/repair.rs`，真实索引 + 5057 项多线程扫描）、`global_client_uses_system_proxy_when_enabled`（`downloader/proxy.rs`）、`bmclapi_optifine_real_download`（`downloader/fetcher/redirect.rs`，标 `#[ignore]`，这正是 `cargo test --lib` 里那 1 个 ignored）。

## Code Conventions & Common Patterns

### Rust 后端

- **命令命名**：snake_case、动词开头（`get_/save_/set_/fetch_/scan_/launch_/spawn_/start_/cancel_/remove_`…）
- **返回**：可失败 → `Result<T, String>`；纯读取 → 裸 `T`/`Option<T>`；错误在模块边界 `.map_err(|e| e.to_string())`
- **错误消息规范**：**中文短文案首行 + `\n` + 技术细节**（如 `下载资源索引失败\n{e}`、`请求失败\nHTTP {status}`）；前端 toast/内联错误经 `errMsg()` 取首行，AlertDialog（带复制）显示全文
- **async 分布**：网络/IO 重命令 async；配置读写/本地小操作同步；重活（remove_dir_all、rename）走 `spawn_blocking`/`tauri::async_runtime::spawn_blocking`
- **注册**：新命令 = `commands/<file>.rs` 加 `#[tauri::command] pub fn`，在 `lib.rs` 顶部（`use commands::<模块>::{…}`）追加导入，再把它加到 `generate_handler!` 列表（列**裸名**，不写路径）——`commands/mod.rs` 只有 `pub mod`，re-export 不能代替
- **持久化**：配置一律 `persist::read::<T>("section").unwrap_or_default()`；结构体实现 `Default`；`save` 前读改写（全局锁串行化）
- **锁**：一律 `parking_lot`（无中毒）；**锁序统一 inner→progress**；`fail()` 前先 `drop` progress 锁；**DB 写（sqlite）移出锁 + 2s 超时**；`update_and_emit` 锁内更新 + clone 快照、锁外 `emit_event`（O(1) 队列投递，任务线程永不碰 IPC）
- **模块组织**：`mod.rs` 只允许**子模块声明 / `pub use` 重导出 / 模块级文档 / `#[cfg(test)]` 测试模块 / 条件编译声明**——不许放任何逻辑代码（哪怕一个小函数）；实现一律放同级命名文件（如 `task/manager.rs`、`online/service.rs`、`downloader/fetcher/api.rs`）
- **import 规范**：禁止函数体内的 `use`（测试模块顶部声明不受限）；方法体内最多两级路径（`a::b`），**禁止出现三段及以上**的 `crate::a::b::c`；同名 trait（如 `sha1::Digest` 与 `sha2::Digest` 实为同一 trait）用 `use xxx as _;`；分组顺序 `std` → 外部 crate → `crate::`，组内字母序；受 `#[cfg]` 约束的 `use` 上提时保留属性
- **参数收敛**：参数超过 7 个时抽上下文结构体（现有范例：`task::TaskCtx`、`game::loader::{VersionTarget, InstallCtx}`、`game/modpack::PackMeta`、`launcher::args::{LaunchRequest, JvmSubst}`、`game::resource::assets::AssetIndexRef`）——**不得使用 `#[allow(clippy::too_many_arguments)]`**（全仓已清零）
- **测试位置**：测试与实现**同文件** `#[cfg(test)] mod tests`，不建独立 `tests.rs`；跨模块共用的测试辅助放被测模块的 `#[cfg(test)] pub(super)` 区（范例：`downloader/fetcher/core.rs` 的 `test_support`）
- **分层不变量**：`downloader/` 是纯传输层，不得 `use crate::{task, launcher, jdk, game}`；特化场景（版本装配 / 资源 / ModLoader / 社区 / 整合包 / 更新）一律放 `game/`
- **下载**：`fetch_with`（3 次重试+退避）/`fetch_fallback`（多源）/`fetch_verified(_c)`（逐源 sha1 校验，空 sha1 跳过）；`_c` 变体带取消（每块检查，Cancelled 立即传播不重试）；批量场景走 `downloader::batch::run`（`BatchItem` + 并发上限 + 复用降级 + 路径互斥）；全局 `http()` client 带 2s connect + 30s read 超时 + 系统代理 + 镜像并发信号量
- **版本目录**：`versions/{id}` 冲突时 `{id}_{n}`；目录内 jar/json 文件名可能 ≠ 目录名，一律扫描兜底
- **外部输入的落盘路径**一律过 `util::safe_relative_path`（拒绝绝对路径 / `..` / 空段），sha1 前两位子目录走 `util::sha1_sub_dir`（不裸切 `hash[..2]`）

### 前端 Vue/TS

- **IPC**：命令名与参数/返回值字段全 snake_case；**TS interface 手写镜像 Rust struct，无自动生成**——改 Rust 字段必须同步前端
- **跨组件缓存必须放普通 `<script lang="ts">` 块或 store 模块级**——`<script setup>` 顶层变量每次组件重建（路由切换）重置（Launch.vue 头像缓存、Download.vue manifest 缓存为范例）
- **状态**：模块级 `reactive()`/`ref()` 单例导出，页面直接 import；无 Pinia/Vuex
- **事件监听幂等**：`ensureListen()` 用 listening 标志防重复注册；终态（done/cancelled/error）事件用 `terminalTaskIds` 防幽灵卡片复活
- **样式**：CSS 变量统一在 App.vue `:root` + 四套 `data-theme` 主题覆盖块（`--color-brand`/`--text-*`/`--surface-*`/`--ld-*` loader 标签色等），组件页面一律 `var()` 引用、`<style scoped>`；图标用 `@lucide/vue`，无自研图标库
- **三端 WebView 一致性（保守用现代 CSS）**：渲染内核三端不同——Win WebView2（Blink）/ macOS WKWebView（WebKit）/ Linux WebKitGTK（WebKit，发行版版本滞后，如 Ubuntu 22.04 仅 2.40）；新 CSS 特性一律 `@supports` 包裹或直接写兼容写法，**禁止裸用**；中文字体栈固定三端兜底链（`"Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", "Noto Sans SC", sans-serif`），禁止单独写死平台字体。降级范例：ResourceDetail.vue `.md-ld-*` 徽章 color-mix、AlertDialog.vue 遮罩 backdrop-filter
- **错误提示两级**：toast（`utils/toast.ts` 的 `showToast(msg, durationMs=2500)` + App.vue 渲染 `toastState`）vs AlertDialog（复杂错误，带复制按钮）；均经 `errMsg()` 取首行
- **展开/收起必须有动画**：一律 `<Transition name="expand">` + `.expand-enter-to/.expand-leave-from { max-height: 1000px }` 显式目标值（max-height 写在 active 类上会因同特异性覆盖 from 态导致**无动画**，勿回退）
- **社区资源搜索**：搜索表单 + 结果列表组件成对复制（绑各自 store）；聚合模式每源只取 `AGG_PAGE_SIZE=15` 条按标题去重；进入下载页 onMounted 并发预加载全部社区 tab 热门（`ensureAutoSearch` 幂等）；左侧 Tab 刷新按钮 = 各 store 的 `reset()`
- **路由**：hash 路由（`createWebHashHistory`），`RouteMeta.transition` + `routeLevel`/`tabOrder` 驱动页面动画；DownloadDetail 为**顶级 tab**（routeLevel 0），无全局 FAB

## Important Files

| 文件 | 说明 |
| --- | --- |
| `src-tauri/src/lib.rs` | 入口：`task::init()`、后台预扫 JDK、**唯一** generate_handler! 注册点（115 个 tauri::command） |
| `src-tauri/src/main.rs` | 仅 `elytra_lib::run()` |
| `src-tauri/src/constants.rs` | APP_NAME/UA/MIRROR_CONCURRENCY、`option_env!` 注入 CURSEFORGE_API_KEY 与 MS_CLIENT_ID（.env 经 build.rs）、persist SECT_* 段名 |
| `src-tauri/src/events.rs` | 事件名常量（与 `src/store/events.ts` 手写镜像同步） |
| `src-tauri/src/task/manager.rs` | TaskManager 单例（create/pause/cancel/fail/remove/list + 挂起决策 + `LazyLock` 单例） |
| `src-tauri/src/task/emit.rs` | `EMIT_TX` 顺序队列线程 + `emit_event` 泛型派发 + `update_and_emit`（锁外 emit） |
| `src-tauri/src/task/progress.rs` | 进度上报（EmitLoop/心跳/`set_*`）+ **终态收口唯一入口** `set_done`/`set_error`/`set_cancelled` |
| `src-tauri/src/task/store.rs` | SQLite `install_jobs` 快照 + `block_on`（RuntimeFlavor 分支防 worker panic）+ 2s DB 写超时 + crash→interrupted 恢复 |
| `src-tauri/src/downloader/fetcher/core.rs` | 多源下载核心：`FetchOptions`/`download_core`/重试与换源/黑洞拒绝/镜像并发信号量 |
| `src-tauri/src/downloader/probe.rs` | 源预探测：HEAD 测 TTFB（选源）+ 流式测吞吐（JDK 大包选源），共用 TTL 缓存 |
| `src-tauri/src/launcher/args.rs` | `${key}` 占位符替换、rules/features 匹配、classpath、JDK22+ 标志 |
| `src-tauri/src/jdk/download.rs` | managed runtime 安装（Mojang all.json 逐文件 sha1 → Azul 包 sha256+解压）、`find_java_home` 递归、`InstallProgress` 钩子 |
| `src-tauri/src/auth/microsoft.rs` | 微软 OAuth（授权码/refresh/XBL/XSTS/MC login）、回调服务 127.0.0.1:27205 |
| `src-tauri/src/online/session.rs` | easytier 会话封装：no_tun+smoltcp+KCP 数据面、bind_device=false、enable_kcp_proxy=true（KCP 数据面首选，缺则 smoltcp 传大数中断） |
| `src-tauri/src/online/mc_advertise.rs` | guest 端 MC 局域网广播模拟（224.0.2.60:4445，绕过 MC 26.2 离线会话校验） |
| `src-tauri/src/util.rs` | 跨域纯函数：`config_dir()`/`safe_relative_path`/`sha1_sub_dir`/`unique_child_dir`/`fnv1a64`/`is_simple_name`/`infer_loader_from_json`… |
| `src-tauri/tauri.conf.json` | 窗口 **840×540**、无边框透明、`beforeDevCommand: pnpm dev`、devUrl :1420 |
| `src-tauri/capabilities/default.json` | main 窗口权限：core:default + window 控制子集 + opener + dialog |
| `src/store/download.ts` | 下载任务状态机（事件驱动核心） |

## Runtime/Tooling Preferences

- **包管理器必须 pnpm**：`beforeDevCommand` 硬编码 `pnpm dev`；`pnpm-lock.yaml` 被 gitignore（依赖版本不锁定）
- **工具链很新**：vite 8、typescript 6、vue-tsc 3、vue-router 5——新增依赖前确认兼容（@lucide/vue 图标库已引入）
- **窗口**：840×540、`decorations:false` + `transparent:true` + `resizable:false`——前端自绘 TitleBar（`startDragging`）
- **Rust 变更需重启 `pnpm tauri dev`**（vite `watch.ignored` 忽略 src-tauri）；前端改动 HMR 即时生效
- **前端大量改动后建议重启 dev**：跨多个 store/模块的批量改动、或触发依赖预构建失效时（vite 日志出现 `Re-optimizing dependencies because vite config has changed`），HMR 可能停在**半旧半新**状态——表现为「组件逻辑更新了但 store 还是旧的」这类诡异现象（实测过一次：资源详情页左上角返回键点了完全没反应，重启 dev 即恢复）。**排查前端怪状前，先重启一次 dev 排除这个因素**
- **reqwest 0.13 无 `.form()`**：表单需手工 urlencode（auth 里 `form_body`）
- **MS_CLIENT_ID 固定，已注册完成**；回调端口 27205 写死
- **lib 名 `elytra_lib`**：Windows 下 bin/lib 同名会 Cargo 8519，保留 `_lib` 后缀
- **版本号**：tauri.conf.json / Cargo.toml / package.json 三处一致（当前 1.0.0），改动需同步
- **capabilities 是 v2 权限门**：新增 JS 侧 window/fs/dialog API 必须先在 `default.json` 加权限
- **打包与卸载**：`bundle.targets` 含 `nsis` + `msi`。Tauri 的 NSIS 卸载器是**逐文件 `Delete` + `RMDir "$INSTDIR"`（非递归，只删空目录）**——游戏目录（`.minecraft`）虽在安装目录内但非空，**卸载不会删游戏数据**；`src-tauri/windows/hooks.nsh` 的 `PREUNINSTALL` 仅做告知（`IfSilent` 跳过，避免静默卸载卡住）。另一处坑：`identifier`（`xyz.bridgefish.elytra`）与配置目录名（`constants::CONFIG_DIR = "Elytra"`）不同，Tauri 原生「删除应用数据」复选框实际删的是 `%APPDATA%\<identifier>` 这个并不存在的目录，故 `POSTUNINSTALL` 按真实路径补删，避免「说删没删」。**MSI 默认装 `Program Files`（只读）**，`default_game_dir_base` 对其做了回退；NSIS 默认装 `%LOCALAPPDATA%\Programs`（可写），游戏目录照旧在 exe 旁。另：卸载后 `HKCU\Software\<manufacturer>\<product>` 会保留（Tauri 用它记住上次安装位置，重装时恢复），仅在勾选「删除应用数据」时才清除——这是有意行为，不是残留 bug
- **系统代理**：Windows 读注册表（`winreg`，仅 windows 依赖）→ env；Linux env → GNOME gsettings；reqwest 默认不读系统代理，开启系统代理（Clash 等）时官方源直连会被劫持/黑洞
- **联机编译**：easytier `kcp` feature 依赖 kcp-sys（bindgen 需 libclang）——构建机须装 LLVM（`winget install LLVM.LLVM`）并设 `LIBCLANG_PATH`
- 敏感信息（token/client secret）不硬编码、不打印日志、不提交 git
- 全局事件拦截：App.vue 禁 F12/DevTools/全选/Tab/ESC 快捷键——调试注意

## Testing & QA

- **无前端测试**；Rust 侧约 257 个单元测试，全部内嵌各模块 `#[cfg(test)] mod tests`（含测试与实现同文件的约定），`cargo test --lib -- --test-threads=1` 运行；另有集成测试 `tests/online_diag.rs`（联机全链路：host/guest 双实例回环 + easytier 日志落盘）
- **单元测试只测纯函数**：PKCE、URL 组装、启动命令构建（`build_launch_command` + `minimal_version_json()` 助手）、离线皮肤 UUID 算法（与 Java `UUID.hashCode() & 1` 一致性）、内存分配公式、util（URL 文件名/版本目录解析/`sha1_sub_dir`/`safe_relative_path`）、maven XML 解析与语义化排序、Mod 双源搜索、修复步骤规划、persist 读写回环、secrets DPAPI、store 快照/恢复、**`store_ops_from_runtime_worker_no_panic`（worker 线程 block_on 回归）**、**`count_assets_all_end_to_end`（真实索引 + 5057 项多线程扫描，网络）**、**`find_java_home_finds_nested_bin`（Azul 布局幂等）**、`downloader/fetcher` 的取消/换源/校验/重定向/并发上限、loader 的 profile 合并与继承展开、房间码解析/广播回环（online）
- **断言风格**：`assert_eq!` 精确值为主（argv 位置、公式值、错误码消息）；行为不变量用 `assert!` + 中文 message
- **平台隔离**：Windows 专属逻辑（BAT 脚本渲染、DPAPI、注册表代理）用 `#[cfg(target_os = "windows")]`
- **persist 测试隔离**：测试进程下配置落在 `target/debug/deps/Elytra/Settings.toml`（config_dir 基于 current_exe 推导，测试进程在 deps/ 下），与正式运行隔离；每个测试先 remove_file 再 save（串行依赖）
- **安全命令**：`cargo test --lib -- --test-threads=1`——日常验证用这个；**并行跑会偶发竞争**（persist 共用 Settings.toml + 网络测试抖动），必须串行
- **clippy 红线**：`cargo clippy --lib` 保持零代码警告（`too_many_arguments` 已通过上下文结构体全部消除；仅余 `type_complexity` ×2 与 `upper_case_acronyms` ×3（Win32 类型），均为显式 allow）
- **端到端测试**：`tests/e2e_download.rs`（`#[ignore]`，需真实网络）驱动 `run_task`（下载主流程）——任务层的事件出口已抽象为 `task::emit::EventSink`（`impl<R: Runtime> EventSink for AppHandle<R>`），测试注入记录型实现即可跑完整流程且不触碰 IPC。两个用例分别覆盖「阶段 A/B 真实执行 + 阶段 C 取消收口」与「真实完整下载 Minecraft 1.20.1」。**注意**：须用 `#[tokio::test(flavor = "multi_thread")]` —— 终态收口经 `store::update_status` 走 `util::block_on` 的 `block_in_place` 分支，current_thread runtime 下会 panic（与生产 tauri runtime 一致）。

## Known Debt（已知遗留，待处理）

- **OptiFabric 覆盖范围有限**：Fabric/Quilt + OptiFine 依赖 OptiFabric 桥接（CurseForge 渠道，slug `optifabric`）；OptiFabric 维护缓慢（CF 最新 1.14.3，2024-01，仅覆盖至 ~1.20.x），1.21+ 组合会明确报错而非静默失效
- **待产品决策**：`GcType::Custom` 语义为"不追加 GC 参数（用 JVM 默认）"，前端 label 已注明，行为符合设计
- **已完成的重构**：全仓 >150 行函数批量拆分完成——`run_task`（534 → 192，阶段 7）+ 其余 9 个长函数（阶段 8，`run_repair_task`/`join`/`run_files_task`/`build_launch_command`/`download_and_extract`/`start_host`/`run_install_jdk`/`run_install`/`extract_forge_client_lzma`），共约 40 个新辅助单元（上下文结构体 + 阶段函数）；仅 `task/progress.rs::EmitLoop::spawn` 判定不拆（本体即采样循环）。事件出口已抽象为 `task::emit::EventSink`；`run_task` 与联机链路均有真实网络端到端测试（`tests/e2e_download.rs` / `tests/online_diag.rs`）
- **已解决**：`LauncherVisibility` 已删除（前后端同步）；离线皮肤配置层已砍（启动与头像固定随机哈希 Steve/Alex，后续计划接入自定义皮肤站登录）；OptiFine 分场景安装（纯净版 classpath 独立版本 / Forge-like 进 mods / Fabric-like OptiFabric 进 mods，mainClass 统一官方 Main；重定向黑洞黑名单精确放行 `minio.749333.xyz`——bmclapi 下载主存储）
- **已解决**：guest 端 MC 局域网广播名参数化（房间码 `#MOTD` 段携带 host 存档名 → guest 广播真实世界名；`OnlineConfig.advertise_motd` 支持自定义广播名，优先级 自定义 > host 世界名 > 默认文案）
- **联机遗留**：跨网联机依赖协调服务器（默认社区节点 `tcp://node.invalid:11010`，easytier 官方公共节点已下线）；MC 26.2 离线账号只能走"局域网发现"路径进入（手动 IP 会报无效会话，广播模拟已绕过）
- **备注**：README.md 是完整中文文档（`README-en-US.md` 为英文版）；AGENTS.md 已被 git 跟踪（修改会出现在 git status）
