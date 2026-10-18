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
- 🧩 ModLoader: install Fabric / Quilt / Forge / NeoForge / OptiFine
- 🔐 Accounts: Microsoft login (OAuth PKCE), offline mode
- 🎭 Offline skin: fixed random-hash Steve/Alex
- ☕ Automatic JDK scanning and custom configuration
- 🎨 Skin painter (toolbox): 64×64 pixel-exact unfolded view **and 3D direct painting** modes, brush / eraser / fill / eyedropper, free zoom & pan, per-part colour-coded guides, inner/outer layer switch, undo/redo, live 3D preview
- 🚀 Multiple game directories, download task management, launch argument building & game process management
- 📥 Toolbox: multi-threaded downloader (URL queue concurrency + per-file HTTP Range segmentation, resumable)

## 🛠️ Development

```bash
pnpm install          # install deps (pnpm required)
pnpm tauri dev        # dev mode: vite :1420 + Rust dev window
cd src-tauri && cargo test --lib   # Rust unit tests (safe)
pnpm build            # frontend type-check + build
pnpm tauri build      # release build
```

> ℹ️ A few network tests (`fetch_asset_index_end_to_end`, `count_assets_all_end_to_end`, `global_client_uses_system_proxy_when_enabled`) hit the real internet with small single requests; they fail or skip offline. `bmclapi_optifine_real_download` is marked `#[ignore]` and does not run by default.

## 📁 Structure

- 🖼️ `src/` — Vue 3 frontend (10 pages + module-level `reactive()` state singletons)
- 🦀 `src-tauri/` — Rust backend: `commands/` command glue layer, `task/` task manager & download orchestration, `downloader/` pure transport layer (fetch/batch/probe/path-lock), `game/` content & assembly (version metadata, resources, ModLoader, community, modpacks, mod updates), `launcher/` launch build & env prep, `auth/` Microsoft login & skin API, `jdk/` JDK scan & managed runtime download, `online/` EasyTier LAN, `persist/` TOML config & secrets
- 📚 `docs/` — icon assets and `code-audit.md` (code issue ledger)
- 📄 `LICENSE` — GPL-3.0 license

See 📖 [AGENTS.md](AGENTS.md) (repository development guide).

## 💻 Runtime

- 🪟 Windows (1008×648 window, borderless & transparent)
- ⚙️ Config stored in `Elytra/Settings.toml` next to the executable

## 📄 License

Released under the [GPL-3.0](LICENSE) license.
