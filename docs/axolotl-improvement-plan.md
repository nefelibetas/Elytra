# ElytraLauncher 参考 Axolotl 的技术改进计划

> 调研对象：[Mystic-Stars/Axolotl](https://github.com/Mystic-Stars/Axolotl)（Tauri v2 + Rust workspace 的 Minecraft 启动器，Modrinth App fork）
> 调研方式：clone 到本地精读 `packages/app-lib/src/` 核心实现，与 ElytraLauncher 现有代码逐项对比
> 覆盖范围：高价值 4 项 + 中价值 3 项（sqlx 已确认可行但不在本文档展开；updater 明确不做）
> 文档定位：**计划文档**——先立项，落地时按"阶段/步骤"拆 PR

---

## 0. 总览

| # | 项目 | 价值 | 工作量 | 落点模块 |
|---|---|---|---|---|
| 1 | 本地运行时复用（LocalRuntimeSource） | 高：迁移用户省数百 MB 下载 | 中（纯新增，不触碰现有下载路径） | `downloader/` 新模块 + `task/runner.rs` |
| 2 | JRE 自动下载（managed Java） | 高：补齐"扫描→下载→校验→注册"闭环 | 中（新增依赖 + 新模块） | `jdk/` 扩展 + 新 `jre_download` |
| 3 | Forge processor 幂等跳过 | 高：重装/修复省时显著 | 低（单函数 + 集成点） | `mod_loader/forge.rs` |
| 4 | 离线模式 strict cache-only | 高：离线体验正确性 | 低（加参数穿透） | `config/downloader/source.rs` + `launch.rs` |
| 5 | 下载源四档模式 + URL 自动分类 | 中：国内体验自适应 | 中（模型扩展 + fetcher 改造） | `config/downloader/model.rs` + `fetcher.rs` |
| 6 | 安装任务持久化 + 恢复 + 回滚 | 中：重启/崩溃不丢任务 | 大（引入 sqlx，重写 task 层） | `task/` 全量 + persist |
| 7 | 下载进度速度/ETA/停滞检测 | 中：DownloadDetail 体验 | 低 | `task/progress.rs` + 前端 |

---

## 1. 本地运行时复用（LocalRuntimeSource）

### 旧方案（Elytra 现状）

用户换启动器（从 HMCL/PCL2/官方启动器迁移）时，`run_task`（`src-tauri/src/task/runner.rs:25`）对版本全部文件**从头下载**：

- 阶段 B `init_local_scan`（runner.rs:539-658）只检测**目标版本目录内**的文件（`{id}_n` 递增命名、残缺目录复用），从不看用户已有的其他 `.minecraft`；
- libraries/assets 的"已有即跳过"（`library.rs:26-85`）仅在**本启动器自己装过的** `.minecraft` 内生效；
- 结果：迁移用户 1.20.4 原版 ≈ 客户端 jar 200MB + assets 300MB + libraries 100MB，即使本机官方启动器刚下过，也要全量重下。

### 新方案（Axolotl 做法）

`packages/app-lib/src/launcher/download.rs:44-117`：

1. **探测本地运行时根**（`LocalRuntimeSource::discover`，:48-68）：
   - 依次检查：目标实例/`.minecraft` 自身 → `ancestors()` 向上找含 `versions/` 目录的祖先（PCL2/HMCL 的多版本目录形态）；
   - 判定条件 `is_local_runtime_root`（:70-72）：`assets/` 与 `libraries/` 同时存在即视为可复用运行时根。
2. **逐产物复用**（`classify_local_artifact`，:81-117，三级判定）：
   - 目标文件已存在 → `Cached`（本启动器已装）；
   - 本地源有同相对路径文件 → **size 比对 + sha1 比对**（:104-112）→ 匹配 `LocalReusable`，否则 `NetworkRequired`；
   - 拿不到期望 sha1 → 直接 `NetworkRequired`（绝不盲复制）。
3. **验证后复制**（`copy_verified_local_artifact`，:119-152）：sha1 命中才复制，复制计入进度、失败当普通下载。
4. **集成方式**（`download_or_reuse_local`，:664-697）：每个下载点包一层"先试本地复用，未命中再走网络"，`force`（修复模式）时跳过复用强制重下。

下载路径全部走该包装：client jar（:1277）、assets index（:1345）、log config（:1859）、libraries（:1445/1594/1668/1727）。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| 迁移场景 | 全量重下数百 MB | size+sha1 双校验后本地复制，瞬时完成 |
| 安全性 | —（不涉及） | sha1 不匹配绝不复制，镜像陈旧/损坏文件天然免疫 |
| 修复模式 | — | `force=true` 跳过复用，修复语义不受影响 |
| 侵入面 | — | 新增模块 + 每个下载点包一层，现有逻辑不动 |

### 落地计划（3 步）

1. 新建 `src-tauri/src/downloader/reuse.rs`：
   - `LocalRuntimeSource` / `is_local_runtime_root` / `classify_local_artifact`（复用 `hash.rs` 的 `file_sha1_matches`，但改为分块读取避免整文件读内存）；
   - `download_or_reuse` 泛型包装（closure 内是现有下载逻辑）。
2. `runner.rs` 阶段 B 增加探测：目标版本目录的 `ancestors()` 找复用根（或前端"选择要复用的 .minecraft"透传路径），传给各下载函数。
3. `library.rs` / `assets.rs` / `version.rs` 的下载入口包上 `download_or_reuse`；`resolve_target_dir` 的"残缺目录复用"逻辑保持不变。

> 备注：Axolotl 的 sha1 校验读整个文件（`sha1_file_async`），Elytra 已有整文件读内存取舍，但复用场景下 assets 可能数千文件，建议用 `BufReader` 分块算 sha1，避免大文件（client jar 200MB）峰值内存爆炸。

---

## 2. JRE 自动下载（managed Java）

### 旧方案（Elytra 现状）

`src-tauri/src/jdk/` 只有**扫描与发现**：

- `scanner.rs:22-32` 遍历已知候选目录（官方启动器 runtime、HMCL、Prism、.jdks 等）；
- 用户机器没有现成 Java 时，`auto_pick_jdk`（`config/launcher/env.rs:187-208`）无解——直接启动失败报"未找到 Java"；
- 无任何下载/安装/自管理运行时能力。

### 新方案（Axolotl 做法）

`packages/app-lib/src/api/jre.rs`：

1. **三级查找**（`find_java_for_version` :420-451 → `find_compatible_java_for_version` :457-468 → `auto_install_java` :480-498）：
   - ① 已注册的 exact 版本（`JavaVersion::get`，失效自动降级移除默认）；
   - ② 全盘扫描 exact 匹配；
   - ③ 最接近的新版本（`closest_compatible_java`，:470-478，>= 需求的最小 major）；
   - ④ 都无 → 自动下载。
2. **双源下载**（`auto_install_java_inner`，:675-734）：
   - 首选 **Mojang 官方 runtime**（`install_mojang_runtime`：读 `piston-meta` 的 java-runtime all.json → 按版本映射组件名 `jre-legacy/java-runtime-gamma/delta/epsilon`，:736-745 → 按平台映射 `windows-x64/mac-os-arm64` 等，:747-758 → 遍历 manifest 逐文件 sha1 校验下载，symlink 条目解析（:785+））；
   - 失败回退 **Azul Zulu**（`install_azul_runtime`：查 Azul metadata API → sha256 校验 → xz2 解压）。
3. **安装确认弹窗**（`confirm_java_download`，:655-673）：通过事件询问用户，拒绝可"推迟到本次"（postpone，不打扰流程）。
4. **注册为 managed Java**（`register_managed_java_versions` :41-109）：扫描 `java_versions_dir/` 下已装运行时，`-jdk-` 目录名匹配 major 时自动设为默认版本；`check_jre`（:1454）启动时重新校验路径有效性。
5. **全局互斥**：`JAVA_INSTALL_LOCK`（:504）防止并发重复下载同一版本。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| 无 Java 机器 | 启动直接失败，用户手动装 | 自动下载 + 进度上报，装完即启动 |
| 官方启动器迁移 | 能扫到 runtime（已有） | 还能补"官方没有的版本"（如 8/25） |
| 下载源 | — | 官方 runtime 优先 + Azul 回退（还可扩 BMCLAPI） |
| 下载安全 | — | 逐文件 sha1 + 包 sha256 + manifest 路径穿越校验（`safe_runtime_path` :768-783） |
| 用户打扰 | — | 首次弹确认框，可推迟 |

### 落地计划（4 步）

1. Cargo.toml 新增依赖：`xz2`（或 `liblzma` 若已存在）解压 tar.xz——先确认是否已在依赖树（`zip` 已有）。
2. 新建 `src-tauri/src/jdk/download.rs`：
   - `MojangRuntimeIndex` 拉取与组件/平台映射（版本映射表：8→jre-legacy、17→gamma、21→delta、25→epsilon）；
   - 逐文件 sha1 下载到 `{minecraft_dir}/runtime/`（镜像复用 `fetcher.rs` + `DownloaderConfig` 源）；
   - `safe_runtime_path` 路径穿越防护；
   - 完成后 `inspect()` 校验版本并写入 `persist` 的 `jdk` section（对齐现有 `JdkInfo`）。
3. `jdk/mod.rs` 增加 `auto_install(major)`，`env.rs::auto_pick_jdk` 找不到时调用（命令入口：`commands/jdk.rs` 加 `install_jdk`，注册到 lib.rs）。
4. 前端 SettingsJdk 加"下载缺失 JDK"按钮（或启动失败时引导），复用现有 toast/AlertDialog 错误提示。

> 备注：Mojang runtime 国内可达性一般，Azul 也不一定稳；落地时下载源并入 `DownloaderConfig.sources`（BMCLAPI 也有 `/java-runtime` 镜像），优先级放最低成本实现——先官方 + Azul 双源即可。

---

## 3. Forge/NeoForge processor 幂等跳过

### 旧方案（Elytra 现状）

`src-tauri/src/downloader/mod_loader/forge.rs` 的 `extract_forge_client_lzma`（:247-392）与 neoforge 的 `extract_neoforge_client`：

- **每次安装/修复都重跑** binarypatcher / installertools；
- 幂等性仅靠"目标文件已存在且 sha1 匹配则跳过"（forge.rs:387-390 合成后校验）——**中间产物（patch 过程）无校验**，且判断在运行之后；
- 运行前无"processor 输出是否已是最新"的预检查；重装场景（版本已装好、仅修复某库）白白跑一遍 Java 工具（forge.rs:425-437，慢且偶尔触发杀软误报）。

### 新方案（Axolotl 做法）

`packages/app-lib/src/launcher/mod.rs:259-359` `processor_outputs_are_current`：

1. **声明式输出校验**：遍历 processor 的 `outputs`（`{模板路径} → {sha1 模板}`）；
2. **binarypatcher 特殊推断**（:273-305）：无 `outputs` 字段时，从 args 找 `--output` 后的 `{key}` 模板 + 对应 `{key}_SHA`，推断出输出路径与期望 hash（`processor_output_sha1` :249-257 容错 `sha1:` 前缀）；
3. **路径白名单**（:306-346）：输出必须 canonicalize 后在允许根（instance 目录 + libraries 目录）内，防恶意模板写外部路径；
4. **命中即跳过**（:956-992）：运行每个 processor 前先调用；全部输出存在且 sha1 符合 → `Skipped ... outputs are valid`，直接进下一步，不 spawn JVM；
5. 未命中才 `run_instance_install_command`（:535-597，含 CancellationToken 取消支持）。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| 重装/修复 | 无条件重跑 binarypatcher（数秒～数十秒 + 杀软风险） | 输出 sha1 命中即跳过 |
| 校验时机 | 运行后单点校验 | 运行前逐输出声明校验 |
| 中间产物 | 无校验（`.forge_patched_minecraft` marker 已用） | 每个声明的输出都校验 |
| 安全性 | — | 输出路径白名单（instance/libs 根内） |
| 改造量 | — | 纯函数 + 循环前调用，零架构改动 |

### 落地计划（2 步）

1. `mod_loader/mod.rs` 或 `forge.rs` 加 `processor_outputs_are_current`（平移 Axolotl 逻辑：`--output` 推断 + canonicalize 白名单 + sha1 校验；`sha1` 用 `hash.rs`）。
2. `extract_forge_client_lzma` / `extract_neoforge_client` 运行工具前先调用；`run_instance_install_command` 加取消透传（对齐 `runner.rs` 现有 cancel 标志）。

---

## 4. 离线模式 strict cache-only

### 旧方案（Elytra 现状）

离线启动路径（`commands/launch.rs:289-344` → `config/launcher/env.rs`）：

- `load_launch_config` 只读本地 JSON，**启动本身不触网**（这点已正确）；
- 但离线模式**未显式声明"禁止网络"**——`pre_launch_check` / `build_launch_command` 是纯本地的，一旦未来某处（如 `resolve_minecraft_manifest` 等价逻辑、皮肤纹理、mod 检查）引入网络调用，离线模式会意外挂起/失败；
- 离线皮肤 `Player` 模式（`skin.rs` 走 `mojang::name_to_uuid` / `texture_value`）会触网——这是设计内行为，但无"离线模式强制跳过"的统一开关。

### 新方案（Axolotl 做法）

`packages/app-lib/src/launcher/mod.rs:1267`：

```rust
let cache_behaviour = offline_mode.then_some(CacheBehaviour::CacheOnly);
```

- 所有元数据查询（manifest / loader 版本 / 版本 JSON）显式传 `CacheBehaviour`，离线 = `CacheOnly`（只读缓存，命中返回，未命中报错，**绝不发网络请求**）；
- 版本不在缓存 → 强制刷新（`resolve_minecraft_manifest_with_cache` :476-520 对在线场景 `MustRevalidate`，对离线场景保持 `CacheOnly`）；
- 离线启动连 log config 都走本地（`ensure_local_log_config`，mod.rs:1344-1350）。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| 离线语义 | 隐式（碰巧不触网） | 显式 `CacheOnly` 枚举贯穿查询层 |
| 未来风险 | 新增网络调用可能破坏离线 | 编译器+运行时双重保障 |
| 未命中行为 | — | 明确报错而非尝试网络（可给出"请联网一次"提示） |

### 落地计划（2 步）

1. `config/downloader/source.rs` 增加 `CacheBehaviour` 枚举（`CacheOnly` / `MustRevalidate` / 默认 TTL），`fetch_with_fallback` / `fetch_json_with_fallback` 加参数（或新函数），TTL 缓存命中逻辑保留。
2. `commands/launch.rs` 离线分支传 `CacheOnly`：`prepare_launch` 透传 `offline` 标志到启动期所有元数据获取点。

> 小改动但语义价值大——建议与 #1 同 PR 落地。

---

## 5. 下载源四档模式 + URL 自动分类（自适应路由）

### 旧方案（Elytra 现状）

`src-tauri/src/config/downloader/model.rs`：

- `DownloadSourceMode` 四值（:127-135）：`OfficialOnly / ChinaOnly / PreferOfficial / PreferChina`，语义 = `sources[]` 数组的两元素排序；
- 模式是**全局的**，不分资源类别——`file_source_mode / manifest_source_mode / community_source_mode` 三个字段手工管理；
- `mirror_url`（:35-47）重写是**源级 URL 模板**：仅当主源是 BMCLAPI 时按 `REWRITE_RULES` 把 mojang 域搬移到 bmclapi 根；`is_mirror_url`（fetcher.rs:26-32）仅做**并发限制判定**（BMCLAPI 8 并发）；
- 无失败记忆、无健康度概念——镜像源一坏，每轮下载都要先撞一遍失败。

### 新方案（Axolotl 做法）

`packages/app-lib/src/util/fetch.rs`，三件套：

1. **按 URL 自动分类资源类**（`infer_resource_class` :1069-1104）：host 匹配表（`resources.download.minecraft.net`→asset、`libraries.minecraft.net`→library、`maven.*`→loader、`piston-meta` 且含 `java-runtime`→java、`api/cdn.modrinth.com`→modrinth、forgecdn→curseforge……），未知 → Other。
2. **四档模式按类生效**（`DownloadSourceMode`：`Auto / OfficialOnly / OfficialPreferred / MirrorPreferred`，settings.rs:13；`source_mode_for_resource` :1048-1067 按 `ResourceClass` 取对应配置；`resolve_download_routes_for` :1017-1046 排序）。
3. **显式镜像路由表**（`explicit_mirror_routes` :792-910）：无需"源模板重写"，任何 mojang/modrinth/curseforge/maven URL 都能**即时生成** BMCLAPI/MCIM 镜像候选（含 `/maven{path}`、`/assets{path}`、`/fabric-meta{path}`、neoforged `/releases/` 前缀剥离等细节），官方 URL 始终在候选列表中保底。
4. **路由健康度自适应**（`ROUTE_HEALTH` :306-337 + `order_auto_routes` :936-984）：
   - 每次请求记录 TTFB/吞吐量（EWMA α=0.25）、连续失败数、cooldown（`retry-after` 可写入 :3188-3253）；
   - Auto 模式下排序键：冷却中 → 失败多 → 无成功样本 → 按 TTFB（元数据）或吞吐量（大文件）→ 冷启动偏好；
   - `mirror_first_loader_routes`（:989-1015）：`maven.minecraftforge.net / fabricmc.net / neoforged.net` **强制镜像优先**（官方保底）——Loader 资源永远镜像先行，正是 Elytra 国内场景最痛的点；
   - 302 后记录实际生效域名（`remember_effective_route_authority` :363-396），去重路由防镜像与官方 302 到同一域名重复尝试。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| 分类粒度 | 3 个全局模式字段 | 5 类资源 × 4 档模式 + URL 自动推断 |
| 镜像 URL 生成 | 源模板重写（须主源是 BMCLAPI） | 任意官方 URL 即时生成镜像候选，官方保底 |
| 失败处理 | 每轮从头撞 | 连续失败冷却 + 排序降权 |
| 速度感知 | 无 | TTFB/吞吐量 EWMA，Auto 模式动态排序 |
| Loader 场景 | 手动调模式 | `mirror_first_loader` 强制镜像优先 |
| 并发限制 | 镜像 URL 静态 8 并发 | 保留（`is_mirror` 字段驱动，同样适用） |

### 落地计划（4 步，建议拆 2 个 PR）

PR-A（模型层，纯新增）：
1. `model.rs` 的 `DownloadSourceMode` 扩展为 5 值（加 `Auto`），`DownloaderConfig` 保留兼容反序列化（`from_string` 兜底），迁移持久化旧值；
2. 新增 `ResourceClass` 枚举 + `infer_resource_class(url)`（host 匹配表，含 BMCLAPI/MCIM 判定）。

PR-B（路由层，替换式改造）：
3. `fetcher.rs` 重构：`fetch_from_sources` 改走 `resolve_download_routes_for(url, class, mode)` 生成路由列表（`DownloadRoute { url, is_mirror, supports_range }`），替代现有"源数组 + mirror_url 重写"；
4. 加 `ROUTE_HEALTH`（DashMap/OnceLock<Mutex<HashMap>>）+ `order_auto_routes` + `record_route_success/failure`（cooldown、EWMA）——先把 `mirror_first_loader` 规则落地（Loader 三 maven 强制镜像优先），健康度排序作为 Auto 模式的第二优先级。

> 改造量最大的点：`mirror_url` 目前在 `DownloadSource` 层，重构后 URL 生成逻辑上移；`is_mirror_url`（并发限制）改由 `DownloadRoute.is_mirror` 字段驱动，保留 MIRROR_SEM 语义。

---

## 6. 安装任务持久化 + 恢复 + 回滚（sqlx）

### 旧方案（Elytra 现状）

`src-tauri/src/task/mod.rs`：

- TaskManager 纯内存（:22-30）：`HashMap<task_id, TaskHandle>`，**应用退出即丢失**；
- 取消 = `cancel` 标志 + 可选 `cleanup_path` 整删（:147-161），崩溃时无任何恢复手段——残缺版本目录只能靠下次 `resolve_version_dir` 的"缺 client jar 复用目录"兜底；
- 无"等待用户"（如必装 Mod 缺失需选择跳过/重试）状态；
- 失败即从 map 移除，前端只能看到一次 error 事件。

### 新方案（Axolotl 做法）

三层：

1. **Job 状态机全事件化**（`install/model.rs`）：`InstallJobState { request, target, cleanup, progress, events, rollback, error, pause_reason, continuation }`（:32-60）；
   - 状态流转 `Queued → Running → {Succeeded | Failed | Interrupted | Canceled | WaitingForUser}`（:1202-1255）；
   - 每步 `record_event(InstallJobEventKind::…)`（:820-928）：PhaseStarted / ContentFileQueued / DownloadRequestStarted / ContentFileFailed / WaitingForUser / RollbackStarted……全部带时间戳可审计；
   - 下载项级状态机（:1102-1114）：`Queued → Downloading → Verifying → Writing → Completed / Skipped / Failed / Canceled`，每项带 attempts/max_attempts/错误信息。
2. **SQLite 持久化**（`install/store.rs` + sqlx 迁移）：`install_jobs` 表存序列化 JobState；`list_interrupted_candidates`（:221）启动时捞出 `Interrupted` 任务，前端询问"恢复上次任务？"；`retry_job / resume_job / skip_missing_content_and_resume_job`（runner.rs:218/363/377）支持从断点续跑；`complete_running_job`（store.rs:403）处理崩溃现场。
3. **回滚**（`install/recovery.rs` + runner.rs）：
   - 新实例任务：`InstallCleanup::DeleteNewInstance`——失败/取消整删（对齐 Elytra 现有语义，但**结构化为 JobState 字段**）；
   - 既有实例任务：`InstallCleanup::RestoreExistingInstance`——先 `prepare_existing_rollback`（:2137-2159）快照原实例元数据 + content，失败时 `apply_cleanup` 还原；
   - `WaitingForUser`（:2476 测试：`missing_required_content_pauses_without_starting_rollback`）——必装 Mod 下载失败时**暂停而非回滚**，用户选择"跳过/重试"后 `skip_missing_content_and_resume_job` 继续，被跳过的项进 `skipped_missing_content_paths`。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| 崩溃/重启 | 任务丢失，目录残留 | Interrupted 状态 + 启动时提示恢复 |
| 失败现场 | 只有一次 error 事件 | 完整事件日志 + 下载项级失败明细（原因/尝试次数/manual_url） |
| 取消 | 整删目录（cleanup_path） | 结构化 `InstallCleanup`（新建删除 / 既有还原） |
| 必装内容缺失 | 任务直接失败 | WaitingForUser 暂停 + 用户选择跳过/重试 |
| 审计 | 无 | 全事件带时间戳，前端可展示任务时间线 |
| 依赖 | 无 | sqlx + SQLite（已确认引入） |

### 落地计划（建议单独立项，4 步）

1. **引入 sqlx**：`Cargo.toml` 加 `sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }`（无需 migrate 宏，运行时 `SqlitePoolOptions`）；库文件路径沿用 `persist` 约定（exe 旁 `Elytra/` 目录）或独立 `Elytra/tasks.db`；初始 schema：`install_jobs`（id, state_json, status, created_at, updated_at）+ 索引。
2. **任务状态机迁移**：`task/model.rs` 扩展 `TaskProgress`（加 phase 枚举、events 队列），`runner.rs` 每步 `record_event`；先保底"失败即存盘"——不追求全量事件化，第一版只持久化 JobState 快照（请求 + 阶段 + 已下载项列表）。
3. **恢复入口**：`commands/task.rs` 加 `list_interrupted_tasks` / `resume_task` / `cancel_task`（复用现有 task 注册机制）；`task::init()` 时若存在 Interrupted 任务推事件给前端（复用 `task-progress` 或新 `task-interrupted` 事件）。
4. **回滚与暂停**（第二迭代）：先做 `InstallCleanup::DeleteNewInstance`（等价现有整删，但结构化）；`RestoreExistingInstance`（补全/修复类任务快照）与 `WaitingForUser` 暂停放二期。

> 与 #7 共用 task 层改造，建议同 PR 分批落地。注意 AGENTS.md 红线：persist 用 Mutex 串行化，引入 sqlx 后 TOML 与 SQLite 并存期间，`Settings.toml` 读写路径不要动（只加新文件）。

---

## 7. 下载进度速度/ETA/停滞检测

### 旧方案（Elytra 现状）

`src-tauri/src/task/progress.rs`：

- 150ms 心跳 + 字节差分 EMA 速度（:106-119，α=0.6/0.4）——已有速度；
- **无 ETA**：`TaskProgress` 无 `eta_seconds` 字段；
- **无停滞检测**：单文件下载慢（镜像黑洞）时进度条干等，无法区分"慢"与"死了"；
- 无"当前正在下载哪个文件/哪个源/第几次尝试"的信息——DownloadDetail 只有总量进度。

### 新方案（Axolotl 做法）

`packages/app-lib/src/install/model.rs`：

1. **`ActiveDownloadState`**（:1117-1135）：每个在途下载一个条目：url / source / bytes_downloaded / bytes_total / attempt / max_attempts / `last_progress_at` / `speed_bytes_per_second`（采样窗口起止字节差）。
2. **`DownloadItemSnapshot`**（:1137-1156）：对前端暴露的只读视图——单文件状态机（含 `Skipped` 与 `manual_url`：CurseForge 需浏览器手动下载时给链接）、attempt/max_attempts、error。
3. **`DownloadJobSummary`**（:1158-1168）：聚合视图——files_completed/total、bytes、`speed_bytes_per_second`、**`eta_seconds`**（bytes 余量 ÷ 速度）、当前源 + fallback 次数（`DownloadMetrics` 事件）。
4. **停滞判定**（:632-643 测试）：`last_progress_at` 距今 > 4s → 速度/ETA 置 None（不显示假数字）；重试后字节回退 → `sub_bytes`（download.rs:426，重试不虚增已下载量）。

### 对比

| 维度 | 旧（Elytra） | 新（借鉴 Axolotl） |
|---|---|---|
| ETA | 无 | 剩余字节/速度，停滞时自动隐藏 |
| 单文件状态 | 只有 4 个文件级计数器 | 每文件状态机 + 尝试次数 + 手动下载链接 |
| 停滞 | 无感知 | 4s 无进展即速度/ETA 置空 |
| 重试计数 | 无 | attempt/max_attempts 可见 |
| 前端改动 | — | DownloadDetail 补列（文件状态/源/ETA） |

### 落地计划（2 步）

1. 后端：`task/model.rs` 的 `TaskProgress` 加 `eta_seconds: Option<u64>` + `active_downloads: Vec<ActiveDownloadState>`（或仅暴露聚合 summary）；`progress.rs` 的 EmitLoop 聚合计算——需要单文件字节回调整合（现在 library.rs 是全局原子计数器，加"当前文件进度 + 上次推进时间"两个原子即可）。
2. 前端：`src/pages/DownloadDetail` 增加"当前文件/源/尝试次数/ETA"展示，事件结构扩展由 TS interface 镜像（AGENTS.md 约定）。

> 最小实现：只加 `eta_seconds` + 停滞隐藏，单文件明细可后置（DownloadDetail 已有文件列表）。

---

## 8. 实施顺序建议

| 批次 | 内容 | 理由 |
|---|---|---|
| **第一批（小而稳）** | #3 processor 幂等、#4 离线 cache-only、#7 进度 ETA | 均为纯增量/单点改造，风险低，收益立竿见影 |
| **第二批（迁移体验）** | #1 本地运行时复用、#2 JRE 自动下载 | 独立模块，不触碰现有下载主路径，需网络验证 |
| **第三批（架构）** | #5 自适应下载源 | fetcher 重构，需回归全部下载场景，建议独立测试轮次 |
| **第四批（大工程）** | #6 sqlx 任务持久化 | 引入数据库 + task 层重写，先立项做设计评审，可跨多个版本迭代 |

每批完成后跑：`cargo check` → `cargo clippy` → `cargo test --lib -- --test-threads=1`，前端 `pnpm build`（vue-tsc 全量类型检查）。
