# Elytra 代码问题记录

- 首次记录：2026-10-07
- 范围：`src-tauri/src/`（104 文件 / ~28k 行）、`src/`（111 文件 / ~23k 行）
- 方法：`cargo clippy --lib`、`npx vue-tsc --noEmit`、行号级静态扫描 + 两路只读审计（Rust / 前端）
- **证据等级标注**：`[复验]` = 本次亲自读代码/跑命令确认；`[审计]` = 子代理行号级报告，抽查但未逐条复验
- 状态列：`待定` = 未决定是否修；`已修` = 已完成并验证

> 本文件是问题台账，追加请保持「文件:行 + 现状 + 影响 + 修法」四要素。

---

## 0. 阻塞项：当前工作区编译不通过 `[复验]`

`cargo clippy --lib` 报 4 个 error，lib 无法编译。起因是**半成品重构未收尾**（工作区 53 文件未提交、588+/911-，含未跟踪的 `src-tauri/src/config/downloader.rs`）。

| # | 位置 | 现状 | 修法 | 状态 |
|---|---|---|---|---|
| 0.1 | `src-tauri/src/task/mod.rs:27-29` | 新增 `pub use crate::{events, jdk, persist};`，但 `lib.rs:6,7,10` 里这三个是私有 `mod` → `error[E0365] … only public within the crate` | 删除三行 re-export，或把 `lib.rs` 对应 `mod` 提升为 `pub mod`（取决于门面是否要给外部用） | 待定 |
| 0.2 | `src-tauri/src/task/mod.rs:146` | 仍调 `runner::run_install_jdk`，函数已搬到 `task/jdk.rs:23` → `error[E0425]` | 改 `jdk::run_install_jdk`，并更新 `mod.rs:131` 那句“在 runner.rs”的注释 | 待定 |
| 0.3 | `src-tauri/src/config/downloader.rs` | 未跟踪新文件，被 `task/mod.rs:25` re-export `DownloaderConfig` | 决定去留后 `git add`（若保留） | 待定 |

前端 `npx vue-tsc --noEmit` → `EXIT=0`，本次**无**类型错误。

---

## 1. 真 bug（非风格问题，建议优先）

### 1.1 锁序反转：ABBA 死锁窗口 `[复验]`

违反 AGENTS「统一 inner→progress 锁序」。

- `task/jdk.rs:46-48`、`task/runner.rs:131-133`：
  ```rust
  if let Some(p) = progress.try_lock() {
      task::global().set_cleanup_path(&p.task_id, dest_dir.clone()); // 内部取 inner.lock()
  }
  ```
  守卫 `p` 活到 if-let 块末，块内 `set_cleanup_path`（`task/mod.rs:253-256`）取 `inner` → **progress→inner**。
  反向路径（持 inner 后阻塞取 progress）：`task/mod.rs:320-336 fail()`、`337-346 list()`、`352-369 remove()`、`264-286 pause()`。
  两线程反向交叉即死锁。
- 正确写法已有两处范例：`task/progress.rs:432-441`（先 drop 再 `take_cleanup_path`）、`task/modpack.rs:461-463`（临时守卫语句末释放）。
- 修法：`let id = { let p = progress.try_lock()?; p.task_id.clone() }; set_cleanup_path(&id, …);`
- 状态：待定

### 1.2 未定义 CSS 变量 `--text-danger`（12 处）`[复验]`

全仓只有引用、无定义；`App.vue:278` 定义的是 `--color-danger: #E5484D`。现全部靠 fallback 硬编码色兜底 → 主题切换时危险色不跟随，且兜底色两套（`#e5484d` 与 `#ff5555`）。

使用点：`component/tools/{FireworkGeneratorTool:345, GiveGeneratorTool:630, ModsEditor:213, OnlineTool:343,360,361, PotionGeneratorTool:366, ServerStatusTool:223,224,225, VillagerGeneratorTool:386,420}`。

修法：统一替换为 `var(--color-danger)`。状态：待定

### 1.3 RNG 失败被吞 → 房间码/网络凭据可预测 `[复验]`

`online/scaffold/room.rs:29`：`let _ = getrandom(&mut bytes);`，失败时 16 字节全零，`u128::from_be_bytes` 得到确定值 → 房间码与 `network_secret` 可预测。`create_room()` 需改返回 `Result` 并上抛。

状态：待定

### 1.4 网络输入上的字节切片（可 panic）`[审计]`

`downloader/assets.rs:117`、`launcher/env.rs:223`、`task/repair.rs:506` 的 `hash[..2]` / `&obj.hash[..2]`：hash 来自远程索引或本地 JSON，长度 <2 或落在多字节边界即 panic。修法：统一 `hash.get(..2).unwrap_or("00")`。

状态：待定

### 1.5 非测试代码 unwrap/expect/panic 清单 `[审计]`（抽查一致）

| 位置 | 现状 |
|---|---|
| `downloader/fetcher/core.rs:154` | `Err(last_err.unwrap())` |
| `downloader/fetcher/redirect.rs:47,59` | `builder.build().expect("reqwest::Client")` / `guard.clone().expect("http client")` |
| `downloader/assets.rs:143`、`library.rs:207` | `permit.acquire_owned().await.unwrap()` |
| `downloader/assets.rs:237-238` | `panic!("asset index 获取失败/超时")` |
| `downloader/mod.rs:40`、`util.rs:288` | `unreachable!()` |
| `task/mod.rs:53,78` | `expect("spawn task-progress-emit thread")` / `expect("TaskManager not initialized")` |
| `task/modpack.rs:509` | `expect("阶段顺序不变量：B1 先于 B2–B5")` |
| `persist/secrets.rs:129,131,153,155` | `Key::try_from(...).unwrap()` / `Nonce::try_from(...).unwrap()`（加密热路径） |
| `launcher/env.rs:324` | `candidates.last().unwrap()` |
| `online/scaffold/identity.rs:27` | `path.parent().expect(…)` |
| `lib.rs:200` | Tauri 顶层 `expect`（允许） |

状态：待定

### 1.6 静默失败（`let _ =` 吞错）`[审计]`

| 位置 | 影响 |
|---|---|
| `task/modpack.rs:454-458` | 包身份标记 `elytra-pack.json` 写失败 → 复用检测静默失效 |
| `jdk/mod.rs:32,47` | JDK 扫描结果落盘失败无提示 |
| `commands/account.rs:246-248` | `ensure_jar()` 失败仅 `eprintln!` → 登录显示成功、启动时才报缺 jar |
| `commands/gamedirs.rs:12` | 默认目录写盘失败静默，下次启动重复生成 |
| `persist/secrets.rs:79` | `set_permissions(0o600)` 失败被吞（安全相关） |
| `task/mod.rs:97` | `store::save_snapshot` / `update_status` 完全静默 |

状态：待定

### 1.7 前端非空断言（可白屏）`[审计]`

`component/download/VanillaDetail.vue:18`（`vdState.selectedVersion!`，模板 27/36/37/44/51/55 反复经 `sv()` 调用）、`VanillaList.vue:23,24`（manifest 无该 id 时 null）、`store/avatar.ts:253,278,347,402,429`（`getContext("2d")!`）。

状态：待定

### 1.8 内联/toast 错误未走 `errMsg()` `[审计]`

`ServerStatusTool.vue:55`、`SettingsThirdParty.vue:53`、`utils/useConfigSection.ts:13`、`store/online.ts:138,152,166`（内联展示完整多行技术细节）。

合规反例（无需改）：`accountUi.ts`/`auth.ts`/`launch.ts` 走 AlertDialog 全文；`SettingsLog.vue:16`、`VersionSetupLogs.vue:30` 属日志原文。

状态：待定

---

## 2. 系统性规范偏差

### 2.1 裸 `color-mix()` 未 `@supports` 包裹 `[复验]`

AGENTS 明令「新 CSS 特性一律 `@supports` 包裹或写兼容写法，**禁止裸用**」。现存 20+ 处裸用，低版本 WebKitGTK 下整条声明失效。

| 文件 | 行 |
|---|---|
| `component/common/TitleBar.vue` | 152,159,187,192,225,233,238（窗口 chrome 主体，7 处） |
| `component/download/CommunitySearchBar.vue` | 162（border 整条失效） |
| `component/tools/OnlineTool.vue` | 361,380 |
| `component/tools/ServerStatusTool.vue` | 224,225 |
| `component/version-setup/VersionSetupMods.vue` | 643,649 |
| `component/version-setup/VersionSetupResource.vue` | 465,471 |
| `component/version-setup/VersionSetupSaves.vue` | 364,370 |

合规范例（照抄这个写法）：`ResourceDetail.vue:1131-1132`（先声明兼容值再声明 color-mix，不支持时前一条生效）、`AlertDialog.vue:105`（`@supports not (backdrop-filter…)`）。

状态：待定

### 2.2 绕过 store 直接 `invoke()`（13 文件）`[审计]`

AGENTS 约定「页面组件直接 import store」；命令封装应下沉 store。

`component/download/ModpackSearchBar.vue:25`、`component/launch/AccountSection.vue:76`、`component/launch/VersionDetailCard.vue:199,203,216,228,276`、`component/settings/SettingsLog.vue:13,26`、`component/settings/SettingsThirdParty.vue:68,80`、`component/tools/ServerStatusTool.vue:49`、`component/version-setup/VersionSetupExport.vue:54,58,83`、`VersionSetupLogs.vue:24,42,44`、`VersionSetupMods.vue:138,142`、`VersionSetupResource.vue:80,109,113`、`VersionSetupSaves.vue:53,85,89`、`VersionSetupServer.vue:19,32`、`pages/ResourceDetail.vue:346,399`。

其中 `SettingsThirdParty` / `ServerStatusTool` / `VersionSetup*` / `SettingsLog` **整块功能没有 store**；`open_folder`/`ensure_game_folder` 在 5 个组件里各调一次。
另：`VersionSetupExport.vue:54` 的 `get_last_offline_username` 已有封装 `auth.ts:190 loadLastUsername`。

状态：待定

### 2.3 重复实现（Rust）`[审计]`

| 项 | 位置 | 说明 |
|---|---|---|
| 并发下载循环 | `downloader/assets.rs:119-208` vs `library.rs:184-243` | permit → wait_path_lock → ProgressCounters → 本地复用/网络降级，逐行相同（含注释） |
| 速度探测双套 | `downloader/probe.rs` vs `jdk/download.rs:190-215` | 各自 TTL 缓存（JDK 侧 `SPEED_CACHE_TTL_SECS=600`） |
| 路径穿越守卫 | `downloader/modpack.rs:286 safe_rel_path` vs `jdk/download.rs:460 safe_runtime_path` | 逻辑同构 |
| 终态收口两套 | `task/modpack.rs:133/154/169` vs `task/progress.rs:390/421/436` | modpack 自建 Done/Cancelled/Error 与步骤重写 |

状态：待定

### 2.4 重复实现（前端）`[审计]`

| 项 | 位置 | 说明 |
|---|---|---|
| `formatOptions` 三份字面量 | `PotionGeneratorTool.vue:18-21`、`FireworkGeneratorTool.vue:17-20`、`VillagerGeneratorTool.vue:20-23` | `utils/give.ts:405 GIVE_FORMAT_OPTIONS` 已存在，`GiveGeneratorTool.vue:20-23` 是正确范例 |
| `copyCommand()` 六份 | `Potion:105-111`、`Firework:88-96`、`Give:151-158`、`Villager:138-145`、`MotdGeneratorTool:80-94`（两份）、`OnlineTool:60-68` | 文案也逐字重复 |
| 生成器输出样式五份 | `Give:650-659`、`Firework:372-381`、`Potion:424-433`、`Villager:439-448`、`MotdPreview.vue:39-48` | 硬编码 `rgba(0,0,0,0.35)` + Cascadia Mono |
| `.nb-tag-x` hover 五份 | `Firework:344-347`、`Give:629-632`、`ModsEditor:212-215`、`Potion:365-368`、`Villager:419-422` | 同上 |
| `.expand-*` 过渡 CSS 五份 | `SettingsDownload.vue:264-277`(1000px)、`SettingsThirdParty.vue:271-286`(1000px)、`AccountSection.vue:872-889`(180px)、`ResourceDetail.vue:969-984`(`var(--pop-h)`)、`VanillaDetail.vue:192-197`(320px) | `styles/transitions.css` 已存在却未承载 expand |
| `VersionSetup*` 动作栏 | `VersionSetupMods.vue:600-651`、`Resource.vue:440-473`、`Saves.vue:340-372` | 选择器仅前缀不同 |
| 常量表双份 | `versionDownload.ts:55-58 loaderMinVersion` vs `utils/versionMeta.ts LOADER_KEYS`；`CustomThemeEditor.vue:56-67 PARTS` vs `theme.ts:24-33 CustomThemeVars` | 新增项需两处同步 |
| 主题变量双事实源 | `App.vue:316-430`（4 套 `[data-theme]`） vs `store/theme.ts:47-96 builtinVars()` | 改一处色值需同步两处 |

状态：待定

### 2.5 expand 动画回退写法 `[审计]`

`component/download/VersionCard.vue:13-16,31,53-56`：用 `Math.min(count*100+24, 99999)+'px'` 内联 style + 手写 transition，未用 `<Transition name="expand">`。除违反约定外，行高按 100px/行估算 → **行高不同即裁切**。

状态：待定

### 2.6 命令层承载业务逻辑（分层违规）`[审计]`

| 位置 | 现状 | 应归位 |
|---|---|---|
| `commands/launch.rs:45-165 spawn_game`（~120 行） | pre_launch_cmd、内存优化、GPU 注册表、`std::process::Command` 构造、stdout/stderr 排空线程、`watch_game_process` 接线 | `launcher/process.rs::spawn_and_watch` |
| `commands/gamedirs.rs:142-169 auto_activate_version` | 目录遍历 + 过滤 + 排序 + 取首 + 调 `set_active_game_dir`（命令调命令） | `downloader/version.rs` |
| `commands/gamedirs.rs:9-26 read_dirs` | getter 有写盘副作用 | 显式 `ensure_initialized()` |
| `commands/offline_account.rs:7-48` | 5 个命令各自内联列表 read/retain/upsert/save | `auth/` 或 `persist` |
| `commands/version.rs:117-135 delete_resource_file` | kind 白名单 + 路径拼装 + 同步 `remove_file` | 与 async `delete_world`/`delete_version_dir` 风格统一 |
| `commands/version_icon.rs:20-31` | custom > pack 优先级业务规则 | domain |
| `commands/account.rs:296-308 get_third_party_config` | 空列表注入内置预设 | `external::ThirdPartyConfig::load()` |
| `commands/launch.rs:168-186` | async fn 内同步 `std::fs` 读（阻塞 tokio worker）；`184` 吞 IO 错误返回空串 | `spawn_blocking` |
| `commands/launch.rs:176 open_launcher_log_dir` | 直接调 `super::gamedirs::open_folder`（命令间耦合） | 领域函数 |

状态：待定

### 2.7 `#[allow(clippy::too_many_arguments)]` 17 处 `[复验]`（清单 `[审计]`）

`downloader/assets.rs:83`、`downloader/modpack.rs:368`、`mod_loader/fabric.rs:14,94,120`、`mod_loader/mod.rs:240`、`mod_loader/neoforge.rs:12,93`、`mod_loader/optifine.rs:131`、`launcher/args.rs:164,276,710`、`task/files.rs:50,129`、`task/modpack.rs:986`、`task/repair.rs:474`、`task/runner.rs:619`。

根因：缺 `TaskCtx { app, cancel, progress, task_id }` 上下文结构体（`task/files.rs:50` 的注释「对齐 search_community 先例」实为自我辩护）。
另有 `type_complexity`（`launcher/env.rs:31` 五元组、`launcher/process.rs:290`）、`upper_case_acronyms`（`launcher/memory.rs:64,66,69`，Win32 类型，合理）、`dead_code`（`downloader/proxy.rs:15,106`，平台条件，可接受）。

状态：待定

### 2.8 超长函数 `[审计]`

| 位置 | 行数 |
|---|---|
| `task/runner.rs:30-541 run_task` | ~511 |
| `task/repair.rs:164-394 run_repair_task` | ~230 |
| `task/files.rs:263-452 run_files_task` | ~189 |
| `launcher/args.rs:277-458 build_launch_command` | ~181 |
| `downloader/mod_loader/forge.rs:59-229 download_and_extract` | ~170 |
| `task/modpack.rs:213-382 run_install` | ~169 |
| `launcher/args.rs:711-836 collect_jvm_args` | ~125 |
| `commands/launch.rs:45-165 spawn_game` | ~120 |
| `task/repair.rs:475-603 count_assets_all` | ~128 |

状态：待定

### 2.9 超大文件 `[复验]`

Rust：`task/modpack.rs` 1267 行、`jdk/download.rs` 1096、`downloader/modpack.rs` 1037、`launcher/args.rs` 904、`online/mod.rs` 838、`task/repair.rs` 766、`task/runner.rs` 760。

前端（>600 行）：`pages/ResourceDetail.vue` 1209、`component/launch/AccountSection.vue` 1084、`pages/CustomThemeEditor.vue` 899、`component/launch/VersionDetailCard.vue` 887、`component/version-setup/VersionSetupMods.vue` 690、`component/tools/GiveGeneratorTool.vue` 667。

状态：待定

### 2.10 其他一致性缺陷 `[审计]`

| 项 | 位置 | 说明 |
|---|---|---|
| `update_and_emit` 自旋 | `task/mod.rs:455-470` | async 上下文里 `std::thread::sleep(5ms)` 自旋 `try_lock`，占死 tokio worker |
| 动态 section 无校验 | `commands/version.rs:216,231` | 用户可控版本名成为 TOML 顶层 key（`version_server_{name}`），无界增长且未做 `is_simple_name` 校验，偏离 `SECT_*` 常量约定 |
| 死命令 | `commands/version.rs:157 list_mods_detailed` | 已注册但前端无调用 |
| 死变体 | `persist/mod.rs:13 PersistError::NotFound` | 全仓无构造点（未逐点复验） |
| unsafe 无 SAFETY | `util.rs:576`、`launcher/memory.rs:63,139`、`launcher/platform.rs:90,135,174,195`、`launcher/process.rs:147`、`persist/secrets.rs:395-446` | 全部缺 `// SAFETY:` 说明 |
| 测试内无谓 unsafe | `online/scaffold/room.rs:180` | `as_bytes_mut()` 改写 String，可用重建字符串消除 |
| 等宽字体栈三套 | tools 系 / 日志系（`SettingsLog.vue:93`、`VersionSetupLogs.vue:133`）/ `CustomThemeEditor.vue:883,895` | 缺 `--font-mono` token |
| 主字体栈偏离规定 | `App.vue:235` | 以 `Inter, "Segoe UI", "Microsoft YaHei UI"` 起头；Inter 未 `@font-face` 引入（不生效）；AGENTS 规定链未遵守 |
| events 注释漂移 | `events.rs:9` vs `store/events.ts:13`、`launch.ts:220` | GAME_EXITED 实际是 4 元组（pid/label/exitCode/crashReport）；ONLINE_STATE 注释缺 `players` |
| doc 误抄 | `launcher/args.rs:576` | `push_unique` 的注释照抄 `has_old_style_natives` |
| 模块划分 | `server_status.rs` 在 crate 根 | 同类域均用目录（`downloader/`、`jdk/`、`online/`） |
| 聚合 total 语义 | `communitySearch.ts:169,249-252` | `Math.max(mr.total, cf.total)` 与分页换算口径不一致 |
| 隐式 camelCase | `downloader/community/mod.rs:112 SearchQuery` 用 `#[serde(rename_all="camelCase")]` | 与 AGENTS「字段全 snake_case」相反（两侧一致，非 bug，但无注释） |
| `as any` | `store/launchOptions.ts:116` | 全仓唯一 1 处 |
| 缓存无上限 | `versionDownload.ts:94`、`versionIcon.ts:8,33`、`versionDetail.ts:141` | 模块级合规，但从不清理 |

状态：待定

---

## 3. 文档漂移：`AGENTS.md` 与实际不符 `[复验]`

| 项 | 文档说 | 实际 |
|---|---|---|
| 页面数 | 7 个 | 9 个（另有 `Tools.vue`、`CustomThemeEditor.vue`） |
| store 数 | 20 个 | 24 个（另有 `nav.ts`、`downloadTab.ts`、`settingsTab.ts`、`versionIcon.ts`） |
| config 结构 | `config/launcher/`、`config/downloader/{model,source}.rs` | 扁平 4 文件 `config/{downloader,game_dirs,online,mod}.rs`；`launcher/` 在顶层；`source.rs` 在 `downloader/` |
| commands 模块 | 13 个 | 15 个 |
| `run_install_jdk` 位置 | `task/runner.rs` | `task/jdk.rs:23` |
| 未记录 | — | `online/scaffold/`（6 文件）、`auth/external.rs`、`downloader/mod_updates/`、`downloader/fetcher/{core,routes,tests}.rs`、`task/jdk.rs`、`src/component/tools/`（5 生成器 + `ModsEditor`）、`SettingsThirdParty.vue`、`utils/{nbt,give,toast,tooltip,useSelectable,useConfirmDelete}.ts` |
| 测试数 | 232 | 未重新统计 |

状态：待定

---

## 4. 合规确认（审计专门核查、**未**发现违规的项，避免误伤）

- 锁类型全 `parking_lot`，无 `std::sync::Mutex/RwLock`；无跨 `.await` 持锁（仅测试锁 `curseforge.rs:330,351`）；无锁内 IO/DB 写。
- `generate_handler!` 注册无遗漏（15 个命令模块全部注册）。
- 模块边界统一 `Result<T, String>`；内部自定义错误（`PersistError`/`MsError`/`FetchError`）均在边界 `to_string` 展平。
- 跨组件缓存**全部**在 store 模块级；未发现「缓存写在 `<script setup>` 顶层」。
- 事件监听幂等标记齐备（`download.ts:245`、`online.ts:72`、`launch.ts:215`、`auth.ts:235`、`memory.ts:13`、`versionDetail.ts:113`）；组件监听器均成对清理。
- TS interface 与 Rust struct 抽查字段一致（`InterruptedJob`/`ServerStatus`/`FabricApiVer`/`OnlineSnapshot`/`events.ts`）；无 `@ts-ignore`/`@ts-expect-error`；无 `v-html`。
- 未使用导出：未发现。
- 正面范例：`pages/Tools.vue` 双 script 块模块级常量、`GiveGeneratorTool.vue:20-23` 复用 `GIVE_FORMAT_OPTIONS`、`ResourceDetail.vue:757` 的 `@supports` 降级、`task/progress.rs:432-441` 的锁序正确写法。

---

## 5. 用户反馈：Rust 后端规范（2026-10-07，共 6 条）

> 以下「基线」数字由脚本全量扫描所得，可复现；治理后应归零。

### 5.1 禁止方法体内深层全路径 `crate::a::b::c`，禁止方法体内 `use`，测试 mod 统一在 mod 顶部声明 use

**基线**：

| 项 | 数量 |
|---|---|
| 三段式 `crate::a::b::c`（生产代码） | **152 处**，最集中 `task/modpack.rs` 20、`launcher/process.rs` 16、`downloader/modpack.rs` 9、`launcher/env.rs` 8、`launcher/args.rs` 7、`online/mod.rs` 6、`task/steps.rs` 6 |
| 函数体内 `use`（生产代码） | **55 处** |
| 函数体内 `use`（测试代码，应上移到测试 mod 顶部） | **75 处** |

**函数体内 `use` 生产代码全量清单**（`文件:行 → 语句`）：

```
util.rs:22,484,574                        │ tokio::runtime::RuntimeFlavor / base64::Engine / windows::io::AsRawHandle
auth/external.rs:395,483                  │ base64::Engine / sha2::Digest
auth/mojang.rs:41                         │ base64::Engine
commands/account.rs:43,97,169             │ tauri_plugin_opener::OpenerExt / base64::Engine ×2
commands/gamedirs.rs:126                  │ tauri_plugin_opener::OpenerExt
downloader/extract.rs:223                 │ crate::jdk::download::safe_runtime_path
downloader/hash.rs:5,11,35,40,59          │ sha2::Digest / sha1::Digest ×3 / io::Read
downloader/modpack.rs:242,622             │ collections::HashMap / io::Write
downloader/proxy.rs:55,56                 │ winreg::enums::HKEY_CURRENT_USER / winreg::RegKey
downloader/reuse.rs:93                    │ io::{Read, Write}
downloader/fetcher/core.rs:357            │ futures_util::StreamExt
downloader/mod_loader/mod.rs:189,190      │ sync::atomic::Ordering / tokio::io::AsyncReadExt
downloader/mod_loader/version_query.rs:85 │ cmp::Ordering
jdk/download.rs:958,959                   │ sha2::Digest / io::Read
launcher/platform.rs:153                  │ crate::launcher::model::ProcessPriority as P
launcher/process.rs:20,45,75,117,160,246,277 │ windows::process::CommandExt ×2 / io::Write / ffi::c_void / time::SystemTime / model::AuthMode ×2
launcher/skin.rs:89                       │ base64::Engine
online/session.rs:42                      │ easytier::common::config::{ConsoleLoggerConfig, …}
online/scaffold/codec.rs:127,159,180,199  │ tokio::io::{AsyncReadExt, AsyncWriteExt} ×2 组
persist/secrets.rs:78,123,142,164,165,167,169,172 │ unix::fs::PermissionsExt / chacha20poly1305 ×2 / HashMap / LazyLock / secret_service / constants / util::block_on
task/modpack.rs:693                       │ crate::downloader::mod_loader::{self, LoaderEvent}
task/progress.rs:506                      │ time::Duration
task/repair.rs:66,485                     │ crate::downloader::extract::legacy_classifier_artifact / file_sha1_matches
task/store.rs:85,119                      │ sqlx::sqlite::SqliteConnectOptions / time::{SystemTime, UNIX_EPOCH}
```

**另**：`downloader/fetcher/tests.rs` 有 5 处 fn 内 `use`（该文件本身是独立测试文件，见 5.5）。

状态：待定

### 5.2 `use crate` 规范：同模块子项用 `{}` 合并；方法内只暴露 `xxx::yyy`；模块顶级函数直接 use；仅重名时才允许 `xxx::yyy::zzz`

对应 5.1 的 152 处深层路径与 55 处函数内 use，逐处改为顶部合并 import + 重名时 `as` 别名。
典型需合并点：`task/modpack.rs`（20 处）、`launcher/process.rs`（16 处，含 `model::AuthMode` 两处重复）。

状态：待定

### 5.3 `mod.rs` 定位：只允许子模块声明、公共接口重导出、模块级文档、测试 mod、条件编译声明；**任何**逻辑代码都不许留

**基线：`mod.rs` 内非声明代码合计 1600 行**（已排除 `#[cfg(test)]` 块）。

| 文件 | 违规行数 | 文件总行 | 内容性质 |
|---|---|---|---|
| `online/mod.rs` | **612** | 839 | 常量、`OnlineStatus` 枚举、`OnlineService` 单例与全部业务实现 |
| `task/mod.rs` | **346** | 605 | TaskManager 单例、`EMIT_TX` 队列线程、`update_and_emit`、`PathLock`、任务注册 |
| `downloader/mod_loader/mod.rs` | **238** | 523 | `maven_path` 等实现 |
| `downloader/mod_loader/mod.rs` | 238 | — | 同上 |
| `downloader/fetcher/mod.rs` | **164** | 222 | `DownloadResult`/`FetchError` 定义 + 实现 |
| `downloader/community/mod.rs` | **142** | 221 | `pub(crate) use` 之外还有实现 |
| `persist/mod.rs` | **46** | 120 | `PERSIST_LOCK`、`PersistError`、read/save 实现 |
| `jdk/mod.rs` | **31** | 49 | `SUPPORTED_JAVA_MAJORS` 常量 + `read()/save()` 小逻辑 |
| `downloader/mod.rs` | **17** | 42 | `resolve_version_dir()` |
| `downloader/mod_updates/mod.rs` | **4** | 25 | `QUERY_CONCURRENCY` 常量 |
| `auth/mod.rs`、`commands/mod.rs`、`config/mod.rs`、`launcher/mod.rs`、`online/scaffold/mod.rs` | **0** | — | 合规范例 |

用户补充：**「再小的逻辑也不行」**——`jdk/mod.rs` 的 `read/save`、`downloader/mod.rs` 的 `resolve_version_dir` 这类小函数反而说明拆分设计有问题，应先重新划分模块边界，而不是把小函数塞进 mod.rs。

状态：待定

### 5.4 注释过多过长，需精简

**基线**：注释行 **3234 / 28273 = 11.4%**；空行 1965；**连续 ≥8 行的注释块 25 处**。

状态：待定

### 5.5 不单独写 `tests.rs`，测试放所属文件内 `#[cfg(test)] mod tests`（或按 5.3 的 mod 规则）

**基线**：独立测试文件仅 1 个 —— `downloader/fetcher/tests.rs`（616 行），由 `downloader/fetcher/mod.rs:17` 的 `#[cfg(test)] mod tests;` 声明。

状态：待定

### 5.6 `downloader` 模块臃肿、分层不合理，是 5.1/5.2 的根因；下载器应保持纯粹，资源特化属于上层场景

**基线**：`downloader/` 共 **37 文件 / 8900 行**，其中纯下载原语与特化场景混在一起：

| 性质 | 文件（行数） | 小计 |
|---|---|---|
| 纯下载原语（应留下） | `fetcher/{core 426, mod 221, redirect 69, routes 164, tests 616}`、`hash 93`、`progress 68`、`probe 146`、`reuse 210`、`endpoints 194`、`proxy 250`、`manifest 12` | ~2470 |
| 资源特化（疑应外移） | `assets 248`、`library 282`、`extract 498`、`version 138`、`source 311`、`wiki_entry 186` | ~1660 |
| ModLoader 安装特化 | `mod_loader/{forge 651, mod 522, version_query 361, optifine 265, neoforge 205, fabric 144}` | 2148 |
| 社区资源搜索特化 | `community/{curseforge 424, modrinth 330, shared 238, translation 237, mod 220}` | 1449 |
| 整合包安装特化 | `modpack 1037` | 1037 |
| Mod 更新检测特化 | `mod_updates/{update 256, identify 252, scan 113, mod 24}` | 645 |
| 调度中枢 | `mod.rs 41` | 41 |

候选分层（待用户确认，非既成方案）：`downloader/` 只留传输层（client/路由/重试/校验/进度/复用/代理）；`resources/`（assets/library/extract/version/manifest/source）、`modloader/`、`community/`、`modpack/`、`modupdates/` 提升为与 `downloader/` 平级的域模块。

状态：待定

### 5.7 无第 7 条（用户 2026-10-07 确认）

---

## 6. 分析结论：模块边界与治理方案

### 6.1 「下载器纯粹」的可检验判据

把 5.6 的观感换成可自动校验的不变量：

- **I1**：`downloader::*` 内的文件不得 `use crate::{task, launcher, jdk, commands, auth}::*`（传输层只依赖 `config`/`constants`/`util`/`persist`/第三方 crate）。
- **I2**：依赖方向单向 —— 传输层 ← 内容装配层 ← 编排层（`task`）。
- **I3**：`downloader::*` 不出现「资源类型」名词（assets/library/mod_loader/community/modpack 等）。

**现状全部违反**（脚本得到的跨模块引用）：

| 违反 | 位置 | 证据 |
|---|---|---|
| 传输/装配层反向依赖 `task` | `downloader/assets.rs`、`library.rs`、`extract.rs` | 各 1 次 `task::wait_path_lock` |
| 内容层依赖 `launcher` | `downloader/modpack.rs`、`downloader/version.rs` | 各 1 次 `launcher::args` |
| 特化层依赖 `jdk` | `downloader/extract.rs`、`mod_loader/forge.rs` | `jdk::download::safe_runtime_path` / `jdk` |
| 特化层依赖 task 类型 | `downloader/mod_loader/mod.rs` | `task::LoaderKind` |
| 编排层反向引用 | `task/modpack.rs` → downloader | **19 次**（最重的耦合） |
| mod.rs 承载逻辑 | `downloader/mod.rs:25 resolve_version_dir` | 见 5.3 |

结论：`downloader` 现在同时是「传输层 + 版本装配层 + 内容获取层 + 安装编排层」，四层混装，所以 import 只能写全路径（5.1 的 152 处、5.3 的 1600 行都是这个结构的症状，不是独立问题）。

### 6.2 各子域的真实依赖（用于定边界）

- 纯传输（被内部引用最多，且不引用任何业务域）：`fetcher`(9) / `hash`(7) / `endpoints`(5) / `progress`(4) / `probe`(2) / `proxy`(2) / `reuse`(1)。
- 版本元数据与源解析：`model`(5)、`source`(3)、`manifest`(1)、`version` —— `model.rs` 内容是 `VersionManifest/VersionMeta/Library/Artifact/LibraryDownloads` 的 serde 模型，**不是下载器概念**。
- 内容装配：`assets`/`library`/`extract`（rules 匹配 + natives 解压 + 资源下载），三者都要 `task::wait_path_lock`。
- 纯特化（只依赖传输层 + 配置）：`mod_loader/`(2148 行)、`community/`+`wiki_entry`(1635)、`modpack`(1037)、`mod_updates/`(645)。

### 6.3 目标结构（候选，待拍板）

```text
downloader/          传输层（唯一职责：把 URL 变成校验过的字节）
  mod.rs             仅 pub mod
  client/            ← fetcher/{core,redirect,routes}.rs（传输核心）
  proxy.rs probe.rs hash.rs progress.rs endpoints.rs reuse.rs
  batch.rs           ← 新抽：assets/library 共用的并发下载循环（现两份逐行相同）

<新域>/              内容与装配层
  version/           ← model.rs + version.rs + manifest.rs + source.rs
  resource/          ← assets.rs + library.rs + extract.rs
  loader/            ← mod_loader/
  community/         ← community/ + wiki_entry.rs
  modpack/           ← modpack.rs（1037 行，按阶段拆 3 文件）
  updates/           ← mod_updates/
```

两种收口形态（见 6.5 决策点）：

- **形态 A（平铺）**：以上 6 个域各自成为顶层模块 → 顶层模块 9 → 15 个。
- **形态 B（收拢，推荐）**：6 个域挂在 `game/` 下（`game::version::…`）→ 顶层只 +1，语义是「游戏内容获取与装配」。

### 6.4 治理顺序（依赖关系决定，不可乱序）

1. **修 0 节编译阻塞** —— 否则后续每步都无法 `cargo check`。
2. **定模块边界 + 迁移文件**（6.3）—— 边界定了 import 才收敛；此步同时消解 I1 的反向依赖。
3. **抽 `batch.rs` 并合并 4 组重复实现**（2.3）—— 迁移中顺手做，避免二次搬运。
4. **清空全部 `mod.rs`**（5.3 的 1600 行）—— 业务下沉到同级实现文件。
5. **import 规范化**（5.1/5.2）：152 处三层路径 + 55 处函数内 use → 顶部合并 import。
6. **测试归位**（5.5）：`fetcher/tests.rs` 并入所属文件内 `#[cfg(test)] mod tests`。
7. **注释精简**（5.4）：25 处 ≥8 连行块先削。
8. 每步收口：`cargo check` + `cargo clippy --lib`（零警告）+ `cargo test --lib -- --test-threads=1`。

### 6.5 待拍板的决策点

1. **新域收口形态**：A 平铺 6 个顶层 / B 收拢到 `game/`（推荐）/ C 折中（只拆 `loader`、`modpack`、`community`、`updates` 四个明显特化，`version`/`resource` 留在 downloader）。
2. **`task::wait_path_lock` 归属**（I1 的核心堵点，被 `assets`/`library`/`extract` 三处引用）：
   - 移入 `downloader`（下载路径互斥本质属下载器）→ 消除反向依赖，但下载器需持有跨任务状态；
   - 或按调用注入（`&PathLock` 参数）→ 依赖干净、调用方略啰嗦。
3. **薄封装层去留**：`downloader/mod.rs::resolve_version_dir`(17 行)、`jdk/mod.rs::read/save`(31 行)、`mod_updates/mod.rs` 常量(4 行) —— 按 5.3「再小的逻辑也不行」，倾向**消除**（合并进 `util`/`persist` 调用点或迁到对应实现文件），而非搬个位置。
4. **`config/downloader.rs` 是否随迁**：它被 downloader 内部引用 15 次，是下载器专属配置模型；`downloader/source.rs:2` 的注释还指向不存在的 `config/downloader/model.rs`（文档漂移）。可保留在 `config/`（全局配置一致性），也可随 `downloader` 下沉。

另：可选门禁脚本 —— 静态拒绝「fn 体内 use」「`mod.rs` 非声明行」「`crate::` 三段路径」「I1 反向依赖」，防止治理后回潮。

---

## 7. 执行记录

决策（2026-10-07 用户拍板）：**形态 B 收拢为 `game/`** + **PathLock 移入 `downloader`** + **薄封装消除** + **`config/downloader.rs` 留在 `config/`**。

### 阶段 1：恢复编译基线 —— 已完成 `[复验]`

| 修复 | 内容 |
|---|---|
| `task/mod.rs:24-29` | 6 行 `pub use crate::{config::downloader::DownloaderConfig, constants, downloader, events, jdk, persist};` 全部改为私有 `use`（其中 `jdk` 与 `pub mod jdk` 同名冲突，故用具体项 `use crate::jdk::SUPPORTED_JAVA_MAJORS;`）；确认全仓无 `task::events` / `task::jdk` / `task::persist` 形式的外部依赖 |
| `task/mod.rs:133` | `jdk::SUPPORTED_JAVA_MAJORS` → `SUPPORTED_JAVA_MAJORS` |
| `task/mod.rs:16` | 补上缺失的 `pub mod jdk;`（HEAD 版有，工作区重构时漏了，是 E0425 的另一半原因） |
| `task/mod.rs:146` | `runner::run_install_jdk` → `jdk::run_install_jdk` |
| `task/mod.rs:131` | 注释「在 runner.rs」→「执行体在 task/jdk.rs」 |

验证：`cargo check --lib` 通过。

遗留：`cargo clippy --lib` 报 2 个 warning（`downloader/mod_loader/forge.rs:20,59` too_many_arguments，重构中丢了 `#[allow]`）。**不补 allow** —— 这两个函数（`download_and_extract` 170 行）正是阶段 3 要重构的对象，届时按 `TaskCtx` 上下文结构体消除参数。

### 阶段 2：模块边界迁移 —— 已完成 `[复验]`

目标树（形态 B）落地：

| 新位置 | 来源 | 说明 |
|---|---|---|
| `downloader/` | 保留 | 只剩传输层：`endpoints` `fetcher/{core,redirect,routes,mod}` `hash` `path_lock`(新) `probe` `progress` `proxy` `reuse`；`mod.rs` 重写为纯声明（原 17 行 `resolve_version_dir` 已迁出） |
| `game/version/{model,manifest,meta,dir,source}.rs` | `downloader/{model,manifest,version,source}.rs` | `version.rs` 按「元数据获取 / 本地目录」拆为 `meta.rs` + `dir.rs`（消除 `game::version::version` 的 module_inception） |
| `game/resource/{assets,library,extract}.rs` | `downloader/{assets,library,extract}.rs` | |
| `game/loader/` | `downloader/mod_loader/` | 另新增 `ctx.rs`（见阶段 2.5） |
| `game/community/` | `downloader/community/` + `wiki_entry.rs` | `wiki_entry` 声明为 `pub(crate) mod`（原文件缺失声明） |
| `game/modpack.rs` | `downloader/modpack.rs` | 1037 行，待阶段 3 拆分 |
| `game/updates/` | `downloader/mod_updates/` | |

**I1 反向依赖已断**：`task::wait_path_lock` 迁为 `downloader::path_lock::wait_path_lock`（`PathLock::try_acquire(&Path)` 无 TaskManager 参数，自持 `static ACTIVE_PATHS: LazyLock<Mutex<HashSet<PathBuf>>>`）；`TaskManager.active_paths` 字段、`acquire/release` 方法、2 个相关测试一并删除（测试迁移为 `downloader::path_lock::tests::acquire_release_roundtrip`）。

**调用点改写**：约 300 处 `crate::downloader::X` 路径（批量 sed）+ 12 个跨域 `use` 块手工拆解（`crate::downloader::{assets, library, model, …}` → 各归其域）。

验证：`cargo check --lib` 通过、`cargo clippy --lib` **零警告**、`cargo test --lib -- --test-threads=1` → **237 passed / 0 failed / 1 ignored**。

测试数核对：全仓 `#[test]`/`#[tokio::test]` 属性数 HEAD 240 → 现在 239（净 -1 = 删除 `task::tests::path_lock_acquire_release`/`path_lock_guard_drops_automatically` 2 个 + 新增 `downloader::path_lock::tests::acquire_release_roundtrip` 1 个），**逐文件对比 13 个搬迁文件全部一致**，无测试随搬迁丢失。运行 passed 比基线少 2（239 → 237）而非预期的少 1，这 1 个差额未定位到具体文件（文件级属性计数无差异），标记为待观察项。

### 阶段 2.5：ModLoader 参数结构收敛 —— 已完成 `[复验]`

用户问「too many arguments 是否可以优化」→ 是，且根因是缺参数对象。做法：新增 `game/loader/ctx.rs` 定义

```rust
pub struct VersionTarget<'a> { pub dir_name: &'a str, pub vdir: &'a Path }
pub struct InstallCtx<'a> {
    pub config: &'a DownloaderConfig,
    pub pc: &'a ProgressCounters,
    pub cancel: &'a Arc<AtomicBool>,
    pub hook: Option<LoaderHook<'a>>,
}
```

各安装器**首行解构**（`let VersionTarget { dir_name, vdir } = target;`），函数体零改动。

| 函数 | 前 | 后 |
|---|---|---|
| `install_loader` | 10 + `#[allow]` | 6 |
| `install_forge` | 9（clippy 警告） | 5 |
| `download_and_extract` | 9（clippy 警告） | 4（`source` 改为内部派生） |
| `install_loader_via_profile` | 9 + `#[allow]` | 6 |
| `install_fabric` / `install_quilt` | 8 + `#[allow]` | 5 |
| `install_neoforge` | 8 + `#[allow]` | 4 |
| `install_optifine` | 8 + `#[allow]` | 6 |
| `extract_neoforge_client` | 8 + `#[allow]` | 6（删未使用的 `_libraries`） |

附带收益：`task/modpack.rs::b_loader` 的 4 路 `match` 手写分派（40 行）→ 单次 `install_loader` 调用；`install_loader` 内的 `NO_CANCEL_ARC` 兜底逻辑删除（`cancel` 恒可用）。

**loader 下 6 处 `too_many_arguments` allow 全部消除**（非掩盖）。

剩余 allow（待阶段 3）：`downloader/assets.rs`、`game/modpack.rs`、`task/files.rs` ×2、`task/repair.rs`、`task/runner.rs`、`launcher/args.rs` ×3、`launcher/env.rs`、`launcher/process.rs` 等。

### 阶段 3：高危 bug 修复 —— 已完成 `[复验]`

| 项 | 位置 | 修法 |
|---|---|---|
| H1 锁序反转（ABBA） | `task/jdk.rs:47`、`task/runner.rs:135` | 改为 `let id = progress.try_lock().map(|p| p.task_id.clone());` 后 **语句末释放守卫**再 `set_cleanup_path`（后者取 inner 锁）；消除 progress→inner 反向锁序 |
| H2 RNG 失败被吞 | `online/scaffold/room.rs:27` | `create_room()` 改返回 `Result<RoomCredential, String>`，`getrandom` 失败上抛（此前失败时 bytes 全零 → 房间码与 network_secret 可预测）；调用点 `online/mod.rs:192` 用 `?`，测试用 `.unwrap()` |
| M2 裸切片 panic | `game/resource/assets.rs:118`、`launcher/env.rs:220`、`task/repair.rs:504` | 新增 `util::sha1_sub_dir(&str)`（`hash.get(..2).unwrap_or("00")`），三处统一调用；补单测覆盖空串/单字符/多字节边界 |

同时淘汰了扫描脚本标出的 `downloader/mod.rs::resolve_version_dir` 里的 `unreachable!()` 依赖（随文件迁移）。

验证：`cargo clippy --lib` 零警告、`cargo test --lib` 238 passed / 0 failed。

### 阶段 4：清空全部 `mod.rs` —— 已完成 `[复验]`

**非声明代码 1600 行 → 0 行**（脚本复核：剩余 15 行均为扫描正则漏判的多行 `use` 续行与 `pub(crate) mod`，属合法声明）。

| 原 mod.rs | 违规行 | 拆出的实现文件 |
|---|---|---|
| `online/mod.rs` | 612 | `model.rs`（状态模型/协议常量）、`service.rs`（单例/共享状态/快照/终止/工具）、`host.rs`（虚拟网+信息服务器+端口发现）、`guest.rs`（加入/代理/中心发现） |
| `task/mod.rs` | 346 | `manager.rs`（TaskManager 单例/终态收口/挂起决策）、`emit.rs`（事件队列线程 + 锁外 emit）、`spawn.rs`（任务注册与 spawn） |
| `game/loader/mod.rs` | 238 | `hook.rs`（LoaderEvent/LoaderHook）、`shared.rs`（profile 合并/继承展开/幂等判定/安装器进程管理）、`dispatch.rs`（统一入口） |
| `downloader/fetcher/mod.rs` | 164 | `api.rs`（11 个公开入口 + DownloadResult/FetchError） |
| `game/community/mod.rs` | 142 | `api.rs`（出参类型 + 按来源分派入口） |
| `persist/mod.rs` | 46 | `config.rs`（PERSIST_LOCK/PersistError/read/save/remove + 测试） |
| `jdk/mod.rs` | 31 | `config.rs`（配置读写 + 扫描注册 + SUPPORTED_JAVA_MAJORS + init） |
| `game/updates/mod.rs` | 4 | 常量下沉 `identify.rs`（`pub(super) const QUERY_CONCURRENCY`） |

附带修正：`TaskManager` 单例由 `OnceLock + set()` 改为 `LazyLock`（初始化表达式固定，消除 `global()` 的未初始化 panic 路径）；测试全部归位到被测算法的所在文件（`loader/shared.rs`、`task/manager.rs`、`task/model.rs`、`online/{model,service,host}.rs`）。

踩坑记录：搬迁 `online/host.rs` 测试时 `'\0'` 字面量被读取工具渲染为空字符，编译暴露后修正；搬迁中一度丢失 `task_status_serde_lowercase` 测试，已补回（全仓 `#[test]` 属性数复核 240 = HEAD 240）。

### 阶段 5：import 规范化 —— 已完成 `[复验]`

| 指标 | 起点 | 现况 |
|---|---|---|
| 函数体内 `use`（生产代码） | 55 处 | **0 处**（剩余 5 处经核实位于 `secrets.rs` 的 `#[cfg] mod key_utils` 等**子模块顶部**，属合法声明） |
| 方法体内三段 `crate::a::b::c` | 152 处 | **0 处** |
| clippy 警告 | 2 | **0** |

手法：按目录切片 5 路并行（persist/online、launcher、commands/auth、task、downloader/game），规则：上提保留 `#[cfg]`、trait 同名用 `as _`（`sha1::Digest`/`sha2::Digest` 实为同一 trait 重导出，只保留一条）、组内字母序。父任务统一编译并修掉 2 处遗留（`util.rs` 重复导入、`hash.rs` 冗余 trait）。

### 阶段 6：测试归位 + 注释精简 —— 已完成 `[复验]`

**`downloader/fetcher/tests.rs`（616 行独立文件）已删除**，18 个测试按归属并入实现文件：

| 归属 | 测试数 | 内容 |
|---|---|---|
| `fetcher/core.rs` | 3 | `is_mirror_url` 判定、镜像并发上限（`MIRROR_CONCURRENCY`）、`sort_routes_with_probes` 排序 |
| `fetcher/routes.rs` | 5 | 镜像候选生成、路由去重/追加、单选模式顺序、健康度冷却、Auto 健康排序 |
| `fetcher/redirect.rs` | 5 | 重定向黑名单/放行、真实 OptiFine 下载（`#[ignore]`）、黑洞拒绝、正常跟随、client reset 重建 |
| `fetcher/api.rs` | 6 | 取消即时返回、sha1 校验失败换源、全坏报 Checksum、空 sha1 跳过、错误信息聚合，**新增** `fetch` 单 URL 冒烟 |
| 共用 helper | — | `serve` / `temp_dest` 下沉 `core.rs` 的 `#[cfg(test)] pub(crate) mod test_support`，供兄弟测试模块复用 |

测试数：238 → **239 passed / 0 failed / 1 ignored**（新增的 `fetch` 单 URL 冒烟，此前公开入口 `fetch` 无覆盖）。

注释精简：注释密度 11.4% → 11.5%（行数随拆分增大），27 处 ≥8 连行块中，压缩了 6 处最长的模块文档（`online/mod.rs` 25→20、`online/session.rs` 20→17、`downloader/endpoints.rs` 17→10 与 14→6、`online/mc_advertise.rs` 16→14、`game/community/mod.rs` 12→9）——**删的是历史叙述**（"此前散落在…"、"相比原 REWRITE_RULES 的差异"、"旧实现…"），保留实测约束与原因。

### 阶段 7：其余审计项 —— 已完成（仅阶段 C 编排拆分留待单独一轮）

| 项 | 状态 | 说明 |
|---|---|---|
| 重复实现：并发下载循环 | ✅ 完成 | 新增 `downloader/batch.rs`（`BatchItem` + `run(items, concurrency, pc, cancel, max_errors)`），`assets.rs` 与 `library.rs` 那两份逐行相同的循环（各 ~90 行）合并为构造批 + 一次调用；顺带消除 `acquire_owned().await.unwrap()` |
| 重复实现：路径穿越守卫 | ✅ 完成 | `game/modpack.rs::safe_rel_path` 与 `jdk/download.rs::safe_runtime_path` 合并为 `util::safe_relative_path`（3 处调用点改为 `root.join(safe_relative_path(...)?)`，2 个本地测试合并为 1 个 util 测试） |
| 静默失败 | ✅ 完成 | 5 处加 stderr 诊断（落 launcher.log）：包身份标记写入、密钥文件 0o600 权限、默认游戏目录持久化、JDK 扫描结果 ×2；`task/store.rs::pool()` 初始化失败改为**一次性**留痕（避免每步刷屏，DB 故障不阻塞任务的语义不变） |
| `too_many_arguments` | ✅ 完成 | loader 6 处 + 本轮 9 处全部消除（引入 `TaskCtx`、`AssetIndexRef`、`ScanInput`、`JvmSubst`/`LaunchRequest`、`PackMeta`；`task/modpack.rs::phase_fetch_pack` 仅删 allow）。全仓该 allow 清零，仅余 `type_complexity` ×2 与 `upper_case_acronyms` ×3（Win32 类型，合理） |
| 速度探测双份 | ✅ 完成 | JDK 侧自建的 `speed_probe`/缓存/常量并入 `downloader/probe.rs`（新增 `probe_speed` + `SpeedProbeResult`，与 TTFB 探测共用 `cache_get/cache_set` 泛型缓存与 TTL/上限常量）；`jdk/download.rs` 删掉自建缓存与本文件 5 个 import。修正原描述：两处**测量不同**（HEAD TTFB vs 流式吞吐），重复的是缓存设施 |
| 终态收口两套 | ✅ 完成 | 抽出 `progress::set_done(app, progress, phase)` 作为 Done 收口唯一实现（runner 的 `update_and_emit`+`fail` 与 modpack 的 `finish_ok`+`remove` 各自一份 → 共用）；`set_error` 增加 cleanup_path 处理（取走登记路径 + `spawn_blocking` 整删），runner 与 modpack 失败路径的**同步** `remove_dir_all`（阻塞任务线程）统一改为先 `set_cleanup_path` 登记 |
| `unwrap/expect` 清理 | ✅ 完成 | 17 处非测试点分类处理：**消除 9 处**（`fetcher/core.rs` 的 `last_err.unwrap()`、`task/modpack.rs` 的 `expect("阶段顺序不变量…")` 改返回 `Err`、`launcher/env.rs` 的 `last().unwrap()` 改 `map`、`online/guest.rs` 的 `session.as_mut().unwrap()` 改 `if let`、`identity.rs` 的 `parent().expect` 改 `if let`、`persist/secrets.rs` 4 处 `Key/Nonce::try_from(..).unwrap()` 改 `map_err + ?`）；**保留 8 处**并加"为何不可达/为何不处理"注释（Tauri 顶层、tokio runtime 构建、事件线程 spawn、semaphore、client 一次性构建 ×2、两处 `for n in 1..` 后的 `unreachable!()`） |
| 长函数拆分 | 🟡 部分完成 | `run_task` **534 → 472 行**：抽出 `prepare_runtime`（配置/目标目录/复用根/路径互斥，返回 `TaskRuntime`）与 `stage_fetch_version`（阶段 A：拉取 JSON + 落盘 + 登记清理），解构后**局部变量名与原一致**故函数体零改动。**阶段 C 的并行编排未拆**——它把 jar/libs/assets/natives/loader 的 future 与 20+ 计数器交织，拆分必须配端到端下载验证（`cargo test --test downloader_test -- --ignored` 或真实安装一次），当前环境不具备该验证条件，留待单独一轮 |
| 其余 10 个长函数（`run_repair_task` 230 行、`join` 196 行等） | ⬜ 未做 | 均为「线性编排 + 内联步骤」，拆分收益低于 `run_task`；建议与阶段 C 一起在能跑端到端验证的轮次处理 |

每阶段收口：`cargo check` + `cargo clippy --lib`（零警告）+ `cargo test --lib -- --test-threads=1`。
