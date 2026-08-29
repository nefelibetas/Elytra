# 功能路线图（待实现/已决策）

> 记录功能缺口分析（2026-08）后的决策。已实现项从清单移除，保留决策依据。
> 缺口分析来源：全仓扫描（AuthMode 枚举、launcher/args.rs 注入点、task/store.rs 快照机制等）。

## 已确认要做（按优先级）

### 1. 服务器直连（--server/--port 快速进服）— 低难度
- **现状**：`launcher/args.rs` 无 `--server`/`--port` 注入点；`PLACEHOLDER_QUICK_PLAY_PATH` 恒空、
  `is_quick_play_*` 恒 false；`LauncherConfig` 无 server/port 字段。
- **方案**：`LauncherConfig.server: Option<String>` + `port: Option<u16>`；`build_launch_command`
  在 game args 追加 `--server <ip> --port <port>`；前端启动页加"服务器地址/端口"输入（可选填）。
- **与联机的联动**：房间码进服已走 MC 局域网发现（广播模拟），无需 server 直连；手动 IP 进服
  场景（跨版本/非 MC 26.2 广播兼容）补上此口。

### 2. 游戏日志查看 UI — 低难度
- **现状**：stdout/stderr 已落盘 `.minecraft/logs/elytra-launch.log`（`commands/launch.rs`）；
  崩溃弹窗仅给 crash-report 路径，不扫 `latest.log` / `hs_err*.log`。
- **方案**：启动页/设置页加"日志"入口（命令读 logs 目录最近日志，前端只读展示）；
  崩溃弹窗补扫 `latest.log` 摘要。

### 3. Mod 启用/禁用 + 依赖冲突提示 — 中难度
- **现状**：`VersionSetupMods.vue` 仅删除/更新/开文件夹；后端 `CommunityDependency` 已返回
  required/optional/**incompatible**/embedded，前端只勾选 required 且无冲突提示。
- **方案**：Mod 列表加禁用（`.disabled` 后缀或状态表）+ 一键启用/禁用；ResourceDetail 展示
  incompatible 冲突（安装前拦截或警示）。

### 4. 下载暂停/继续（任务级）— 中难度
- **现状**：任务中心仅"取消"；终态无 paused；崩溃恢复 = 快照重跑。
- **方案**：**不做字节级 Range 续传**（下载全程 sha1 校验，半文件无法校验，整文件重下与
  现有文件级幂等契合）。**暂停** = 取消当前 fetcher（`fetcher` `_c` 变体已优雅取消）+
  保留任务快照 + 状态标 `Paused`；**继续** = 快照重跑（已完成文件 sha1 跳过，未完成整文件重下）。
- **收益**：大整合包下载 80% 时暂停 → 继续仅重下未完成文件，省已完成文件带宽/时间。
- **基础设施已具备**：`_c` 可取消 fetch、`task/store.rs` 快照恢复、文件级 sha1 幂等。

### 5. 下载限速 — 中难度
- **现状**：前端 `max_speed` 字段已声明但保存时不传（死配置）；后端无实现。
- **方案**：fetcher 加限速器（令牌桶/每块 sleep 节流，`bytes/sec`），任务配置透传；
  SettingsDownload 加限速输入。

## 已决策：不做 / 砍掉（记录理由）

| 项 | 理由 |
| --- | --- |
| 离线自定义皮肤（独立做） | **无通用实现路径**：1.19.3+ authlib 不再解析 `--userProperties`，皮肤只能来自正版 sessionserver / 外置登录 / CustomSkinLoader mod。单独做只能服务 ≤1.19.2 或依赖第三方 mod。**改由外置登录（authlib-injector）一并接管**，见 `docs/authlib-injector-plan.md`。 |
| LiteLoader 支持 | 已砍（代码零残留，2026-08）。无用户量，版本查询也无价值。 |
| 暗色模式 | 已有 4 套亮色主题 + 自定义编辑器；暗色需全 token 适配，无用户诉求。 |
| 关于/更新页 | 无发行渠道（更新服务）时无意义；版本号可在 TitleBar 或设置页脚展示。 |
| 手动代理 / PAC | 系统代理三态（Auto/System/Direct）覆盖 95% 场景；手动代理是极少数网络需求。 |
| 自定义下载源 | 官方 + BMCLAPI 双源 + Auto 健康度路由已覆盖国内场景；自定义源需 API 格式适配，用户群小。 |
| 字节级断点续传 | 校验要求完整文件，半文件无法校验；任务级暂停/继续（见上）已满足实际需求。 |
| 任务队列 | 并发多任务 + PathLock 防冲突已够；队列是 UX 优化非必需。 |
| 全屏启动选项 | 游戏内 F11，零价值。 |
| 正版 Slim 换肤 | variant 固定 CLASSIC；正版 slim 模型用户小众，需要时随手加 dropdown。 |
| token 过期预判 | 启动前多一次 API 调用换提示；刷新失败时已有报错链路。 |
| 整合包 CF restricted 文件 | CurseForge 版权文件分发限制，无法程序化解决；收尾提示手动放置已是上限。 |
| CurseForge API Key | 构建期 `option_env!` 注入；属发版流程问题（配 .env），非功能缺口。 |

## 参考

- 缺口全量分析：会话记录 2026-08-29（GapScan 全仓扫描）
- 外置登录详细规划：`docs/authlib-injector-plan.md`
