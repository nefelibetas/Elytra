<div align="center">

<img src="docs/icon.svg" width="128" height="128" alt="Elytra" />

# Elytra Launcher

### 🪽 展开鞘翅，轻盈启程

—— 让每一次踏入方块世界，都如振翅滑翔般轻快 ✨

<sub>🪟 Windows · 🐧 Linux &nbsp;|&nbsp; 由 Tauri v2 与 Vue 3 驱动</sub>

<sub>**免费** · **开源** · **跨平台**</sub>

**简体中文** · [English](README-en-US.md)

</div>

---

## ✨ 功能

- 📦 游戏版本下载：官方源 / 国内镜像多源回退，sha1 校验，并发与限速
- 🧩 ModLoader：Fabric / Quilt / Forge / NeoForge / OptiFine 安装
- 🔐 账号：微软正版登录（OAuth PKCE）、离线模式
- 🎭 离线皮肤：固定随机哈希 Steve/Alex
- 🌐 局域网联机：EasyTier 虚拟局域网（房间码互连，无需公网服务器），跨网可配协调服务器

## 🛠️ 开发

```bash
pnpm install          # 安装依赖（必须用 pnpm）
pnpm tauri dev        # 开发模式：vite :1420 + Rust dev 窗口
cd src-tauri && cargo test --lib   # Rust 单元测试（安全）
pnpm build            # 前端类型检查 + 构建
pnpm tauri build      # 发布构建
```

> ℹ️ 少数网络测试（`fetch_asset_index_end_to_end`、`count_assets_all_end_to_end`、`global_client_uses_system_proxy_when_enabled`）走真实外网单发小请求，离线环境会失败或跳过；`bmclapi_optifine_real_download` 标了 `#[ignore]` 默认不跑。

## 📁 结构

- 🖼️ `src/` — Vue 3 前端（9 个页面 + 模块级 `reactive()` 状态单例）
- 🦀 `src-tauri/` — Rust 后端：`commands/` 命令胶水层、`task/` 任务管理与下载编排、`downloader/` 纯传输层（fetch/批处理/探测/路径互斥）、`game/` 内容与装配（版本元数据/资源/ModLoader/社区资源/整合包/Mod 更新）、`launcher/` 启动构建与环境准备、`auth/` 微软登录与皮肤 API、`jdk/` JDK 扫描与 managed runtime 下载、`online/` EasyTier 局域网联机、`persist/` TOML 配置与密钥存储
- 📚 `docs/` — 图标资源与 `code-audit.md`（代码问题台账与治理记录）
- 📄 `LICENSE` — MIT 许可证

详见 📖 [AGENTS.md](AGENTS.md)（仓库开发指南）。

## 💻 运行时

- 🪟 Windows（窗口 840×540、无边框透明）
- ⚙️ 配置存于 exe 同级 `Elytra/Settings.toml`

## 📄 许可证

基于 [MIT](LICENSE) 许可证开源。
