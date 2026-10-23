# 贡献指南

**简体中文** · [English](CONTRIBUTING-en-US.md)

感谢有兴趣参与 Elytra Launcher！提交代码前请先阅读本指南；架构分层与编码约定见 [AGENTS.md](AGENTS.md)。

## 开发环境

- **工具链（推荐）**：推荐用 [mise](https://mise.jdx.dev/) 按 [`mise.toml`](mise.toml) 安装（node / rust / protoc）；CI 读同一份文件，能减少本地与 CI 的版本差异。mise 不是强制项，也可手动安装对应版本
- **不使用 mise**：推荐安装 Node.js 24 与 pnpm 12（`npm install -g pnpm@12` 或 Corepack）；这些版本与 `mise.toml` 的基准一致，可减少本地与 CI 的差异
- **包管理器（必须 pnpm）**：仓库脚本、`pnpm-lock.yaml` 与 CI 都按 pnpm 固定；主要有三个原因：
  - 内容寻址存储 + 硬链接/全局 store，减少重复磁盘占用
  - 严格的 `node_modules` 结构能更早暴露未声明依赖
  - 换用 npm / yarn 可能生成不同依赖树，与 CI 的 `--frozen-lockfile` 不一致
- **libclang**：项目依赖 EasyTier，正常构建就会编译它；kcp-sys 的 bindgen 需要 LLVM——安装后设置 `LIBCLANG_PATH`
- **7-Zip**：EasyTier 构建需要 `7z` 在 PATH——Windows 安装 7-Zip，Linux 安装 `p7zip-full`，macOS 安装 `p7zip`
- **构建期可选配置**：复制 `.env.example` 为 `.env`（已被 gitignore），按需填写 CurseForge API Key；微软 OAuth Client ID 已内置公开常量，无需配置。留空不影响编译，仅 CurseForge 来源不可用
- 其他 Tauri v2 系统依赖见 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## 常用命令

见 [README](README.md) 的「开发」一节。提交前请确保以下检查全部通过：

```bash
cd src-tauri
cargo fmt                                # 改 Rust 后必跑，统一全仓格式（CI 用 --check）
cargo check
cargo clippy --lib                       # 保持零警告
cargo test --lib -- --test-threads=1     # 必须串行（并行会因共享配置与网络抖动偶发失败）

cd ..
npx vue-tsc --noEmit                     # 前端类型检查
pnpm lint:css                            # CSS 兼容性检查
pnpm i18n:check                          # 语言包校验（缺 key / 占位符不一致 / 裸中文）
```

CI（`.github/workflows/ci.yml`）在 PR 与 `main` / `develop` 推送时跑同一组检查，包含 `cargo fmt --check`。

集成测试：`cargo test --test online_diag`（host/guest 双实例回环，走真实 EasyTier）、`cargo test --test tool_download`（本地 HTTP 服务驱动，无需外网）。

## 分支与提交

- 分支命名：`feature/xxx` 或 `fix/xxx`
- 提交信息：遵循 [Conventional Commits](https://www.conventionalcommits.org/)
- 改动尽量小而聚焦；较大的改动建议先开 Issue 讨论

## 开发注意

- **TS 类型**：修改 Rust 数据结构后必须重新生成前端类型（`cd src-tauri && cargo run --bin gen_ts --features gen-ts`），产物 `src/types/` 入库
- **配置目录**：dev / 测试进程的配置写在 exe 旁 `Elytra/`（与正式安装隔离），正式安装写在平台用户数据目录
- **PCL 数据**：`src-tauri/src/data/wiki_entries.txt` 是 PCL `WikiEntries.txt` 的未修改快照（行号 = MC 百科条目 id）；更新它必须同步 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) 的 commit 与版本，并重跑 `cargo test --lib wiki_entry`
- **单元测试与实现同文件**（各模块内嵌 `#[cfg(test)] mod tests`），跨模块共用的测试辅助放被测模块的 `pub(super)` 区
- **安全问题**：请勿公开提交，报告方式见 [SECURITY.md](SECURITY.md)

## 发布新版本

发布由 [`.github/workflows/release.yml`](.github/workflows/release.yml) 自动完成：

1. 同步三处版本号（`src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` / `package.json`），须与 tag 号一致
2. 推送 tag：`git tag v1.0.0 && git push origin v1.0.0`
3. 工作流依次执行：验证门（vue-tsc + lint:css + i18n + fmt + clippy + test；clippy 已含编译检查）→ 三平台并行构建 → 全部通过后把草稿 Release 转正；任一步失败则停在草稿、不对外露出
4. Release 正文：`docs/release-notes/<tag>.md` 存在时由 publish 阶段覆盖为人工正文；不存在则保留 `--generate-notes` 自动生成

同版本重跑：`git tag -f v1.0.0 <新提交> && git push -f origin v1.0.0`。

构建期密钥（CurseForge API Key）走 GitHub Secrets；微软 Client ID 已内置公开常量，无需配置。本地构建读仓库根 `.env`（模板见 `.env.example`），未配置时 CurseForge 来源不可用。
