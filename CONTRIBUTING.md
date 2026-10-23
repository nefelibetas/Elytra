# 贡献指南

感谢有兴趣参与 Elytra Launcher！提交代码前请先阅读本指南；架构分层与编码约定见 [AGENTS.md](AGENTS.md)。

## 开发环境

- **工具链**：版本以 [`mise.toml`](mise.toml) 为唯一来源（node / pnpm / rust / protoc），执行 `mise install` 安装
- **包管理器**：只支持 pnpm（`pnpm-lock.yaml` 入库，CI 一律 `pnpm install --frozen-lockfile`）
- **libclang**：编译联机功能（EasyTier 的 kcp feature）需要 LLVM——安装后设置 `LIBCLANG_PATH`
- 其他 Tauri v2 系统依赖见 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## 常用命令

见 [README](README.md) 的「开发」一节。提交前请确保以下检查全部通过：

```bash
cd src-tauri
cargo fmt                                # 改 Rust 后必跑，统一全仓格式
cargo check
cargo clippy --lib                       # 保持零警告
cargo test --lib -- --test-threads=1     # 必须串行（并行会因共享配置与网络抖动偶发失败）

pnpm build                               # vue-tsc 类型检查 + CSS 兼容性检查
pnpm i18n:check                          # 语言包校验（缺 key / 占位符不一致 / 裸中文）
```

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

## 发布新版本

发布由 [`.github/workflows/release.yml`](.github/workflows/release.yml) 自动完成：

1. 同步三处版本号（`src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` / `package.json`），须与 tag 号一致
2. 推送 tag：`git tag v1.0.0 && git push origin v1.0.0`
3. 工作流依次执行：验证门（vue-tsc + cargo check + clippy + test）→ 三平台并行构建 → 全部通过后把草稿 Release 转正；任一步失败则停在草稿、不对外露出

同版本重跑：`git tag -f v1.0.0 <新提交> && git push -f origin v1.0.0`。

构建期密钥（CurseForge API Key / 微软 Client ID）走 GitHub Secrets；本地构建读仓库根 `.env`，细节见 AGENTS.md。
