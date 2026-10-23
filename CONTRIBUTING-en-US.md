# Contributing

**English** · [简体中文](CONTRIBUTING.md)

Thanks for your interest in Elytra Launcher! Read this guide before submitting code. Architecture layering and coding conventions are in [AGENTS.md](AGENTS.md).

## Development environment

- **Toolchain (recommended)**: we recommend [mise](https://mise.jdx.dev/) with [`mise.toml`](mise.toml) (node / rust / protoc); CI reads the same file, which reduces local/CI version drift. mise is not mandatory, and you can install the versions manually.
- **Not using mise**: we recommend Node.js 24 and pnpm 12 (`npm install -g pnpm@12` or Corepack); these match the `mise.toml` baseline and reduce local/CI drift.
- **Package manager (pnpm required)**: the repo scripts, `pnpm-lock.yaml`, and CI are all pinned to pnpm, for three main reasons:
  - the content-addressable store plus hard links/global store reduces duplicate disk usage
  - the strict `node_modules` layout surfaces undeclared dependencies earlier
  - npm / yarn may produce a different dependency tree from CI's `--frozen-lockfile` install
- **libclang**: the project depends on EasyTier, which is compiled in normal builds; kcp-sys bindgen requires LLVM — set `LIBCLANG_PATH` after installing it.
- **7-Zip**: EasyTier's build needs `7z` on PATH — install 7-Zip on Windows, `p7zip-full` on Linux, or `p7zip` on macOS.
- **Tauri system dependencies**: see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
- **Optional build-time config**: copy `.env.example` to `.env` (gitignored) and fill in the CurseForge API key as needed; the Microsoft OAuth client ID is a built-in public constant and needs no configuration. Leaving it empty does not affect compilation; only the CurseForge source is disabled.

## Common commands

See the Development section in [README-en-US.md](README-en-US.md). Before submitting, make sure the following checks pass:

```bash
cd src-tauri
cargo fmt                                # apply formatting (CI checks with --check)
cargo check
cargo clippy --lib                       # keep zero warnings
cargo test --lib -- --test-threads=1     # must run serially (shared config + network flakiness)

cd ..
npx vue-tsc --noEmit                     # frontend type check
pnpm lint:css                            # CSS compatibility check
pnpm i18n:check                          # locale validation (missing keys / placeholder mismatch / raw Chinese)
```

CI (`.github/workflows/ci.yml`) runs the same set of checks on pull requests and pushes to `main` / `develop`, including `cargo fmt --check`.

Integration tests: `cargo test --test online_diag` (host/guest loopback with real EasyTier) and `cargo test --test tool_download` (local HTTP server, no internet required).

## Branches and commits

- Branch names: `feature/xxx` or `fix/xxx`
- Commit messages: follow [Conventional Commits](https://www.conventionalcommits.org/)
- Keep changes small and focused; open an issue to discuss larger changes first

## Development notes

- **TS types**: after changing Rust data structures, regenerate the frontend types (`cd src-tauri && cargo run --bin gen_ts --features gen-ts`) and commit the generated files under `src/types/`.
- **Config directory**: dev / test processes store config next to the executable (`Elytra/`), isolated from a normal install.
- **PCL data**: `src-tauri/src/data/wiki_entries.txt` is an unmodified snapshot of PCL's `WikiEntries.txt` (line number = MCMod entry id). Updating it requires syncing the commit and version in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) and rerunning `cargo test --lib wiki_entry`.
- **Unit tests live next to the implementation** (each module has its own `#[cfg(test)] mod tests`); shared test helpers go in the tested module's `pub(super)` area.
- **Security issues**: do not open a public issue; see [SECURITY.md](SECURITY.md).

## Releasing

Releases are automated by [`.github/workflows/release.yml`](.github/workflows/release.yml):

1. Sync the version in three places (`src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` / `package.json`); it must match the tag.
2. Push the tag: `git tag v1.0.0 && git push origin v1.0.0`
3. The workflow runs the verification gate (vue-tsc + lint:css + i18n + fmt + clippy + test; clippy already covers compilation), builds all three platforms in parallel, and publishes the draft release only if everything passes; any failure leaves the release in draft.
4. Release notes: if `docs/release-notes/<tag>.md` exists, the publish job uses it; otherwise the generated notes are kept.

To rerun the same version: `git tag -f v1.0.0 <new-commit> && git push -f origin v1.0.0`.

The build-time key (CurseForge API key) comes from GitHub Secrets; the Microsoft client ID is a built-in public constant and needs no configuration. Local builds read `.env` from the repository root; see `.env.example` for the template.
