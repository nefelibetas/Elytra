# Repository Guidelines

## Project Overview

Elytra Launcher — 基于 **Tauri v2（Rust 后端）+ Vue 3（TypeScript 前端）** 的 Minecraft 启动器。功能：游戏版本下载（官方/国内镜像多源）、ModLoader 安装（Fabric/Quilt/Forge/NeoForge/OptiFine）、微软账号登录（OAuth PKCE）、离线模式（固定随机默认皮肤）、**皮肤衣柜**（3D 全身预览 + 微软账号皮肤/披风管理 + 本地皮肤收藏；第三方站点衣柜不内置，点头像用浏览器打开站点衣柜）、**JDK 扫描 + managed runtime 自动下载**（Mojang 官方 → Azul 双源回退）、Mod 双源搜索与**依赖解析**、下载任务管理、启动参数构建与游戏进程管理、**局域网联机**（easytier 库内嵌虚拟局域网 + 端口转发，房间码互连，无需公网服务器）、**工具箱多线程下载器**（URL 队列并发 + 单文件 HTTP Range 分段，断点续传，独立于启动器下载任务）、**工具箱种子预览**（输入世界种子 → cubiomes 群系瓦片 + 结构候选 + 结构与史莱姆区块快速查找，正式版参数族）、**工具箱世界预览**（选存档文件夹递归发现世界，内嵌全页世界预览并检索群系/史莱姆区块）。

后端 **160 个 `#[tauri::command]`**（分布于 21 个命令模块），前端 **10 个页面**（`src/pages/`），无状态管理库（模块级 `reactive()` 单例）。

## Architecture & Data Flow

```
前端 Vue3 (src/)  ──invoke(snake_case)──▶  commands/（薄胶水层，21 模块 / 160 命令）
    ▲                                          │ 读 persist 配置、调领域函数
    │ task-progress / ms-login-complete /     ▼
    │ game-exited 事件（emit 顺序队列线程）  downloader/（传输层）  game/（内容与装配）  auth/
    └──────── 返回 Result<T, String> 或裸值   task/（任务编排）launcher/ jdk/ online/
                                              config/（纯数据）persist/（TOML + secrets）
```

- **后端分层**：`commands/` 只做胶水（读配置 → 调领域函数 → `map_err` 展平 String）；领域逻辑按域归位——**传输层 `downloader/`**（`fetcher/{api,core,routes,redirect}.rs` 下载核心与公开入口、`batch.rs` 并发批处理（派发循环与 worker 均检查取消；并发许可在派发循环内获取，失败计数用原子累加以便 `max_errors` 真正止损）、`probe.rs` 源探测 TTFB/吞吐、`path_lock.rs` 跨任务路径互斥、`hash/progress/proxy/reuse/endpoints`；**不得依赖 `task`/`launcher`/`jdk`/`game`**），**内容与装配 `game/`**（`version/` 版本元数据与本地目录、`resource/` 资源与支持库装配、`loader/` ModLoader 安装、`community/` 社区资源搜索、`modpack.rs` 整合包、`updates/` Mod 更新），任务编排 `task/`（`manager.rs` TaskManager 单例；任务 id 带进程前缀（启动毫秒+pid），避免重启后与崩溃残留快照行撞主键 / `emit.rs` 事件队列 / `spawn.rs` 任务注册 / `runner.rs` 下载主体 / `progress.rs` 进度与终态收口 / `store.rs` SQLite 快照），启动 `launcher/`（`args.rs` 命令构建 / `env.rs` 环境准备 / `process.rs` 进程生命周期与监听 / `script.rs` 启动脚本渲染 / `skin.rs` 离线皮肤 / `platform.rs` 平台调节），账号 `auth/`（`microsoft.rs` OAuth / `mojang.rs` 官方 API / `account.rs` 回调与披风 / `external.rs` 第三方皮肤站 / `model.rs` MsAccount），纯数据 `config/`；统一持久化 `persist/`（`config.rs` section 读写 / `store.rs` 低层文件 IO / `secrets.rs` 密钥；`util::config_dir()` 三端统一走平台用户数据目录：Windows `%APPDATA%\Elytra`、macOS `~/Library/Application Support/Elytra`、Linux `$XDG_DATA_HOME/Elytra` 缺省 `~/.local/share/Elytra`（exe 旁在只读安装位置不可写、多用户共用会被 `secrets.bin` 里的凭证互相污染）——dev/test 进程在 cargo target 下仍走 exe 旁保持隔离；TOML 分 section，全局 Mutex 串行化）；联机 `online/`（`service.rs` 单例与共享状态 / `host.rs` / `guest.rs` / `session.rs` easytier 会话 / `scaffold/` 协议编解码与房间码）；跨域小工具 `util/`（纯函数，按职责拆子模块）
- **事件推送**：`task/emit.rs` 的 `DISPATCH`（LazyLock + 独立 `task-progress-emit` 线程）**顺序派发全部事件**（task-progress / task-waiting-user / ms-login-complete / game-exited / game-window-visible / online-state / tooldl-progress / auto-task-progress / auto-tasks-finished / versions-changed / version-dir-changed）——`app.emit` 若因 webview 未消费而阻塞只占派发线程，任务线程/进度锁永不碰 IPC（卡死与取消失效的根治）。其中**高频快照类**（`task-progress` / `tooldl-progress` / `auto-task-progress`）按**合并键**只保留最新一帧（payload 是完整快照、非增量，终态即最后一次写入故不会被丢；`auto-task-progress` 的键为「实例 + 阶段」，避免 pre_launch 残留帧顶掉 post_exit 进度；键含事件名是为避免进度帧顶掉 `task-waiting-user` 的弹窗帧），余下低频事件 FIFO 直投 —— 这样 webview 卡住时内存有界（否则整合包安装场景按「时长 × 频率 × 帧大小」线性上涨）
- **锁约定**：全后端 `parking_lot`；**统一 inner→progress 锁序**（cancel/list/fail 先 inner 后 progress），调用 `fail()` 前必须 `drop` progress 锁（防重入死锁）；跨任务路径互斥独立为 `downloader::path_lock`（自持全局表，不挂 TaskManager）；`cancel()` 的 DB 持久化移出锁；DB（sqlite）写统一 2s 超时
- **终态收口唯一入口**：`task/progress.rs` 的 `set_done` / `set_error` / `set_cancelled`——三者内部统一「发终态事件 → 移除任务注册」，且 `set_error`/`set_cancelled` 会取走此前 `set_cleanup_path` 登记的目录做**后台**整删（禁止在任务线程里同步 `remove_dir_all` 大目录）
- **配置联动**：`gamedirs::set_active_game_dir` 写 downloader section 的 `minecraft_dir`，launch/downloader 都读该字段
- **CurseForge Key**：运行时优先读 `secrets.bin` 的用户 key，其次 `constants::CURSEFORGE_API_KEY`（构建期 `.env`）；用户 key 只经命令写入，不返回前端
- **目录变化监听**：`game/version/watch.rs` 进程级单例（`notify`，非递归多目标）——跟随激活目录监听 `versions/`（版本列表）与聚焦版本的一级资源子目录（`saves`/`mods`/`resourcepacks`/`datapacks`/`shaderpacks`）；300ms 静默窗口合并后推 `versions-changed`（payload 带最新全量列表）与 `version-dir-changed`；`set_active_game_dir` / `auto_activate_version` 触发重定向，`set_watched_version` 由前端选中版本驱动聚焦；前端 `gameDirs.ensureVersionsWatch()` 仅在窗口可见时应用（不可见只置脏，`visibilitychange` 时补拉），`versionDetail.watchVersionDir()` 按 kinds 刷新 mods/存档列表
- **下载源模式三态**：`game_source_mode`（游戏下载源，统管版本列表 / 版本 json / 资源 / 支持库 / ModLoader 构件与安装器 / Mojang JDK runtime：OfficialOnly / MirrorOnly / Auto 默认——按健康度自适应路由，元数据官方优先 4s 降级镜像）、`community_source_mode`（Modrinth 官方 vs MCIM 镜像）；**网络代理三态** `network_mode`（Auto 读系统代理 / System 强制 / Direct 直连）。natives 新旧两代：旧式（≤1.12）`process_natives` 解压到 `.minecraft/natives/<ver>/`（`.elytra-ready` marker）并设 `-Djava.library.path`；新式（1.13+）native JAR 作为普通库进 classpath

## Key Directories

| 路径 | 职责 |
| --- | --- |
| `src-tauri/src/downloader/` | **传输层**（URL → 校验过的字节，不认资源类型）：`fetcher/{api,core,routes,redirect}.rs`（公开 fetch 变体 / 多源下载核心（`MIRROR_SEM` 把镜像与高并发不稳的官方 maven 统一压到 `constants::MIRROR_CONCURRENCY = 8`；`segmented` 对镜像源强制单连接；**任何自建 client 必须带 `UA_DOWNLOAD`——统一走 `redirect::route_client`，BMCLAPI 302 到的 CERNET/USTC 节点对无 UA 请求直接 403**）/ 镜像候选与健康度排序 / 黑洞 302 拒绝与安全跟随）、`batch.rs`（并发批处理：本地复用优先 + sha1 校验 + 路径互斥 + 逐文件进度）、`probe.rs`（HEAD 测 TTFB 与流式测吞吐，共用 TTL 缓存）、`path_lock.rs`（跨任务目标路径互斥）、`hash.rs`（sha1/sha512/流式）、`progress.rs`（`ProgressCounters`）、`proxy.rs`（系统代理探测）、`reuse.rs`（本地运行时复用）、`segmented.rs`（**单文件 HTTP Range 分段下载**：206 探测 / 多段并发随机写 / 段内字节级断点续传 / 跨段共享限速；镜像源与小文件自动回退单连接）、`endpoints.rs`（网络拓扑单一事实源；镜像路由表 `MIRROR_ROUTES` 行序即优先级——**NeoForge 的 `maven.neoforged.net/releases/…` 在 BMCLAPI 侧必须去掉 `/releases` 段（写成 `/maven/…`），保留即 404**，实测 21.1.248 的 51 条 URL 只有去段后 51/51 可达） |
| `src-tauri/src/game/` | **内容与装配**（特化场景，依赖 `downloader` 的传输能力）：`version/`（`model.rs` 版本 JSON serde / `meta.rs` 元数据获取 / `manifest.rs` 清单 / `dir.rs` 已安装版本目录域 / `source.rs` 源与镜像重写 / `watch.rs` 目录变化监听）、`resource/`（`assets.rs` 资源索引与批量下载 / `library.rs` 支持库 / `extract.rs` rules 匹配与 natives 解压，`TARGET_OS` 唯一实现）、`loader/`（`dispatch.rs` 统一入口 / `ctx.rs` 参数结构 / `shared.rs` profile 合并与继承展开 / `processors.rs` **installer processors 引擎**（按 installer 自己声明的 `install_profile.processors` 执行补丁链，不写死工具与配方）/ fabric·forge·neoforge·optifine·version_query）、`community/`（`api.rs` 出参与分派 / modrinth / curseforge / shared / translation / wiki_entry）、`modpack.rs`（整合包清单解析与安装计划）、`updates/`（Mod 扫描/识别/更新） |
| `src-tauri/src/map/` | **存档世界预览（只读 loaded region）**：`region_io.rs`（Anvil region/chunk 解析、旧格式回退、维度目录解析与自定义维度发现、区块表面群系采样）、`folder.rs`（任意文件夹递归发现世界根目录：level.dat / 26.x world_gen_settings，跳过库/日志等噪声目录）、`search.rs`（已加载地图群系列表/最近群系扫描）、`render.rs`（原版自然色 + Mod 方块覆盖层、俯视（`max_y` 向下扫描，降低可看洞穴/地下）、PNG 瓦片）、`mod_color.rs`（按需扫描实例 mods/*.jar 的 blockstates/model/texture 取顶面均色，失败走语义兜底）、`tile_cache.rs`（按世界/维度/region/最大 Y/mtime 落盘缓存）、`world_info.rs`（种子/DataVersion/Y 范围/维度列表）、`chunk_pos.rs`；不依赖 `tauri`，命令在 `commands/map.rs` |
| `src-tauri/src/seedmap/` | **工具箱种子预览域**：`version.rs`（正式版/补丁 → cubiomes 参数族，快照拒绝）、`ffi.rs`（C shim 安全封装）、`tile.rs`（512×512 群系瓦片 + 结构候选 + 最近结构/史莱姆查找）、`cache.rs`（复用 `mapcache`，键含数据版本）、`seed.rs`（数字 i64 / Java 字符串哈希）；只做主世界。C 源码在 `src-tauri/vendor/cubiomes/`（xpple fork `d934de1`，MIT；Windows MSVC 用 clang-cl） |
| `src-tauri/vendor/cubiomes/` | vendored cubiomes（活跃 fork，26.3/结构/地形 1.14+）；只保留核心/feature helper，不含 loot；`elytra_shim.c` 为 Elytra 专用稳定 C API |
| `src-tauri/src/task/` | 任务系统：`manager.rs`（TaskManager 单例 + 终态收口）、`emit.rs`（`DISPATCH` 派发线程 + 进度帧按任务合并 + 锁外 emit）、`spawn.rs`（任务注册与 spawn）、`ctx.rs`（`TaskCtx` 句柄结构）、`model.rs`（serde 任务配置 + `TaskProgress`/`StepInfo` 唯一定义）、`steps.rs`（StepSpec 分析）、`progress.rs`（EmitLoop/心跳/`set_*`/`StepRef` 分步进度：整包 B/C 段长步骤经它刷新 `steps[idx].progress`；**总百分比两种口径**——`update_progress` 默认优先字节占比，`HeartbeatStats::percent_by_steps` 走完成步骤数占比（整合包安装用：其 `est_bytes` 含永不进字节分子的估算，见该字段注释））、`runner.rs`（完整下载主体）、`repair.rs`（缺失扫描/`count_assets_all`）、`files.rs`（批量下载 + WaitingForUser）、`jdk.rs`（JDK 安装任务）、`modpack.rs`（整合包安装编排）、`store.rs`（SQLite 快照/恢复） |
| `src-tauri/src/tooldl/` | **工具箱多线程下载器域**（独立于 `task/` 的启动器下载任务）：`model.rs`（任务状态 + 对外快照）、`manager.rs`（任务注册表 + 并发槽位调度 + 暂停/继续/取消/清除）、`runner.rs`（单任务执行体：分段下载 + 250ms 进度心跳 + 终态收口）。复用 `downloader::segmented` 内核；事件出口走 `task::EventSink`（域层不依赖 `AppHandle`，可注入测试），进度以单任务快照事件 `tooldl-progress` 推送 |
| `src-tauri/src/config/` | 纯数据模型（无逻辑）：`downloader.rs`（多源/三态/`NetworkMode`/`DownloaderConfig`/`DownloadSource`）、`online.rs`、`game_dirs.rs` |
| `src-tauri/src/auth/` | 微软 OAuth PKCE（`microsoft.rs`）+ 官方 API（`mojang.rs`）+ 账号服务（`account.rs`）+ 第三方皮肤站（`external.rs`，authlib-injector）+ 本地皮肤收藏（`skins.rs`）+ MsAccount（`model.rs`，多账号列表，secrets.bin 加密） |
| `src-tauri/src/jdk/` | `scanner.rs`（本机 JDK 扫描；候选含**激活游戏目录**下 `<minecraft_dir>/runtime`，与安装位置同源）、`download.rs`（managed runtime 三源：Mojang all.json 逐文件 sha1 / Adoptium 包 sha256 / Azul 包 sha256，`on_attempt` 每次源尝试清零计数 + `on_bytes` 带传输总长当进度分母；**zip/tar 的目录条目名带尾 `/` 必须先跳过再校验路径**；Mojang `jre-legacy` 含零字节对象）、`config.rs`（配置读写/扫描注册/`SUPPORTED_JAVA_MAJORS`）、`model.rs` |
| `src-tauri/src/online/` | **局域网联机（EasyTier + Scaffolding 协议）**：`service.rs`（单例 + 共享状态 + 快照/终止；持有 `proxy`/`info_server` 停止句柄，`stop()` 先释放端口再做秒级的 `session.stop()`）、`host.rs`（建网 + 信息服务器 6 协议 + 游戏端口发现与实例重建）、`guest.rs`（加入/中心发现/本地代理；`gen` 贯穿到 `activate_guest_join`，回写前校验代际——否则发现/协商窗口内点停止后仍会回写并持续广播）、`session.rs`（easytier 会话：no_tun+smoltcp+KCP 数据面、host 固定虚拟 IP 10.144.144.1、guest DHCP、白名单 ACL）、`model.rs`（状态模型与协议常量）、`scaffold/`（`room.rs` 房间码 base34+模 7、`codec.rs` 帧编解码（帧体长度硬上限 `MAX_BODY_LEN`，分配前校验——长度字段是外部可控的裸 u32）、`profiles.rs` 玩家表、`server.rs` 信息服务器（`InfoServer` 句柄**必须被单例持有**：drop 才停 accept 线程并释放端口）、`identity.rs` machine_id）、`mc_discover.rs`（组播 224.0.2.60:4445 解析游戏端口）、`mc_advertise.rs`（guest 端模拟 MC LAN 广播，绕过 MC 26.2 离线会话校验） |
| `src-tauri/src/persist/` | `config.rs`（TOML section 读写 + 全局锁）、`store.rs`（低层文件 IO + 路径）、**`instance.rs`（实例配置 `versions/{id}/.elytra-instance.toml`：一期只承载 `[auto_tasks]`，文件存在即该实例完全采用自己的自动任务）**、`secrets.rs`（密钥加密：DPAPI / Keychain / Secret Service + ChaCha20-Poly1305；**原子写**（.tmp + rename，权限落 tmp 上）+ 解析失败备份 `.bak`；列表类数据的「读-改-写」一律走 `update_json_list`（整段持锁），不要用 read/save 两次调用拼） |
| `src-tauri/src/launcher/` | `args.rs`（命令构建）/ **`compat.rs`（版本 ↔ 所选 Java 的兼容判定唯一入口：下界按版本 json 的 `javaVersion.majorVersion` 对所有主版本通用（过低一律 Block）；上界只在跨 Java 8→9 时拦/提示——launchwrapper 主类（≤1.12.2 原版与 Forge）在 Java 9+ 必然崩（`Launch.<init>` 强转 URLClassLoader）故 Block，Java 8 时代的 Forge/NeoForge/OptiFine 只支持 Java 8 故 Warn，`disable_launch_wrapper` 启用时豁免）** / `env.rs`（环境准备 + JDK 匹配；`load_launch_config` 启动前按 `compat::check` 拦下必然失败的组合）/ `process.rs`（进程生命周期与监听 + **崩溃判定**：非零退出码 + 时间窗内崩溃证据（crash-report / 游戏 `logs/latest.log` / **启动器捕获的 `logs/elytra-launch.log` 本次偏移之后的新增段**，覆盖 JVM 在游戏日志成型前退出：JDK 不兼容、主类缺失、参数被拒）+ **启动取消闸门**：`begin_launch` / `request_cancel` / `note_launched` 由「取消标志 + 已拉起 pid」两个原子量协作，取消先置标志再取 pid，故与 pid 登记竞争不漏杀；进程退出即注销 pid，防后续取消误杀复用同一 pid 的进程）/ **`auto_tasks/`（自动任务：`model.rs` 模型 / `runner.rs` 执行器（内置在前、自定义在后，失败继续 + 汇总事件）/ `backup.rs` 存档备份（时间戳目录、保留 N 份）/ `record.rs` mod 清单 / `crashpack.rs` 崩溃日志打包（含脱敏）/ `redact.rs` 脱敏 / `ctx.rs` 上下文）** / `script.rs`（BAT/SH 渲染）/ `skin.rs`（离线皮肤）/ `platform.rs`（平台调节）/ `memory.rs`（内存与 GC 参数；`GcType::Custom` 不追加预设 GC 参数，由用户「Java 虚拟机参数」框自填）/ `model.rs`（`LaunchOptions` 含 `auto_tasks`，实例文件存在时由实例自动任务覆盖全局；natives 目录按 MC 版本共享 `natives/<mc>/`，`.elytra-ready` 存 native 集合指纹——指纹不一致即清空重建，避免同 MC 换版本时跳过解压导致缺原生库） |
|`src-tauri/src/commands/`|21 个命令模块（160 命令）：account(27) / community(6) / config(1) / downloader(5) / gamedirs(12) / **auto_tasks(4：实例自动任务读写 + 全局保存 + 立即执行一次)** / jdk(5) / launch(7：含 begin_launch / cancel_launch) / launch_options(3) / launch_script(1) / loader(6) / **map(15：世界信息/维度 region/瓦片/群系 + 文件夹递归发现/文件夹地图读取 + 已加载地图群系检索 + 史莱姆覆盖/搜索)** / **seedmap(8：版本列表/解析/瓦片/群系/结构候选/结构快速查找/史莱姆查找)** / offline_account(5) / online(6) / server_status(1) / skins(6，本地皮肤衣柜) / task(14) / tooldl(9，工具箱下载器) / version(15) / version_icon(4) —— 薄胶水，逻辑在各领域层|
| `src/pages/` | Launch（左栏启动 + 右栏实例卡片/联机条）/ Download（资源中心）/ **DownloadDetail（下载任务中心，顶级路由）** / ResourceDetail（资源详情+依赖链；提交下载/安装成功后自动回退一层——依赖链进入的回到上一层详情，从列表进入的回到资源列表）/ Settings（左侧 Tab + 工具 Grid；工具详情页内返回）/ VersionSelect / VersionSetup（容器化，业务在 store/组件）/ **WorldPreview（世界预览全页：仅解析版本/存档名，实现在 `component/map/WorldPreviewView`；真实 region、维度/可调最大 Y 俯视、浮动面板、群系/史莱姆检索）** / CustomThemeEditor / SkinStudio（皮肤衣柜：3D 预览 + 皮肤/披风/本地衣柜，**仅微软账号可进**） |
| `src/store/` | 模块级 reactive/ref 单例，**按域分包**：`config/`（`downloaderConfig`/`gameDirs`/`online`/`toolDownload`/`externalConfig`/`jdkOptions`/`launchOptions`/`autoTasks`（自动任务：全局/实例读写 + 进度/失败事件）/`configRecovery`）、`task/`（`download`（dlState 任务状态机 + `ensureListen` 幂等 + terminalTaskIds 防幽灵）/`downloadTab`/`versionDownload`/`versionSetup`/`versionDetail`/`versionIcon`）、`auth/`（`auth`/`accountUi`/`avatar`（FNV-1a 头像））、`skin/`（`skinStudio`（皮肤衣柜：3D 全身预览帧 + 官方皮肤/披风 + 本地衣柜）/`skinEditor`（皮肤绘画器：64×64 像素缓冲**刻意非响应式**，视图只订阅 `revision`）/`skinRender`）、`community/`（`communitySearch`/`categorySearch`（聚合/分页/四分类工厂）/`resourceDetail`）、`app/`（`events`（镜像 Rust `events.rs`）/`alert`/`theme`（四主题）/`nav`/`settingsTab`/`selectedVersion`/`memory`/`appLog`/`launch`（启动阶段/步骤 + 代际号取消守卫 `cancelLaunch`））。**鞘翅预览已移除**（画家算法在薄板共面/部分重叠场景的固有限制，收益低于维护成本；实现保留在 Git 历史 `78cdbe2` 可恢复）。 |
| `src/types/` | **自动生成的 TS 类型**（`ts-rs`，勿手改）：按域分包 `config/`、`task/`、`tooldl/`、`launcher/`、`jdk/`、`auth/`、`game/`、`community/`、`map/`、`seedmap/`。生成命令 `cd src-tauri && cargo run --bin gen_ts --features gen-ts`（该 bin 在 Cargo.toml 里挂 `required-features = ["gen-ts"]` **默认不构建**——tauri-cli 会把包内所有 bin 当附加二进制打进安装包，macOS universal 构建只产出主二进制，会把整个打包步骤搞挂）。登记清单 `src-tauri/src/ts_export.rs`，导出路径由 `src-tauri/.cargo/config.toml` 的 `TS_RS_EXPORT_DIR` 决定。改 Rust 数据结构后必须重跑生成，否则前端类型落后 |
| `src/component/` | 按域分目录：`common/`（TitleBar / AlertDialog / ScrollTopFab / IconImage / EmptyState / ErrorState / CheckToggle / DropSelect / LoadingSpinner / StepDot / MenuPopover）、`download/`、`launch/`、`settings/`、`tools/`（Give/Entity/Command/Motd 生成器 + Firework/Potion/VillagerTrade/Mods 编辑器（烟花/药水在 Give 里选中对应物品时内联出现）+ SearchSelect 搜索选择 + ServerStatus 工具 + **DownloadTool（多线程下载器）** + **SeedMapTool（种子预览：群系瓦片 + 结构候选）** + **MapFolderTool（工具箱世界预览：选文件夹递归发现世界，当前工具详情页内嵌 WorldPreviewView，不跳路由）** + **SkinPainterTool（皮肤绘画器：展开图逐像素 + 3D 直绘双模式，3D 模式下预览撑满中栏、左键落笔 / 抓手或右键拖动旋转、内层外层手动切换；展开图每部位独立配色标注；预览的透明像素垫棋盘底纹，且只垫第一层 —— 第二层空着时要透出内层，垫了就整块盖住）**）、`version/`、`version-setup/`、`map/`（WorldMapCanvas / WorldPreviewView）、`settings/`（含 `SettingsTools.vue`：工具 Grid + 原位详情；`AutoTasksEditor.vue`：自动任务编辑器，双模式复用——实例设置页编辑实例、设置页编辑全局）、`version-setup/` —— 页面组件直接 import store，无 props 链 |
| `src/utils/` | `error.ts`（**`errMsg()` 取错误首行**，toast/内联错误用）、`format.ts`（fmtCount/fmtSize/fmtModDate/fmtDateTime/fmtSpeed/fmtEta…）、`versionClassify.ts`（目录名 `{mc}-{loader}-{ver}` 解析）、`versionMeta.ts`、`give.ts`/`motd.ts`/`nbt.ts`（生成器算法）、`toast.ts`（全局提示队列：`showToast` 默认 4000ms / 同屏最多 5 条排队补位 / hover 暂停 / 手动关闭，App.vue 右上角堆叠渲染 + `role=status` 播报，`data-tabskip` 不入键盘遍历）、`focusNav.ts`（键盘焦点遍历：Tab/Shift+Tab 按文档序移动、弹窗内自动收窄成焦点陷阱、`focusDialog`/`restoreFocus` 供弹窗接管焦点；焦点环由它内联写入——内描边、标题栏内改用前景色，`.kb-focus` 仅作状态标记）、`tooltip.ts`、`useConfigSection.ts`、`useSelectable.ts`、`useConfirmDelete.ts` |

## Development Commands

```bash
pnpm tauri dev      # 主开发命令：vite :1420 + Rust dev 窗口（Rust 改动需手动重启才生效）
pnpm tauri:build    # 本地发布构建（vue-tsc + lint:css + cargo release + bundle）
pnpm tauri build    # 仅打包（beforeBuildCommand 只跑 vite build；CI 前置检查由 check job 负责）
pnpm dev            # 仅 vite dev server（:1420 strictPort）
pnpm build          # vue-tsc --noEmit + lint:css + vite build → dist/
pnpm lint:css       # CSS 兼容性检查（stylelint，基线见 package.json browserslist）
pnpm i18n:check     # i18n 门禁：zh/en key 对称 + 占位符一致 + 后端 key（含 keyed_msg）+ 前端静态 t() key + 裸中文 + 后端未本地化错误首行（存量见 scripts/i18n-rust-plain-baseline.json，新增即失败；`node scripts/i18n-check.mjs --update-baseline` 重写基线）
cd src-tauri && cargo fmt                # Rust 格式统一（改 Rust 后必跑）
cd src-tauri && cargo check              # Rust 编译检查
cd src-tauri && cargo clippy --lib       # Rust lint（保持零代码警告）
cd src-tauri && cargo test --lib -- --test-threads=1   # 单元测试（当前 431 passed + 8 ignored（OptiFabric 真实下载 + NeoForge 21.x/26.x 与 Forge 1.20.1/1.12.2/1.7.10 安装链端到端 + 安装矩阵 2 个：loader 9 形态 / OptiFine 5 组合）；串行防 persist/网络竞争）
cd src-tauri && cargo run --bin gen_ts --features gen-ts    # 生成前端 TS 类型 → src/types/（改 Rust 数据结构后必跑；不在测试里生成）
cd src-tauri && cargo test --test online_diag          # 联机集成测试（host/guest 双实例回环，走真实 easytier）
cd src-tauri && cargo test --test tool_download        # 工具箱下载器集成测试（本地 HTTP 服务驱动完整链路：分段/暂停续传/取消/回退）
cd src-tauri && cargo test --test e2e_download -- --ignored   # run_task 端到端（真实网络：取消路径秒级 / 完整下载分钟级）
cd src-tauri && JAVA_HOME=<jdk> cargo test --lib -- --ignored --test-threads=1 matrix   # ModLoader 安装矩阵（loader 9 形态 + OptiFine 5 组合；报告落 target/e2e-loader/report-*.md，单跑某几条用 ELY_E2E_CASES=名字,名字）
npx vue-tsc --noEmit    # 前端全量类型检查（可在项目根直接跑）
```

**网络测试**（走真实外网单发小请求，离线环境会失败或跳过，可接受）：`fetch_asset_index_end_to_end`（`game/resource/assets.rs`）、`count_assets_all_end_to_end`（`task/repair.rs`，真实索引 + 5057 项多线程扫描）、`global_client_uses_system_proxy_when_enabled`（`downloader/proxy.rs`）、`bmclapi_optifine_real_download`（`downloader/fetcher/redirect.rs`，标 `#[ignore]`，这正是 `cargo test --lib` 里那 1 个 ignored）。

## Code Conventions & Common Patterns

### Rust 后端

- **格式化**：改 Rust 文件后必须跑 `cargo fmt`；提交前 `cargo fmt --check` 应零差异（全仓统一，不手调换行）
- **命令命名**：snake_case、动词开头（`get_/save_/set_/fetch_/scan_/launch_/spawn_/start_/cancel_/remove_`…）
- **返回**：可失败 → `Result<T, String>`；纯读取 → 裸 `T`/`Option<T>`；错误在模块边界 `.map_err(|e| e.to_string())`
- **错误消息规范**：**中文短文案首行 + `\n` + 技术细节**（如 `下载资源索引失败\n{e}`、`请求失败\nHTTP {status}`）；前端 toast/内联错误经 `errMsg()` 取首行，AlertDialog（带复制）显示全文
- **错误本地化**：用户可见错误首行走 `@i18n` 协议——`bail_key!` / `common_err_ctx_key!`（Result 路径）与 `keyed_msg!`（事件/状态等非 Result 路径，支持 `?k=v` 参数插值）；首行 key、次行中文兜底、其后技术细节。聚合错误里内嵌的 `@i18n:key?k=v` token 也会被 `formatError` 就地翻译；**`?k=` 参数必须与文案占位符完全一致**（门禁校验）。存量未 keyed 的中文首行锁在 `scripts/i18n-rust-plain-baseline.json`
- **错误构造走宏**（`src/macros.rs`，经 `#[macro_use] mod macros` 全局可用，**全部走 `@i18n` 协议**）：`common_err_ctx_key!` 生成 `.map_err` 闭包；`bail_key!` 早返回；`keyed_msg!` 产出非 `Result` 路径的本地化文本（事件负载/状态串）。三者都用 `$($t:tt)*` 原样转发，支持内联捕获与位置参数；**ctx/文案含内联捕获时必须走位置参数形态**（`common_err_ctx_key!("errors.x", "下载 {} 失败", x)`），写成 `"下载 {x} 失败"` 的裸字符串形态会退化为字面量输出（静默错误）。未 keyed 的 `common_err_ctx!`/`bail!` 已删除，别再加回来（门禁会拦未 keyed 的中文错误首行）
- **section 读写走宏**（`persist/accessors.rs` 的 `section_accessors!`）：为配置 section 生成 `xxx_config()` / `save_xxx_config()`（支持自定义默认形态 `=> T, SECT, <默认表达式>`）；新增配置 section 优先加进宏，而非手写 `persist::read(SECT).unwrap_or_default()`
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
- **版本要求的 Java 是硬下界**：`launcher/compat.rs::check` 是启动前兼容判定的唯一入口——`javaVersion.majorVersion` 低于要求（任意主版本：8/11/16/17/21/25 一视同仁）一律拦；上界只对跨 Java 8→9 的老工具链生效（launchwrapper 拦、8 时代 Forge/OptiFine 提示），因为 Java 9 起向后兼容旧字节码，给新版本配更新 LTS 是正常用法
- **零字节对象合法**：清单声明的 sha1 等于空串 sha1（`downloader::hash::SHA1_EMPTY`）时，200 空响应按 0 字节文件写入并通过校验（Mojang `jre-legacy` 的 `lib/security/trusted.libraries` 就是这种条目）；未声明的空响应仍判失败（黑洞防护）
- **总进度与步骤进度同口径**：`progress::update_progress` 计算总百分比时会同步单步任务的 `steps[0].progress`——两套分母会让卡片出现「总 99% / 步骤 0%」；多步任务的步骤进度仍由 `StepRef` 独立维护
- **serde_json 的字符串索引在"读"时也会写入**：`value["missing"]` 会把 `"missing": null` 插进对象。凡"只看一眼"的探测一律 `get`/`get_mut`，只有确实要赋值时才用索引。踩过的坑：`merged["arguments"].as_object_mut()` 让老版本（1.7.10/1.8.9）合并出的版本 JSON 多出 `"arguments": null`，读取端 `VersionJson.arguments`（老版本本就没有该字段）反序列化直接失败——`无效类型 null`。故该字段为 `Option<Arguments>` + `jvm_args()/game_args()` 访问口（`#[serde(default)]` 只兜"缺失"，兜不住显式 null）
- **loader 库条目的两种形态都要能进 classpath**：老式 profile/spec-0 条目标 `{name, url}`、spec-0 的 `versionInfo.libraries` 甚至只有 `{name}`，都没有 `downloads.artifact`（classpath 与缺库检查都从 `artifact.path` 取路径 → 缺失即静默丢出 classpath）。`shared::normalize_legacy_libraries` 按坐标补 path/url（无 url 走 `MOJANG_LIBRARIES` 兜底）；**只有 `classifiers` 的 natives 条目绝不合成普通构件**（实测会去下 CDN 上不存在的 `*-platform-x.y.jar` → 404）
- **installer 式 loader 安装必须把整份 `install_profile.libraries` 落盘**：Forge 1.17+ 的运行时模块 jar（`fmlcore` / `javafmllanguage` / `lowcodelanguage` / `mclanguage`）**只在 install_profile 声明**，version.json 里只有 `fmlloader`/`fmlearlydisplay`——只下 version.json 的库，游戏启动会在 `MinecraftLocator.scanMods` 上报 `Invalid paths argument, contained no existing paths: [.../fmlcore-x.y.z.jar]` 秒退（退出码 1），而安装任务却"成功"。官方 installer 正是把这份清单全量装进 `libraries/`；映射在 `forge::profile_libraries`（URL 为空的条目由 installer 内嵌流程落盘，须过滤掉否则批量下载会判"网络无法下载"而中止安装），安装收尾另有 `forge::verify_profile_artifacts` 把清单里**每一个**构件的落盘情况核一遍（含内嵌条目），漏项直接报安装错误——不让"安装成功、启动秒退"这种假成功流出去。矩阵用例对每条 installer 用例断言该清单已落盘（`missing_libs`）
- **外部输入的落盘路径**一律过 `util::safe_relative_path`（拒绝绝对路径 / `..` / 空段），sha1 前两位子目录走 `util::sha1_sub_dir`（不裸切 `hash[..2]`）
- **子进程一律无窗口（铁律）**：应用运行期**不得弹出控制台窗口**。所有 spawn 点（`std::process::Command` / `tokio::process::Command`：java 探版本、Forge/NeoForge installer processors、游戏进程、`cmd /C` 自动任务、`taskkill`…）必须经 `util::process::NoWindow` 装配——`cmd.no_window()`；需附加标志（`CREATE_BREAKAWAY_FROM_JOB` / `CREATE_NEW_PROCESS_GROUP`）时用 `cmd.no_window_with(extra)`，**不要自己再调 `creation_flags`**（覆盖式设置会丢掉 `CREATE_NO_WINDOW`）。漏掉的典型症状：安装 Forge/NeoForge 时连弹多个命令行窗口，几十毫秒后自行消失

### 前端 Vue/TS

- **IPC**：命令名与参数/返回值字段全 snake_case；**`invoke` 一律封装成方法**——`invoke` 只允许出现在 store/封装层（组件、页面禁止裸调），泛型类型也只写在封装层
- **TS 类型镜像（自动生成，勿手写）**：前端与后端共享的数据类型（config / 事件 payload / 命令 DTO）由 Rust 侧 `#[derive(ts_rs::TS)]` + `#[ts(export_to = "<域>/")]` 生成，产物在 `src/types/<域>/*.ts`。
  - 生成命令：`cd src-tauri && cargo run --bin gen_ts --features gen-ts`（**不在测试里生成**，避免跑测试误改工作区）；新类型登记进 `src-tauri/src/ts_export.rs`
  - **改了 Rust 字段/类型必须跑一次生成**，否则前端类型落后（生成物 `src/types/` 入库，diff 可 review）
  - 前端 import 生成类型（`import type { X } from "../types/<域>/X"`）；前端名与 Rust 名不一致时用别名 `export type { RustName as FrontendName }`
  - 生成注意：`#[serde(skip_serializing)]` 需配 `#[ts(skip)]`；`u64`/`i64` 需 `#[ts(type = "number")]`（否则生成 `bigint`），`Option<u64>` 用 `#[ts(type = "number | null")]`
  - `invoke` 的泛型用生成类型：`return invoke<X>(...);`（封装层内）
- **跨组件缓存必须放普通 `<script lang="ts">` 块或 store 模块级**——`<script setup>` 顶层变量每次组件重建（路由切换）重置（Launch.vue 头像缓存、Download.vue manifest 缓存为范例）
- **状态**：模块级 `reactive()`/`ref()` 单例导出，页面直接 import；无 Pinia/Vuex
- **事件监听幂等**：`ensureListen()` 用 listening 标志防重复注册；终态（done/cancelled/error）事件用 `terminalTaskIds` 防幽灵卡片复活
- **样式**：CSS 变量统一在 App.vue `:root` + 四套 `data-theme` 主题覆盖块（`--color-brand`/`--text-*`/`--surface-*`/`--ld-*` loader 标签色等），组件页面一律 `var()` 引用、`<style scoped>`；图标用 `@lucide/vue`，无自研图标库
- **三端 WebView 一致性（保守用现代 CSS）**：渲染内核三端不同——Win WebView2（Blink）/ macOS WKWebView（WebKit）/ Linux WebKitGTK（WebKit，发行版版本滞后，如 Ubuntu 22.04 仅 2.40）；新 CSS 特性一律 `@supports` 包裹或直接写兼容写法，**禁止裸用**；中文字体栈固定三端兜底链（`"Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", "Noto Sans SC", sans-serif`），禁止单独写死平台字体。降级范例：ResourceDetail.vue `.md-ld-*` 徽章 color-mix、AlertDialog.vue 遮罩 backdrop-filter。**基线**：Chrome 111 / Safari 16.4（≈ WebKitGTK 2.40），由 `package.json` 的 `browserslist` 与 `vite.config.ts` 的 `build.target` 固定（macOS 打包下限 `minimumSystemVersion 13.0`）；新增 CSS 必须过 `pnpm lint:css`（stylelint + no-unsupported-browser-features）。该检查对 `color-mix()` / `backdrop-filter` / `scrollbar-gutter` 是盲区（caniuse 缺条目或只标部分支持），这三类仍须手写 `@supports` 或双声明降级
- **错误提示两级**：toast（`utils/toast.ts` 的 `showToast(msg, durationMs=4000)` + App.vue 渲染 `toasts` 队列：右上角堆叠、同屏最多 5 条、hover 暂停、可手动关闭）vs AlertDialog（复杂错误，带复制按钮）；均经 `errMsg()` 取首行
- **键盘与焦点**：Tab/Shift+Tab 走 `utils/focusNav.ts` 的应用自管遍历（文档序；弹窗打开时范围收窄成焦点陷阱，`data-tabskip` 子树跳过），不用浏览器原生遍历；焦点环由该模块**内联写入内描边**（组件 scoped `outline: none` 会盖掉样式表写法）——不要再给按钮加 `:focus-visible` 外轮廓，正偏移的外轮廓会被看成按钮外的异色细边；弹窗需 `role="dialog" aria-modal="true"` 并经 `focusDialog`/`restoreFocus` 接管焦点，提示类通道用 `role="status"`
- **展开/收起必须有动画**：一律 `<Transition name="expand">` + `.expand-enter-to/.expand-leave-from { max-height: 1000px }` 显式目标值（max-height 写在 active 类上会因同特异性覆盖 from 态导致**无动画**，勿回退）
- **社区资源搜索**：搜索表单 + 结果列表组件成对复制（绑各自 store）；聚合模式每源只取 `AGG_PAGE_SIZE=15` 条按标题去重；进入下载页 onMounted 并发预加载全部社区 tab 热门（`ensureAutoSearch` 幂等）；左侧 Tab 刷新按钮 = 各 store 的 `reset()`
- **路由**：hash 路由（`createWebHashHistory`），`RouteMeta.transition` + `routeLevel`/`tabOrder` 驱动页面动画；DownloadDetail 为**顶级 tab**（routeLevel 0），无全局 FAB

### 注释规范（全仓通用：Rust / TS / Vue）

**一句话**：注释只写「代码读不出来、但下一个改这段代码的人必须知道」的东西。

- **只写约束，不写故事**。允许：不变量、边界与数值口径、平台/协议差异、外部系统硬约束、必须与哪处保持同步、安全要求（路径校验/脱敏/加密/凭据）。
- **禁止**：
  - 复述代码——把紧接着的几行用中文再说一遍；
  - 项目背景与开发历程；**回归/来历叙事**（「回归：曾…」「此前…导致…」「踩过的坑」「实测过一次」「已修复/已移除/本次」）一律不写——踩坑沉淀出的约束只写约束本身；
  - 叮嘱语气与口语（「务必/切记/勿随手改」「幽灵卡片复活」「炸档」「顺手」「别改回…」）→ 改客观陈述句。
- **长度**：单个条目的注释（`//` / `///` / `/** */`，含续行）**≤3 行**（并列多条独立事实时最多 4 行）；模块头 `//!` **≤6 行**。写不下说明该拆结构或该抽文档，不是把注释写长。
- **magic number 带口径**：写单位与作用（如「超时 4s：官方源实测 2.5–4.7s，2s 会误判降级」），但不写发现过程。
- **结构标记**：分节用 `// ── X ──`（独占一行，标题即内容）；`///` 写「是什么 + 非显然约束」，不要复读签名（参数名即说明）。
- **语言**：中文；标识符、协议/API 名、CLI 命令保留原文。测试内注释同样适用（只写外部约束与不变量，不写步骤复述）。

```rust
// ✗ 回归：曾对本地源也返回拼好的缓存路径，open_zip 打开不存在的 modpacks/xxx.zip
//   报「系统找不到指定的路径。(os error 3)」，本地导入完全不可用。
// ✓ 仅在线源落 `modpacks/{key}.{ext}`；本地源直接用用户选定的文件。
```

## Important Files

| 文件 | 说明 |
| --- | --- |
| `src-tauri/src/lib.rs` | 入口：`task::init()` / `tooldl::init()`、后台预扫 JDK、**唯一** generate_handler! 注册点（160 个 tauri::command） |
| `src-tauri/src/main.rs` | 仅 `elytra_lib::run()` |
| `src-tauri/src/constants.rs` | APP_NAME/UA/MIRROR_CONCURRENCY、`option_env!` 注入 CURSEFORGE_API_KEY（.env 经 build.rs）、内置公开常量 MS_CLIENT_ID、persist SECT_* 段名 |
| `src-tauri/src/events.rs` | 事件名常量（与 `src/store/app/events.ts` 手写镜像同步） |
| `src-tauri/src/task/manager.rs` | TaskManager 单例（create/pause/cancel/fail/remove/list + 挂起决策 + `LazyLock` 单例） |
| `src-tauri/src/task/emit.rs` | `DISPATCH` 派发线程（进度帧按 `(事件, 任务)` 合并 + 低频事件 FIFO）+ `emit_event` 泛型派发 + `update_and_emit`（锁外 emit） |
| `src-tauri/src/task/progress.rs` | 进度上报（EmitLoop/心跳/`set_*`）+ **终态收口唯一入口** `set_done`/`set_error`/`set_cancelled` + **百分比口径**（`update_progress` 优先字节占比；单步任务同步 `steps[0].progress` 到同一口径） |
| `src-tauri/src/task/store.rs` | SQLite `install_jobs` 快照 + `block_on`（RuntimeFlavor 分支防 worker panic）+ 2s DB 写超时 + crash→interrupted 恢复 |
| `src-tauri/src/downloader/fetcher/core.rs` | 多源下载核心：`FetchOptions`/`download_core`/重试与换源/黑洞拒绝/镜像并发信号量 |
| `src-tauri/src/downloader/probe.rs` | 源预探测：HEAD 测 TTFB（选源）+ 流式测吞吐（JDK 大包选源），共用 TTL 缓存 |
| `src-tauri/src/launcher/args.rs` | `${key}` 占位符替换、rules/features 匹配、classpath、JDK22+ 标志 |
| `src-tauri/src/jdk/download.rs` | managed runtime 三源安装（Mojang all.json 逐文件 sha1 / Adoptium 包 sha256 / Azul 包 sha256）、`find_java_home` 递归、`InstallProgress` 进度钩子（`on_attempt` 换源清零 + `on_bytes` 带传输总长） |
| `src-tauri/src/auth/microsoft.rs` | 微软 OAuth（授权码/refresh/XBL/XSTS/MC login）、回调服务 localhost:27205 |
| `src-tauri/src/online/session.rs` | easytier 会话封装：no_tun+smoltcp+KCP 数据面、bind_device=false、enable_kcp_proxy=true（KCP 数据面首选，缺则 smoltcp 传大数中断） |
| `src-tauri/src/online/mc_advertise.rs` | guest 端 MC 局域网广播模拟（224.0.2.60:4445，绕过 MC 26.2 离线会话校验） |
| `src-tauri/src/util/` | 跨域纯函数（按职责拆分子模块，`mod.rs` 重导出、`crate::util::xxx` 路径不变）：`dirs.rs`（`config_dir()`/平台目录基准）/`path.rs`（`safe_relative_path`/`sha1_sub_dir`/`unique_child_dir`）/`text.rs`（`fnv1a64`/`is_simple_name`）/`version_name.rs`（`infer_loader_from_json`）/`net.rs`/`icon.rs`/`log.rs`/`time.rs`/`rt.rs`/`process.rs`（`NoWindow`：子进程无窗口装配，铁律——所有 spawn 必过） |
| `src-tauri/tauri.conf.json` | 窗口 **1008×648**（840×540 的 1.2 倍）、无边框透明、`beforeDevCommand: pnpm dev`、devUrl :1420；CSP 的 `img-src` 需放行社区 CDN（`cdn.modrinth.com` / `*.forgecdn.net` / `mod.mcimirror.top`），否则远程图标被静默拦截 |
| `src-tauri/capabilities/default.json` | main 窗口权限：core:default + window 控制子集 + opener + dialog |
| `src/store/task/download.ts` | 下载任务状态机（事件驱动核心） |

## Runtime/Tooling Preferences

- **包管理器必须 pnpm**：`beforeDevCommand` 硬编码 `pnpm dev`；`pnpm-lock.yaml` **入库**（工具链前沿，锁版本保证构建可复现；CI 与本地一律 `pnpm install --frozen-lockfile`）。pnpm 版本见 `mise.toml`（node 24 / pnpm 12）
- **工具链版本基准是 `mise.toml`（推荐使用 mise）**（node / pnpm / rust / protoc）：本地推荐 `mise install`，也可手动安装对应版本；CI（`.github/workflows/release.yml`，推 `v*` tag 才跑）用 `jdx/mise-action` 读同一份文件、**不写死任何版本号**。改动工具链 = 改 `mise.toml` 一处。各 job 另设 `MISE_RUSTUP_HOME` 指向 mise 数据目录——rust 实体在 rustup home 里，不放进被缓存的 mise 目录会出现「缓存命中但工具链缺失」
- **工具链很新**：vite 8、typescript 6、vue-tsc 3、vue-router 5——新增依赖前确认兼容（@lucide/vue 图标库已引入）
- **窗口**：1008×648（840×540 的 1.2 倍）、`decorations:false` + `transparent:true` + `resizable:false`——前端自绘 TitleBar（`startDragging`）
- **Rust 变更需重启 `pnpm tauri dev`**（vite `watch.ignored` 忽略 src-tauri）；前端改动 HMR 即时生效
- **前端大量改动后建议重启 dev**：跨多个 store/模块的批量改动、或触发依赖预构建失效时（vite 日志出现 `Re-optimizing dependencies because vite config has changed`），HMR 可能停在**半旧半新**状态——表现为「组件逻辑更新了但 store 还是旧的」这类诡异现象（实测过一次：资源详情页左上角返回键点了完全没反应，重启 dev 即恢复）。**排查前端怪状前，先重启一次 dev 排除这个因素**
- **reqwest 0.13 无 `.form()`**：表单需手工 urlencode（auth 里 `form_body`）
- **MS_CLIENT_ID 已内置公开常量**（不再是 `.env`）；回调固定 `http://localhost:27205`，换 Azure 应用时同步改常量与注册地址
- **lib 名 `elytra_lib`**：Windows 下 bin/lib 同名会 Cargo 8519，保留 `_lib` 后缀
- **版本号**：tauri.conf.json / Cargo.toml / package.json 三处一致（当前 1.0.3），改动需同步
- **capabilities 是 v2 权限门**：新增 JS 侧 window/fs/dialog API 必须先在 `default.json` 加权限
- **打包与卸载**：`bundle.targets` 列全了 `deb` / `rpm` / `appimage` / `app` / `dmg` / `nsis` / `msi`（tauri-bundler 的 `package_types()` 会与目标平台原生集合求交过滤：Windows 只出 nsis+msi、macOS app+dmg、Linux deb+rpm+appimage，配一份即可三平台通用）。Tauri 的 NSIS 卸载器是**逐文件 `Delete` + `RMDir "$INSTDIR"`（非递归，只删空目录）**——游戏目录（`.minecraft`）虽在安装目录内但非空，**卸载不会删游戏数据**；`src-tauri/windows/hooks.nsh` 的 `PREUNINSTALL` 仅做告知（`IfSilent` 跳过，避免静默卸载卡住）。另一处坑：`identifier`（`xyz.bridgefish.elytra`）与配置目录名（`constants::CONFIG_DIR = "Elytra"`）不同，Tauri 原生「删除应用数据」复选框实际删的是 `%APPDATA%\<identifier>` 这个并不存在的目录，故 `POSTUNINSTALL` 按真实路径补删，避免「说删没删」。**MSI 默认装 `Program Files`（只读）**，`default_game_dir_base` 对其做了回退；NSIS 默认装 `%LOCALAPPDATA%\Programs`（可写），游戏目录照旧在 exe 旁。另：卸载后 `HKCU\Software\<manufacturer>\<product>` 会保留（Tauri 用它记住上次安装位置，重装时恢复），仅在勾选「删除应用数据」时才清除——这是有意行为，不是残留 bug
- **发布工作流（`.github/workflows/release.yml`；PR / 主分支推送的验证门在 `.github/workflows/ci.yml`）**：**只在推 `v*` tag 时跑**（手动 dispatch 只出 Actions artifacts；`workflow_dispatch` 需 workflow 在默认分支 `main` 上才可点）。验证门复用 `.github/workflows/check.yml`（`frontend` 平台无关检查只跑一次：vue-tsc + lint:css + i18n + fmt；`rust-windows` clippy + test 串行；`rust-cross` Linux/macOS clippy，覆盖平台 cfg 分支）、`prepare`（`needs: check`，唯一创建草稿 Release）、`build`（`needs: prepare`，三平台并行打包）、`publish`（`needs: [prepare, build]`）——**任一环节失败 Release 就停在草稿**，不会对外露出半成品。产物：Windows 便携 zip + NSIS + MSI、macOS **universal** dmg（`APPLE_SIGNING_IDENTITY=-` 做 ad-hoc 签名：Apple Silicon 上未签名的二进制根本无法启动）、Linux AppImage + deb + rpm + 便携 tar.gz。构建期密钥走 GitHub Secrets（仅 `ELYTRA_CURSEFORGE_API_KEY`；MS_CLIENT_ID 已内置公开常量，`src-tauri/build.rs` 读仓库根 `.env` 逐行注入 `rustc-env`），**未配置则 workflow 直接报错退出**——`.env` 被 gitignore，本地缺 `.env` 时 CurseForge 源静默降级（模板见 `.env.example`）。workflow 还会校验三处版本号一致、且与 tag 相符。Linux 固定跑 `ubuntu-22.04`（AppImage/deb 的 glibc 下限跟构建机走，用 latest 会在旧发行版上起不来）。**同版本重跑**：`git tag -f v1.0.0 <新提交> && git push -f origin v1.0.0`（版本校验要求 tag 号与 `tauri.conf.json` 一致，不能换个新号重跑）。`ci.yml` 与 release 的 check 共用 `check.yml`；`tauri.conf.json` 的 `beforeBuildCommand` 只跑 `pnpm build:frontend`，类型/CSS 检查由 check 承担（本地完整构建用 `pnpm tauri:build`）
- **系统代理**：Windows 读注册表（`winreg`，仅 windows 依赖）→ env；Linux env → GNOME gsettings；reqwest 默认不读系统代理，开启系统代理（Clash 等）时官方源直连会被劫持/黑洞
- **EasyTier 构建依赖**：easytier 是常规依赖，`cargo check/test/build` 都会编译它；`kcp` feature 依赖 kcp-sys（bindgen 需 libclang）——构建机须装 LLVM（`winget install LLVM.LLVM`）并设 `LIBCLANG_PATH`；另需 `7z` 在 PATH（Windows 7-Zip / Linux `p7zip-full` / macOS `p7zip`）；`easytier-proto` / `prost-wkt-types` 的 build.rs 会调 `protoc`，PATH 上没有就 panic（`Could not find protoc`）——`mise.toml` 已把 `protoc` 列为工具，本地 `mise install` 即装，CI 同源（勿再往 workflow 里塞单独的 protoc 安装步骤）
- **种子预览构建依赖**：`src-tauri/vendor/cubiomes` 由 `build.rs` 编译；Windows MSVC 目标用 **clang-cl**（上游 xpple fork 使用 VLA，`cl.exe` 不兼容），LLVM 与 kcp-sys 的 libclang 同源；`build.rs` 会从 `LIBCLANG_PATH` / `C:\Program Files\LLVM\bin` / PATH 自动找 `clang-cl.exe`，找不到时构建失败并提示。macOS/Linux 用系统 cc。
- 敏感信息（token/client secret）不硬编码、不打印日志、不提交 git
- 全局事件拦截：App.vue 禁 F12/DevTools/全选/刷新；**Tab 不吞**——转发给 `utils/focusNav.ts` 的应用自管焦点遍历（文档序，弹窗打开时范围收窄到弹窗）；ESC 无弹窗时回启动页

## Testing & QA

- **无前端测试**；Rust 侧 439 个单元测试（431 passed + 8 ignored），全部内嵌各模块 `#[cfg(test)] mod tests`（含测试与实现同文件的约定），`cargo test --lib -- --test-threads=1` 运行；集成测试 `tests/online_diag.rs`（联机全链路：host/guest 双实例回环 + easytier 日志落盘）与 `tests/tool_download.rs`（工具箱下载器全链路：本地 HTTP 服务驱动入队 / 分段 / 暂停续传 / 取消 / 回退，5 用例，含 1 个真实网络 `#[ignore]`）
- **发布前 GTK 冒烟**：CI 只构建不渲染，发版前在 WSLg / Linux VM 跑 `pnpm tauri dev` 过 10 个页面（重点：滚动条宽度、徽章底色、弹窗遮罩、皮肤工具画布），确认无布局错位
- **单元测试只测纯函数**：PKCE、URL 组装、启动命令构建（`build_launch_command` + `minimal_version_json()` 助手）、离线皮肤 UUID 算法（与 Java `UUID.hashCode() & 1` 一致性）、内存分配公式、util（URL 文件名/版本目录解析/`sha1_sub_dir`/`safe_relative_path`）、maven XML 解析与语义化排序、Mod 双源搜索、修复步骤规划、persist 读写回环、secrets DPAPI、store 快照/恢复、**`store_ops_from_runtime_worker_no_panic`（worker 线程 block_on 回归）**、**`count_assets_all_end_to_end`（真实索引 + 5057 项多线程扫描，网络）**、**`find_java_home_finds_nested_bin`（Azul 布局幂等）**、`downloader/fetcher` 的取消/换源/校验/重定向/并发上限、`downloader/segmented` 的切分覆盖/小文件不分段/分段写一致性/不支持 Range 回退/段级与段内续传/陈旧位图作废、loader 的 profile 合并与继承展开、房间码解析/广播回环（online）、**`task/emit` 的派发器（进度帧按 `(事件, 任务)` 合并 / 低频事件 FIFO / 同任务帧严格保序 / 等待决策帧不被进度帧顶替）**、`task/progress` 的 `StepRef::percent`（文件口径优先 / 无分母或步骤结束返回 `None`）与整合包 `Counters` 的进入/离开步骤、**步骤索引↔名字对齐**（整合包 `step_index_points_at_matching_names`：Online/Local 两种前缀、`replace_steps_keeps_done_steps_at_100`：已完成步骤不得被重置；修复 `test_repair_indices_align_with_steps`：穷举 16 种缺失组合）
- **断言风格**：`assert_eq!` 精确值为主（argv 位置、公式值、错误码消息）；行为不变量用 `assert!` + 中文 message
- **平台隔离**：Windows 专属逻辑（BAT 脚本渲染、DPAPI、注册表代理）用 `#[cfg(target_os = "windows")]`
- **persist 测试隔离**：测试进程下配置落在 `target/debug/deps/Elytra/Settings.toml`（config_dir 基于 current_exe 推导，测试进程在 deps/ 下），与正式运行隔离；每个测试先 remove_file 再 save（串行依赖）
- **安全命令**：`cargo test --lib -- --test-threads=1`——日常验证用这个；**并行跑会偶发竞争**（persist 共用 Settings.toml + 网络测试抖动），必须串行
- **clippy 红线**：`cargo clippy --lib` 保持零代码警告（`too_many_arguments` 已通过上下文结构体全部消除；仅余 `type_complexity` ×2 与 `upper_case_acronyms` ×3（Win32 类型），均为显式 allow）
- **端到端测试**：`tests/e2e_download.rs`（`#[ignore]`，需真实网络）驱动 `run_task`（下载主流程）——任务层的事件出口已抽象为 `task::emit::EventSink`（`impl<R: Runtime> EventSink for AppHandle<R>`），测试注入记录型实现即可跑完整流程且不触碰 IPC。两个用例分别覆盖「阶段 A/B 真实执行 + 阶段 C 取消收口」与「真实完整下载 Minecraft 1.20.1」；另有第三个用例 `spawn_task_with_sink_registers_and_runs`（**不需要网络**）从 `spawn_task_with_sink` 入口进入，覆盖「注册 → 写恢复快照 → 执行闭包」，补上直接调 `run_task` 绕过入口的空缺。**注意**：须用 `#[tokio::test(flavor = "multi_thread")]` —— 终态收口经 `store::update_status` 走 `util::block_on` 的 `block_in_place` 分支，current_thread runtime 下会 panic（与生产 tauri runtime 一致）。
- **ModLoader 安装矩阵**（`game/loader/mod.rs` 的 `#[cfg(test)] mod matrix`，2 个 `#[ignore]` 用例）：直接调安装入口（`install_loader` / `install_optifine`，均不依赖 `AppHandle`），真实网络 + 真实 JDK 跑「形态」而非「版本」——loader 侧 9 条（Fabric / Quilt / Forge 1.7.10·1.8.9 spec-0 / Forge 1.12.2 legacy-maven / Forge 1.16.5 processors+maven / Forge 1.20.1 / NeoForge 20.4.237 / NeoForge 1.21.1）+ OptiFine 侧 5 条（1.20.1·1.12.2 的纯净版 / Forge 组合 / Fabric 组合，后者含 CurseForge 取 OptiFabric 桥接）。每个用例除产物清单外，还把安装出的版本 JSON **按真实模型解析并构造启动命令**（检查 classpath 项数、`${}` 占位符残留、loader 本体是否在 `-cp`/`--module-path`、natives 标志）——`arguments: null`、loader 不进 classpath 这两类问题就是这一步抓到的。结果同时落 `target/e2e-loader/<用例>.json` 与 `report-loader.md` / `report-optifine.md`（可直接当测试记录）。

- **工具箱下载器测试**：`tests/tool_download.rs` 走本地 HTTP 服务（可控 206/200 + 按块节流），验证入队调度、分段写一致性、暂停保留位图、续传完成、取消清理、不支持 Range 回退。`tooldl` 域同样只依赖 `task::EventSink` 而非 `AppHandle`（`enqueue`/`resume` 的第一个参数是 `Arc<dyn EventSink>`），故无需 mock app 即可驱动。用例共享进程级全局（注册表 / 并发上限），文件内有 `TEST_LOCK` 串行闸，默认可与并行测试框架共存。

## Known Debt（已知遗留，待处理）

- **Fabric/Quilt + OptiFine 明确不推荐**：该组合依赖 OptiFabric 桥接（CurseForge 渠道，slug `optifabric`），上游维护缓慢、兼容性有限；UI 标注“不推荐”并建议 Sodium + Iris 或原版/Forge + OptiFine，1.21+ 组合隐藏并明确报错。
- **跨网联机依赖协调节点**：默认内置 `https://etnode.zkitefly.eu.org/node{1,2}`（社区节点，明确禁中转；官方 `node.invalid` / `node.invalid` 已下线）；默认 `allow_relay=false`（`p2p_only`，数据面只走已建立的直连，节点仅承担发现/握手/打洞信令），打洞失败需用户在「设置 → 联机设置」临时允许中继；MC 26.2 离线账号只能走“局域网发现”路径进入，手动 IP 会报无效会话（广播模拟已绕过）。
- **`src-tauri/src/data/wiki_entries.txt` 维护**：PCL `WikiEntries.txt` 的未修改快照，行号 = MC 百科条目 id；更新文件必须同步 `THIRD-PARTY-NOTICES.md` 的 commit/版本，并重跑 `cargo test --lib wiki_entry`。
- **地图调色数据**：`src-tauri/assets/natural_block_colors.json` 与草/树叶色图由 Minecraft 客户端素材派生；仅在本地用于地图着色，公开分发前需确认许可并在 `THIRD-PARTY-NOTICES.md` 保留来源说明。
- **种子预览精度/版本边界**：cubiomes 仅覆盖正式版参数族（Beta 1.7 → 26.3）；补丁版本映射到最近参数族并在 UI 标注，快照/基岩版禁用。群系已用官方 26.3 服务端锚点校验；结构是候选（1.18+ 地形依赖结构仍可能误报）。升级 vendored cubiomes 或参数族后必须重跑 `seedmap::tile` 锚点测试与官方服务端抽样对拍。
- **vendored cubiomes**：`src-tauri/vendor/cubiomes` 为 xpple fork `d934de1`（MIT），只含核心/feature helper、不含 loot；升级时同步 `PROVENANCE.md` 与 `THIRD-PARTY-NOTICES.md`。Windows MSVC 构建依赖 clang-cl，见构建依赖条目。
