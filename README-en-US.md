<div align="center">

<img src="docs/icon.svg" width="128" height="128" alt="Elytra" />

# Elytra Launcher

### 🪽 Spread your wings, take flight

—— Every trip into the blocky world, as light as a glide ✨

<sub>🪟 Windows · 🐧 Linux &nbsp;|&nbsp; powered by Tauri v2 & Vue 3</sub>

<sub>**Free** · **Open source** · **Cross-platform**</sub>

[简体中文](README.md) · **English**

</div>

---

## ✨ Features

- 📦 Version download: official / China-mirror multi-source fallback, sha1 verification, concurrency & rate limiting
- 🧩 ModLoader: install Fabric / Quilt / Forge / NeoForge / OptiFine, LiteLoader version query
- 🔐 Accounts: Microsoft login (OAuth PKCE), offline mode
- 🎭 Offline skin: fixed random-hash Steve/Alex (aligned with PCL2, only below 1.19.2)
- ☕ Automatic JDK scanning and custom configuration
- 🚀 Multiple game directories, download task management, launch argument building & game process management

## 🛠️ Development

```bash
pnpm install          # install deps (pnpm required)
pnpm tauri dev        # dev mode: vite :1420 + Rust dev window
cd src-tauri && cargo test --lib   # Rust unit tests (safe)
pnpm build            # frontend type-check + build
pnpm tauri build      # release build
```

> ⚠️ Note: a full `cargo test` downloads hundreds of MB for real (`download_snapshot_full`); use `cargo test --lib` for everyday verification.

## 📁 Structure

- 🖼️ `src/` — Vue 3 frontend (6 pages + module-level `reactive()` state singletons)
- 🦀 `src-tauri/` — Rust backend: `commands/` command glue layer, `task/` task manager & download orchestration, `config/launcher/` launch build/env/offline skin, `auth/` Microsoft login & skin API, `downloader/` download engine, `persist/` TOML config
- 📚 `docs/` — design docs
- 📄 `LICENSE` — MIT license

See 📖 [AGENTS.md](AGENTS.md) (repository development guide).

## 💻 Runtime

- 🪟 Windows (840×540 window, borderless & transparent)
- ⚙️ Config stored in `Elytra/Settings.toml` next to the executable

## 📄 License

Released under the [MIT](LICENSE) license.
