# Repository Guidelines

## Project Overview

Elytra Launcher — 基于 **Tauri v2（Rust 后端）+ Vue 3（TypeScript 前端）** 的 Minecraft 启动器。功能：游戏版本下载（官方/国内镜像多源）、ModLoader 安装（Fabric/Quilt/Forge/NeoForge/OptiFine）、微软正版登录（OAuth PKCE）、离线模式（固定随机默认皮肤）、**JDK 扫描 + managed runtime 自动下载**（Mojang 官方 → Azul 双源回退）、Mod 双源搜索与**依赖解析**、下载任务管理、启动参数构建与游戏进程管理、**局域网联机**（easytier 库内嵌虚拟局域网 + 端口转发，房间码互连，无需公网服务器）。

后端约 60+ 个 `#[tauri::command]`（联机 7 个），前端 7 个页面，无状态管理库（模块级 `reactive()` 单例）。

## Architecture & Data Flow

```
前端 Vue3 (src/)  ──invoke(snake_case)──▶  commands/（薄胶水层）
    ▲                                          │ 读 persist 配置、调领域函数
    │ task-progress / ms-login-complete /     ▼
    │ game-exited 事件（emit 顺序队列线程）  downloader/（下载流水线）auth/（微软登录）
    └────────────── 返回 Result<T, String> 或裸值  config/（纯数据模型）task/（TaskManager）
```

- **后端分层**：`commands/` 只做胶水（读配置 → 调领域函数 → `map_err` 展平 String）；领域逻辑按域归位——下载编排 `task/runner.rs`，启动 `config/launcher/`（`args.rs` 命令构建 / `env.rs` 环境准备 / `skin.rs` 离线皮肤），账号 `auth/`（`microsoft.rs` OAuth / `mojang.rs` 官方 API / `account.rs` 回调与披风 / `model.rs` MsAccount），下载引擎 `downloader/`，纯数据 `config/`；统一持久化 `persist/`（`util::config_dir()` 平台感知：Windows/Linux exe 旁 `Elytra/Settings.toml`，macOS `~/Library/Application Support/Elytra/`，TOML 分 section，全局 Mutex 串行化）；任务 `task/` TaskManager 单例（`tauri::async_runtime::spawn` + 事件推送）；联机 `online/`（easytier 内嵌虚拟局域网）；跨域小工具 `util.rs`（纯函数）
- **事件推送**：`task/mod.rs` 的 `EMIT_TX`（LazyLock + 独立 `task-progress-emit` 线程 + mpsc）**顺序派发全部事件**（task-progress / task-waiting-user / ms-login-complete / game-exited / game-window-visible / online-state）——`app.emit` 若因 webview 未消费而阻塞只占队列线程，任务线程/进度锁永不碰 IPC（卡死与取消失效的根治）
- **锁约定**：全后端 `parking_lot`；**统一 inner→progress 锁序**（cancel/list/fail 先 inner 后 progress），调用 `fail()` 前必须 `drop` progress 锁（防重入死锁）；`cancel()` 的 DB 持久化移出锁；DB（sqlite）写统一 2s 超时
- **配置联动**：`gamedirs::set_active_game_dir` 写 downloader section 的 `minecraft_dir`，launch/downloader 都读该字段
- **下载源模式三态**：`file_source_mode`（OfficialOnly / ChinaOnly / Auto 默认——按健康度自适应路由，元数据官方优先 4s 降级镜像）、`manifest_source_mode`、`community_source_mode`（Modrinth 官方 vs MCIM 镜像）；**网络代理三态** `network_mode`（Auto 读系统代理 / System 强制 / Direct 直连）。natives 新旧两代：旧式（≤1.12）`process_natives` 解压到 `.minecraft/natives/<ver>/`（`.elytra-ready` marker）并设 `-Djava.library.path`；新式（1.13+）native JAR 作为普通库进 classpath

## Key Directories

| 路径                        | 职责                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src-tauri/src/commands/`   | 16 个命令模块：jdk / downloader / loader / task / account / gamedirs / launch / community / online / **map（地图渲染）/ map_editor（三级树+世界图标）** / version / server_status / offline_account / launch_options / launch_script（真薄胶水；`launch.rs` 额外含进程管理） |
| `src-tauri/src/config/`     | 纯数据模型：`launcher/`（`args.rs` 命令构建 / `env.rs` / `memory.rs` / `platform.rs` / `skin.rs` 离线皮肤）、`downloader/model.rs`（多源/三态/NetworkMode，`source.rs` 为 DownloaderConfig 网络 impl：`fetch_with_fallback` 主源 4s 降级 + URL 去重 + 10 分钟 TTL 缓存）、`game_dirs.rs`、`offline_skin.rs`                                                                                                                                                                                                                                                                                |
| `src-tauri/src/downloader/` | 下载引擎：`fetcher.rs`（文件下载核心：自适应路由/健康度/黑洞 302 拒绝/系统代理/超时双保险/多种 fetch 变体含 `_c` 可取消）、`source.rs`、`assets.rs`、`library.rs`、`extract.rs`（`TARGET_OS`/`matches_rules` 平台判定唯一实现）、`community.rs`（Mod/光影/资源包等社区资源双源搜索+依赖解析）、`progress.rs`（`ProgressCounters` 下载进度计数器）、`mod_loader/`（fabric/forge/neoforge/optifine/version_query）、`reuse.rs`（本地运行时复用）                                                                                                                                                                                                                           |
| `src-tauri/src/task/`       | 任务系统：`mod.rs`（TaskManager 单例 + `EMIT_TX` 队列线程 + PathLock + `emit_event`）、`model.rs`（serde 任务配置 + `TaskProgress`/`StepInfo` 唯一定义）、`steps.rs`（StepSpec 分析）、`progress.rs`（EmitLoop/心跳/set_*）、`runner.rs`（下载主体 + `run_install_jdk`）、`repair.rs`（缺失扫描/`count_assets_all`）、`files.rs`（批量下载 + WaitingForUser）、`store.rs`（SQLite 快照/恢复）                                                                                                                                                                                              |
| `src-tauri/src/auth/`       | 微软 OAuth PKCE（`microsoft.rs`）+ 官方 API（`mojang.rs`）+ 账号服务（`account.rs`）+ MsAccount（`model.rs`，多账号列表，secrets.bin 加密）                                                                                                                                                                                                                                                                                                                                                                                                                                                |
|`src-tauri/src/map/`|**世界地图域模块**（地图编辑器数据层，纯领域逻辑无 IPC）：`tree.rs`（三级树：游戏目录→版本→存档，单次 IPC 返回全树）、`region_io.rs`（mca 读取 + fastanvil 三代 chunk 解析回退 + `JavaWrcDecompression` 自定义压缩兜底 + `dimension_dir` 维度解析唯一实现 + `MapRegionInfo` 含 mtime）、`render.rs`（**主流自然色调色板**：基础方块色由 `tools/gen_palette.mjs` 从原版客户端贴图顶面采样生成 JSON 内嵌（含 per-texture 平均色表），草/树叶走原版生物群系色图、水面走生物群系水色、全局饱和度/亮度调校 `grade()`、未知方块语义兜底；top-down/y 切片渲染，`render_region_tile` 全 region 合成 512×512 PNG——合成缓冲 1MB 必须堆分配）、**`mod_color.rs`（Mod 方块色动态覆盖层：jar 集合 sha1 指纹 + 3s TTL 复核，命名空间→jar 索引，按需采样缺失方块——blockstate/模型链解析含原版模板面语义表与 26.x sprite 纹理格式，Mod 引用原版贴图走内嵌纹理表；渲染层 pick_with 传入）**、`tile_cache.rs`（PNG 落盘 `mapcache/`，键=格式版本+路径哈希+维度+region 坐标+样式+mtime）、`chunk_pos.rs`（坐标换算纯函数）、**`biomegen/`（**生物群系预测引擎**：未探索区"底图"层——语义逐项对照 26.1.2 官方 server 字节码 + 官方 datagen `biome_parameters` 表（7593 行内嵌 `overworld_26.bin`）：Xoroshiro128PlusPlus/`md5("octave_k")` 播种链、ImprovedNoise/PerlinNoise/NormalNoise(337:331)、ShiftA/B、shifted_noise xz_scale=0.25、depth=y 梯度+offset 样条（内嵌 `offset_26.json`）；真实存档 full-chunk 表层对拍 95.8%（12 群系 21,904 样本）；`png_tile.rs` 预测瓦片 512²、`colors.rs` 54 群系色板、`world.rs` 种子读取（`world_gen_settings.dat` gzip NBT）+ 共享预测器单槽缓存；命令 `get_prediction_tile` + `get_map_block` 未加载区预测回退；1.20.1→26.1.2 气候密度函数字节级一致、1.18 前不支持；mod 引擎替换型不支持）+ **`terrain/`（地形高度引擎，26.1.2 final_density 全移植：`terrain_26.json` 内嵌（20 密度闭包 + 43 noise 规格）、`bank.rs` 噪声银行（NormalNoise md5 xor 根对 + legacy BlendedNoise 播种链）、`eval.rs` arena 求值器（shift_a/b、样条嵌套、weird_scaled、blend 常量、y 梯度…）、`legacy.rs` BlendedNoise（16/8 层结构移植）、`solver.rs` 共享引擎单例 + 盖层 3 判水 + 快分类（~0.7ms/列 debug；真实对拍：森林/内陆带 96.9%、全档含沼泽滩涂 82~85%，见 Known Debt）；png_tile 水域分层渲染（缓存键 v2 + terrain 开关 t0/t1）；**DataVersion ≥4790（26.1.2 校准）才叠加地形水域，旧参数族走群系色底图**）**
| `src-tauri/src/online/`     | **局域网联机**：easytier 库内嵌虚拟局域网（`session.rs` 会话封装：no_tun+smoltcp+KCP 数据面、host 固定虚拟 IP 10.144.144.1、guest DHCP、端口转发方向 host 入口→本地游戏 / guest 本地→host 入口）、`room.rs`（房间码：UUID 短码+入口端口+@LAN IP，HMAC 派生网络凭据）、`mod.rs`（OnlineService 单例 + online-state 事件：host 组播扫游戏端口补转发 / guest 加入 + host 可达性探测 + MC 局域网广播模拟）、`mc_discover.rs`（组播 224.0.2.60:4445 解析游戏端口）、`mc_advertise.rs`（guest 端模拟 MC LAN 广播，绕过 MC 26.2 离线会话校验）                                                                                                                                                                                                                                                                                                                                                                     |
| `src/pages/`                | Launch / Download（资源中心）/ **DownloadDetail（下载任务中心，顶级路由）** / ResourceDetail（资源详情+依赖链）/ Settings / VersionSelect / VersionSetup（容器化，业务在 store/组件）                                                                                                                                                                                                                                                                                                                                                                                                      |
| `src/store/`                | `download.ts`（dlState 任务状态机 + `ensureListen` 幂等 + terminalTaskIds 防幽灵）、`memory.ts`、`communitySearch.ts`（聚合/分页）、`categorySearch.ts`（四分类工厂）、`resourceDetail.ts`、`versionDownload.ts`、`launch.ts`、`accountUi.ts`、`auth.ts`、`avatar.ts`（FNV-1a 头像）、`alert.ts`、`theme.ts`（四主题）、`selectedVersion.ts`、`jdkConfig.ts`、`launchOptions.ts`、`offlineSkinConfig.ts`、`downloaderConfig.ts`、`gameDirs.ts`、`online.ts`（联机状态机）、`events.ts`（事件名镜像 Rust `events.rs`）——模块级 reactive/ref 单例；`useConfigSection`（utils）可在模块顶层调用                                |
| `src/component/`            | TitleBar / DownloadTab / VersionCard / VersionItem / AlertDialog / DialogModal / ScrollTopFab / VanillaList / VanillaDetail / ModSearchBar / ModResultList / ShaderSearchBar / ShaderResultList / ResourcepackSearchBar / ResourcepackResultList / DatapackSearchBar / DatapackResultList / ModpackSearchBar / ModpackResultList / IconImage / LaunchProgress / AccountSection / LaunchSection / CapeDialog / SettingsOfflineSkin / SettingsLaunchOptions / SettingsJdk / SettingsDownload / GameDirList / GameVersionList / VersionSetupOverview / SettingsOnline ——页面组件直接 import store，无 props 链 |
| `src/utils/`                | versionClassify（目录名 `{mc}-{loader}-{ver}` 解析）、format（fmtCount/fmtSize/fmtModDate/fmtDateTime）、**error.ts `errMsg()`（取错误首行：toast/内联错误显示用，dialog 场景不动）**                                                                                                                                                                                                                                                                                                                                                                                        |

## Development Commands

```bash
pnpm tauri dev      # 主开发命令：vite :1420 + Rust dev 窗口（Rust 改动需手动重启才生效）
pnpm tauri build    # 发布构建（vue-tsc + vite + cargo release + bundle）
pnpm dev            # 仅 vite dev server（:1420 strictPort）
pnpm build          # vue-tsc --noEmit（全量类型检查）+ vite build → dist/
cd src-tauri && cargo check    # Rust 编译检查
cd src-tauri && cargo clippy   # Rust lint（保持零代码警告）
cd src-tauri && cargo test --lib -- --test-threads=1   # 单元测试（248 个；串行防 persist/网络竞争）
npx vue-tsc --noEmit           # 前端全量类型检查（可在项目根直接跑）
```

**测试红线**：`download_snapshot_full` 已标 `#[ignore]`（真实下载数百 MB 到 `D:\download_test`），`cargo test` 全量默认不触发；显式 `cargo test --test downloader_test -- --ignored` 才跑。网络测试（`fetch_manifest_works`/`fetch_asset_index_end_to_end`/`count_assets_all_end_to_end`/`global_client_uses_system_proxy_when_enabled`）走真实外网单发小请求，离线环境会失败或跳过，可接受。联机集成测试 `cargo test --test online_diag`（host/guest 双实例回环 5MB 大数据 + easytier 日志落盘，走真实生产代码 `EasyTierSession::start`）。

## Code Conventions & Common Patterns

### Rust 后端

- **命令命名**：snake_case、动词开头（`get_/save_/set_/fetch_/scan_/launch_/spawn_/start_/cancel_/remove_`…）
- **返回**：可失败 → `Result<T, String>`；纯读取 → 裸 `T`/`Option<T>`；错误在模块边界 `.map_err(|e| e.to_string())`
- **错误消息规范**：**中文短文案首行 + `\n` + 技术细节**（如 `下载资源索引失败\n{e}`、`请求失败\nHTTP {status}`）；前端 toast/内联错误经 `errMsg()` 取首行，AlertDialog（带复制）显示全文
- **async 分布**：网络/IO 重命令 async；配置读写/本地小操作同步；重活（remove_dir_all、rename）走 `spawn_blocking`/`tauri::async_runtime::spawn_blocking`
- **注册**：新命令 = `commands/<file>.rs` 加 `#[tauri::command] pub fn`，再在 `lib.rs` 的 `generate_handler!` 追加**全限定路径**——`commands/mod.rs` 只有 `pub mod`，re-export 不能代替
- **持久化**：配置一律 `persist::read::<T>("section").unwrap_or_default()`；结构体实现 `Default`；`save` 前读改写（全局锁串行化）
- **锁**：一律 `parking_lot`（无中毒）；**锁序统一 inner→progress**；`fail()` 前先 `drop` progress 锁；**DB 写（sqlite）移出锁 + 2s 超时**；`update_and_emit` 锁内更新 + clone 快照、锁外 `emit_event`（O(1) 队列投递，任务线程永不碰 IPC）
- **下载**：`fetch_with`（3 次重试+退避）/`fetch_fallback`（多源）/`fetch_verified(_c)`（逐源 sha1 校验，空 sha1 跳过）；`_c` 变体带取消（每块检查，Cancelled 立即传播不重试）；全局 `http()` client 带 2s connect + 30s read 超时 + 系统代理 + 镜像并发信号量
- **版本目录**：`versions/{id}` 冲突时 `{id}_{n}`；目录内 jar/json 文件名可能 ≠ 目录名，一律扫描兜底

### 前端 Vue/TS

- **IPC**：命令名与参数/返回值字段全 snake_case；**TS interface 手写镜像 Rust struct，无自动生成**——改 Rust 字段必须同步前端
- **跨组件缓存必须放普通 `<script lang="ts">` 块或 store 模块级**——`<script setup>` 顶层变量每次组件重建（路由切换）重置（Launch.vue 头像缓存、Download.vue manifest 缓存为范例）
- **状态**：模块级 `reactive()`/`ref()` 单例导出，页面直接 import；无 Pinia/Vuex
- **事件监听幂等**：`ensureListen()` 用 listening 标志防重复注册；终态（done/cancelled/error）事件用 `terminalTaskIds` 防幽灵卡片复活
- **样式**：CSS 变量统一在 App.vue `:root` + 四套 `data-theme` 主题覆盖块（`--color-brand`/`--text-*`/`--surface-*`/`--ld-*` loader 标签色等），组件页面一律 `var()` 引用、`<style scoped>`；图标用 `@lucide/vue`，无自研图标库
- **三端 WebView 一致性（保守用现代 CSS）**：渲染内核三端不同——Win WebView2（Blink）/ macOS WKWebView（WebKit）/ Linux WebKitGTK（WebKit，发行版版本滞后，如 Ubuntu 22.04 仅 2.40）；新 CSS 特性一律 `@supports` 包裹或直接写兼容写法，**禁止裸用**；中文字体栈固定三端兜底链（`"Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", "Noto Sans SC", sans-serif`），禁止单独写死平台字体。降级范例：ResourceDetail.vue `.md-ld-*` 徽章 color-mix、AlertDialog.vue 遮罩 backdrop-filter（2025-08 治理落地）
- **错误提示两级**：toast（`window.__showToast`，2.5s）vs AlertDialog（复杂错误，带复制按钮）；均经 `errMsg()` 取首行
- **展开/收起必须有动画**：一律 `<Transition name="expand">` + `.expand-enter-to/.expand-leave-from { max-height: 1000px }` 显式目标值（max-height 写在 active 类上会因同特异性覆盖 from 态导致**无动画**，勿回退）
- **社区资源搜索**：搜索表单 + 结果列表组件成对复制（绑各自 store）；聚合模式每源只取 `AGG_PAGE_SIZE=15` 条按标题去重；进入下载页 onMounted 并发预加载全部社区 tab 热门（`ensureAutoSearch` 幂等）；左侧 Tab 刷新按钮 = 各 store 的 `reset()`
- **路由**：hash 路由（`createWebHashHistory`），`RouteMeta.transition` + `routeLevel`/`tabOrder` 驱动页面动画；DownloadDetail 为**顶级 tab**（routeLevel 0），无全局 FAB

## Important Files

| 文件                                    | 说明                                                                                                                                                           |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/src/lib.rs`                  | 入口：`task::init()`、后台预扫 JDK、**唯一** generate_handler! 注册点（约 60+ 命令）                                                                            |
| `src-tauri/src/main.rs`                 | 仅 `elytra_lib::run()`                                                                                                                                         |
| `src-tauri/src/constants.rs`            | APP_NAME/UA/MIRROR_CONCURRENCY、`option_env!` 注入 CURSEFORGE_API_KEY 与 MS_CLIENT_ID（.env 经 build.rs）、persist SECT_* 段名                                 |
| `src-tauri/src/events.rs`               | 事件名常量（与 `src/store/events.ts` 手写镜像同步）                                                                                                            |
| `src-tauri/src/task/mod.rs`             | TaskManager 单例 + `EMIT_TX` 顺序队列线程 + `emit_event` 泛型派发 + PathLock + `update_and_emit`（锁外 emit）                                                  |
| `src-tauri/src/task/store.rs`           | SQLite `install_jobs` 快照 + `block_on`（RuntimeFlavor 分支防 worker panic）+ 2s DB 写超时 + crash→interrupted 恢复                                            |
| `src-tauri/src/downloader/fetcher.rs`   | 54KB 下载核心：`build_client()`（OnceLock，含 mirror_safe_redirect + apply_system_proxy）、`system_proxy_urls()`、`resolve_download_routes()`、全部 fetch 变体 |
| `src-tauri/src/config/launcher/args.rs` | `${key}` 占位符替换、rules/features 匹配、classpath、JDK22+ 标志                                                                                               |
| `src-tauri/src/jdk/download.rs`         | `auto_install`（Mojang all.json 逐文件 sha1 → Azul 包 sha256+解压）、`find_java_home` 递归、`safe_runtime_path` zip slip 防护、`InstallProgress` 钩子          |
| `src-tauri/src/auth/microsoft.rs`       | 微软 OAuth（授权码/refresh/XBL/XSTS/MC login）、回调服务 127.0.0.1:27205                                                                                       |
| `src-tauri/src/online/session.rs`       | easytier 会话封装：no_tun+smoltcp+KCP 数据面、bind_device=false、enable_kcp_proxy=true（KCP 数据面首选，缺则 smoltcp 传大数中断）、easytier 诊断日志初始化      |
| `src-tauri/src/online/mc_advertise.rs`  | guest 端 MC 局域网广播模拟（224.0.2.60:4445，绕过 MC 26.2 离线会话校验）                                                                                       |
| `src-tauri/src/persist/store.rs`        | 配置文件路径 = `util::config_dir()/Settings.toml`（Windows/Linux exe 旁 `Elytra/`；macOS `~/Library/Application Support/Elytra/`——app bundle 内部只读且破坏签名，故外置） |
| `src-tauri/tauri.conf.json`             | 窗口 **840×540**、无边框透明、`beforeDevCommand: pnpm dev`、devUrl :1420                                                                                       |
| `src-tauri/capabilities/default.json`   | main 窗口权限：core:default + window 控制子集 + opener + dialog                                                                                                |
| `src/store/download.ts`                 | 下载任务状态机（事件驱动核心）                                                                                                                                 |

## Runtime/Tooling Preferences

- **包管理器必须 pnpm**：`beforeDevCommand` 硬编码 `pnpm dev`；`pnpm-lock.yaml` 被 gitignore（依赖版本不锁定）
- **工具链很新**：vite 8、typescript 6、vue-tsc 3、vue-router 5——新增依赖前确认兼容（@lucide/vue 图标库已引入）
- **窗口**：840×540、`decorations:false` + `transparent:true` + `resizable:false`——前端自绘 TitleBar（`startDragging`）
- **Rust 变更需重启 `pnpm tauri dev`**（vite `watch.ignored` 忽略 src-tauri）；前端改动 HMR 即时生效
- **reqwest 0.13 无 `.form()`**：表单需手工 urlencode（auth 里 `form_body`）
- **MS_CLIENT_ID 固定，已注册完成 **；回调端口 27205 写死
- **lib 名 `elytra_lib`**：Windows 下 bin/lib 同名会 Cargo 8519，保留 `_lib` 后缀
- **版本号**：tauri.conf.json / Cargo.toml / package.json 三处一致（当前 1.0.0），改动需同步
- **capabilities 是 v2 权限门**：新增 JS 侧 window/fs/dialog API 必须先在 `default.json` 加权限
- **系统代理**：Windows 读注册表（`winreg`，仅 windows 依赖）→ env；Linux env → GNOME gsettings；reqwest 默认不读系统代理，开启系统代理（Clash 等）时官方源直连会被劫持/黑洞
- **联机编译**：easytier `kcp` feature 依赖 kcp-sys（bindgen 需 libclang）——构建机须装 LLVM（`winget install LLVM.LLVM`）并设 `LIBCLANG_PATH`
- 敏感信息（token/client secret）不硬编码、不打印日志、不提交 git
- 全局事件拦截：App.vue 禁 F12/DevTools/全选/Tab/ESC 快捷键——调试注意

## Testing & QA

- **无前端测试**；Rust 侧 191 个单元测试，全部内嵌各模块 `#[cfg(test)]`（无 tests/ 集成目录），`cargo test --lib` 运行；另有一个集成测试 `tests/online_diag.rs`（联机全链路）
- **单元测试只测纯函数**：PKCE、URL 组装、启动命令构建（`build_launch_command` + `minimal_version_json()` 助手）、离线皮肤 UUID 算法（与 Java `UUID.hashCode() & 1` 一致性）、内存分配公式、util（URL 文件名/版本目录解析）、maven XML 解析与语义化排序、Mod 双源搜索、修复步骤规划、persist 读写回环、secrets DPAPI、store 快照/恢复、**`store_ops_from_runtime_worker_no_panic`（worker 线程 block_on 回归）**、**`count_assets_all_end_to_end`（真实索引 + 5057 项多线程扫描，网络）**、**`find_java_home_finds_nested_bin`（Azul 布局幂等）**、房间码解析/广播回环（online）
- **断言风格**：`assert_eq!` 精确值为主（argv 位置、公式值、错误码消息）；行为不变量用 `assert!` + 中文 message
- **平台隔离**：Windows 专属逻辑（BAT 脚本渲染、DPAPI、注册表代理）用 `#[cfg(target_os = "windows")]`
- **persist 测试隔离**：测试进程下配置落在 `target/debug/deps/Elytra/Settings.toml`（config_dir 基于 current_exe 推导，测试进程在 deps/ 下），与正式运行隔离；每个测试先 remove_file 再 save（串行依赖）
- **安全命令**：`cargo test --lib -- --test-threads=1`（无网络、无大写入）——日常验证用这个；**并行跑会偶发竞争**（persist 共用 Settings.toml + 网络测试抖动），必须串行
- **危险命令**：`cargo test --test downloader_test -- --ignored`——真实下载最新 snapshot 数百 MB 到 `D:\download_test`，避免
- **clippy 红线**：`cargo clippy --lib` 保持零代码警告（doc 格式、unneeded return、too_many_arguments 均已清理或显式 allow）
- 集成测试无 mock/本地 fixture，走真实官方源；`downloader/hash.rs` 与 version JSON 解析无单测（仅被集成测试间接覆盖）

## Known Debt（已知遗留，待处理）

- **地图预测层水陆已由地形引擎解决**（`biomegen/terrain/`，26.1.2 final_density 全移植 + 盖层 CAP=3）：真实逐列对拍——森林/内陆带 96.9%，全档 16 region 12.6k 列 82~85%；北西大片沼泽/滩涂带（真实水面 62）列级水陆由地表规则主导（深 dirt 岛 4~10 层、pond 沟、块插值 ±1~3），逐列精确需 surface-rules + 水位模拟（跨日级，待产品决策）；渲染为 4×4 单元整格，视觉影响低于列级数字；渲染已接入（预测瓦片水域按深分层，缓存键 v2），快分类 ~0.7ms/列
- **OptiFabric 覆盖范围有限**：Fabric/Quilt + OptiFine 依赖 OptiFabric 桥接（CurseForge 渠道，slug `optifabric`）；OptiFabric 维护缓慢（CF 最新 1.14.3，2024-01，仅覆盖至 ~1.20.x），1.21+ 组合会明确报错而非静默失效
- **待产品决策**：`GcType::Custom` 语义为"不追加 GC 参数（用 JVM 默认）"，前端 label 已注明，行为符合设计
- **已解决**：`LauncherVisibility` 已删除（前后端同步）；离线皮肤配置层已砍（启动与头像固定随机哈希 Steve/Alex，后续计划接入自定义皮肤站登录）；OptiFine 分场景安装（纯净版 classpath 独立版本 / Forge-like 进 mods / Fabric-like OptiFabric 进 mods，mainClass 统一官方 Main；重定向黑洞黑名单精确放行 `minio.749333.xyz`——bmclapi 下载主存储）
- **已解决**：guest 端 MC 局域网广播名参数化（房间码 `#MOTD` 段携带 host 存档名 → guest 广播真实世界名；`OnlineConfig.advertise_motd` 支持自定义广播名，优先级 自定义 > host 世界名 > 默认文案）
- **联机遗留**：跨网联机依赖协调服务器（默认社区节点 `tcp://node.invalid:11010`，easytier 官方公共节点已下线）；MC 26.2 离线账号只能走"局域网发现"路径进入（手动 IP 会报无效会话，广播模拟已绕过）
- **备注**：README.md 是完整中文文档（非空）；AGENTS.md 已被 git 跟踪（修改会出现在 git status）
