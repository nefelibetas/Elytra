# ElytraLauncher

基于 **Tauri v2（Rust）+ Vue 3（TypeScript）** 的 Minecraft 启动器。

## 功能

- 游戏版本下载：官方源 / 国内镜像多源回退，sha1 校验，并发与限速
- ModLoader：Forge / Fabric / NeoForge / Quilt / OptiFine / LiteLoader 版本查询与安装
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
- `src-tauri/` — Rust 后端（`commands/` 薄胶水层 + `downloader/` 下载引擎 + `auth/` 微软登录 + `persist/` TOML 配置）

详见 [AGENTS.md](AGENTS.md)（仓库开发指南）。

## 运行时

- Windows（窗口 840×540、无边框透明）
- 配置存于 exe 同级 `Elytra/Settings.toml`
