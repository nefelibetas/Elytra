# ElytraLauncher

基于 **Tauri v2（Rust）+ Vue 3（TypeScript）** 的 Minecraft 启动器。

## 功能

- 游戏版本下载：官方源 / 国内镜像多源回退，sha1 校验，并发与限速
- ModLoader：Fabric / Forge / NeoForge / OptiFine 安装，Quilt / LiteLoader 版本查询（Quilt 暂复用 Fabric 流程）
- 账号：微软正版登录（OAuth PKCE）、离线模式
- 离线皮肤：Steve / Alex / 随机 / 自定义 / 正版玩家皮肤（对齐 PCL2 机制，仅 1.19.2 以下）
- JDK 自动扫描与自定义配置
- 多游戏目录、下载任务管理、启动参数构建与游戏进程管理

## 开发

```bash
pnpm install          # 安装依赖（必须用 pnpm）
pnpm tauri dev        # 开发模式：vite :1420 + Rust dev 窗口
cd src-tauri && cargo test --lib   # Rust 单元测试（安全）
pnpm build            # 前端类型检查 + 构建
pnpm tauri build      # 发布构建
```

> 注意：`cargo test` 全量会真实下载数百 MB（`download_snapshot_full`），日常验证用 `cargo test --lib`。

## 结构

- `src/` — Vue 3 前端（6 个页面 + 模块级 `reactive()` 状态单例）
- `src-tauri/` — Rust 后端：`commands/` 命令胶水层、`task/` 任务管理器与下载编排、`config/launcher/` 启动构建/环境/离线皮肤、`auth/` 微软登录与皮肤 API、`downloader/` 下载引擎、`persist/` TOML 配置

详见 [AGENTS.md](AGENTS.md)（仓库开发指南）。

## 运行时

- Windows（窗口 840×540、无边框透明）
- 配置存于 exe 同级 `Elytra/Settings.toml`
