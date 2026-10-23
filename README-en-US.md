<div align="center">

<img src="docs/icon.svg" width="128" height="128" alt="Elytra" />

# Elytra Launcher

### 🪽 Spread your elytra, take off

—— Downloads, modpacks, LAN play — all in one go

<sub>🪟 Windows · 🐧 Linux · 🍎 macOS &nbsp;|&nbsp; powered by Tauri v2 & Vue 3</sub>

<sub>**Free** · **Open source** · **Cross-platform**</sub>

[![release](https://img.shields.io/github/v/release/nefelibetas/Elytra?style=flat-square)](https://github.com/nefelibetas/Elytra/releases)
[![license](https://img.shields.io/badge/license-GPL--3.0-blue?style=flat-square)](LICENSE)
[![stars](https://img.shields.io/github/stars/nefelibetas/Elytra?style=flat-square)](https://github.com/nefelibetas/Elytra)

[简体中文](README.md) · **English**

</div>

---

## 🖼️ Preview

<p align="center">
  <img src="docs/screenshots/01-launch.png" width="840" alt="Launch" />
  <br /><sub><b>Launch</b> · account, version card & launch entry</sub>
</p>

<p align="center">
  <img src="docs/screenshots/02-download.png" width="840" alt="Resources" />
  <br /><sub><b>Resources</b> · vanilla game versions plus community content (Mods / Modpacks / Datapacks / Resource Packs / Shaders / Worlds)</sub>
</p>

<p align="center">
  <img src="docs/screenshots/03-version-setup.png" width="840" alt="Version Settings" />
  <br /><sub><b>Version Settings</b> · mod management & update check</sub>
</p>

<p align="center">
  <img src="docs/screenshots/04-skin-wardrobe.png" width="840" alt="Wardrobe" />
  <br /><sub><b>Wardrobe</b> · 3D full-body preview, skin & cape management (genuine accounts only)</sub>
</p>

<p align="center">
  <img src="docs/screenshots/05-skin-painter.png" width="840" alt="Skin Painter" />
  <br /><sub><b>Skin Painter</b> · pixel-exact unfolded view + 3D direct painting (Settings → Tools)</sub>
</p>

<p align="center">
  <img src="docs/screenshots/06-online.png" width="840" alt="Multiplayer" />
  <br /><sub><b>Multiplayer</b> · room-code pairing from the Launch page sidebar</sub>
</p>

## ✨ Features

### 📦 Download & Install

- 🎮 **Game versions**: official / China-mirror multi-source fallback (Auto · Official-only · Mirror-only), sha1 verification, concurrent downloads & rate limiting
- 🧩 **ModLoaders**: one-click install for Fabric / Quilt / Forge / NeoForge / OptiFine; vanilla-only OptiFine is merged into a self-contained version
- 🧰 **Missing-file repair**: scan a version install and re-download whatever is missing
- ☕ **JDK management**: local JDK scan + managed runtime auto-download (Mojang → Azul fallback)
- 📥 **Content Hub**: search & install mods / modpacks / datapacks / resource packs / shaders / worlds (Modrinth · CurseForge); modpacks can also be imported locally (`.mrpack` / `.zip`); a custom CurseForge key can override the built-in key in Settings
- 🕸️ **Mod dependency resolution**: dependency chain preview with one-click install per entry

### 🗂️ Content Management

- ⚙️ **Version Setup**: mods (update check · bulk enable/disable), worlds, resource packs, shader packs, datapacks, server list, icon, game logs
- 📤 **Export**: launch script (`.bat` / `.sh`) and modpack (MRPACK)
- 📁 **Multiple game directories**: manage and switch between separate game folders
- 🤖 **Auto tasks**: run on launch / after exit — world backup (keep N copies), mod list recording, crash log packing (with redaction); global or per-instance config
- 👀 **Directory watching**: version and resource folders refresh automatically

### 🔐 Accounts & Appearance

- 🔐 **Accounts**: Microsoft login (OAuth PKCE, multi-account), offline mode, third-party skin sites (Yggdrasil-compatible: LittleSkin / self-hosted)
- 🎭 **Skin wardrobe**: 3D full-body preview, official skin & cape switching, skin upload, local skin collection
- 🎨 **Skin painter**: 64×64 pixel-exact unfolded view **and 3D direct painting** modes, brush / eraser / fill / eyedropper, free zoom & pan, per-part colour-coded guides, inner/outer layer switch, undo/redo, live 3D preview

### 🌐 LAN Play & Toolbox

- 🌐 **LAN play**: embedded EasyTier virtual network (room-code pairing, no public server needed), configurable coordination server for cross-network play
- 🚀 **Multi-threaded downloader**: URL queue concurrency + per-file HTTP Range segmentation, resumable
- 📡 **Server status**: Java SLP and Bedrock RakNet probing (multiple addresses)
- 🧪 **Generators**: MOTD, item NBT, entity NBT, commands (with target-selector builder), inline firework / potion / villager-trade editors

### 🎛️ Interface

- 🎨 Four built-in themes (Grass · End · Nether · Ice) + custom theme editor
- 🌐 Chinese & English UI: follow the system or switch manually, applied instantly at runtime (including task/step text and error messages)
- ⌨️ Full keyboard navigation & focus management, borderless self-drawn window

## 📥 Download & Install

Grab the artifact for your platform from [Releases](https://github.com/nefelibetas/Elytra/releases):

| Platform | Artifact | Notes |
|---|---|---|
| 🪟 Windows | `-setup.exe` (NSIS) · `.msi` · portable `.zip` | Install location is selectable (NSIS defaults to `%LOCALAPPDATA%`, MSI to `Program Files`); the zip runs as-is |
| 🍎 macOS | universal `.dmg` | Requires macOS 13+; ad-hoc signed, **right-click → Open** on first launch |
| 🐧 Linux | `.AppImage` · `.deb` · `.rpm` · portable `.tar.gz` | Built on Ubuntu 22.04 (glibc 2.35+) |

### Uninstall & data

| Form | Uninstall | Launcher config (`Elytra/`) |
|---|---|---|
| 🪟 NSIS install | bundled uninstaller | Removed only when "Delete the application data" is checked (unchecked by default); lives in `%APPDATA%\Elytra` |
| 🪟 MSI install | bundled uninstaller | **Kept** — delete `%APPDATA%\Elytra` manually |
| 🪟 portable zip | delete the folder | Delete `%APPDATA%\Elytra` manually |
| 🍎 macOS | delete `Elytra Launcher.app` | Delete `~/Library/Application Support/Elytra` manually |
| 🐧 deb / rpm | your package manager | **Kept** — delete `~/.local/share/Elytra` manually |
| 🐧 AppImage / tar.gz | delete file / folder | Delete `~/.local/share/Elytra` manually |

- All Windows forms (installer or portable) **share one config** in `%APPDATA%\Elytra`
- **Uninstalling never removes game data**: install directories are removed non-recursively (non-empty folders survive), and game folders you point elsewhere are never touched

## 🚀 Quick Start

1. **Set up a game directory** — versions, mods and worlds all live in the game directory (`.minecraft`). Windows installs default to a folder next to the launcher; if none exists (MSI installs into the read-only `Program Files`, macOS, Linux), add one from the Version Select page
2. **Download the game** — open Content Hub → Vanilla and pick a version; the expanded details also let you add ModLoaders (Fabric / Quilt / Forge / NeoForge / OptiFine). Click "Start download" and track it in the Task Center
3. **Sign in (optional)** — switch between Microsoft / offline / third-party accounts at the top of the launch page; offline mode uses a fixed random Steve/Alex skin and supports single-player
4. **Launch the game** — select the version on the launch page and click "Launch". If no Java runtime is found, open Settings → Launch settings → Java runtime to scan for local JDKs or install the managed runtime in one click

> Mods, modpacks, resource packs, shaders and worlds are all searchable in the Content Hub; the Toolbox offers LAN play, server status, NBT / command generators, the skin painter and a multi-threaded downloader.

## 🛠️ Development

> Toolchain versions come solely from `mise.toml` (node / pnpm / rust / protoc); building the LAN feature (EasyTier) also needs libclang (LLVM). pnpm is the only supported package manager.
>
> Not using mise? Install Node.js 24 and pnpm 12 (`npm install -g pnpm@12` or Corepack); Rust / protoc versions follow `mise.toml`.

```bash
mise install          # install toolchain from mise.toml
pnpm install          # install frontend deps
pnpm tauri dev        # dev mode: vite :1420 + Rust dev window (restart for Rust changes)
pnpm build            # frontend type-check + CSS compatibility check + build
pnpm i18n:check       # validate locale files (missing keys / placeholder mismatch / raw Chinese)

cd src-tauri
cargo check                                        # compile check
cargo clippy --lib                                 # lint (zero warnings)
cargo test --lib -- --test-threads=1               # unit tests (must run serially)
cargo test --test online_diag                      # LAN integration test (two instances, loopback)
cargo test --test tool_download                    # toolbox downloader integration test (local HTTP server)
```

> Optional build-time config: copy `.env.example` to `.env` (gitignored) and fill in the CurseForge API key as needed (it can also be overridden under Settings → Downloads → CurseForge key); the Microsoft OAuth client ID is a built-in public constant and needs no configuration. Leaving it empty does not affect compilation; only the CurseForge source is disabled.

> ℹ️ A few network tests (`fetch_asset_index_end_to_end`, `count_assets_all_end_to_end`, `global_client_uses_system_proxy_when_enabled`) hit the real internet with small single requests; they fail or skip offline. `bmclapi_optifine_real_download` is marked `#[ignore]` and does not run by default.

## 📁 Structure

### 🖼️ Frontend `src/`

- **`pages/`** — 9 pages (launch / content hub / tasks / settings / version select / version setup / resource detail / skin wardrobe / theme editor)
- **`component/`** — components grouped by domain (`common/` `download/` `launch/` `settings/` `tools/` `version/` `version-setup/`)
- **`store/`** — module-level `reactive()` state singletons, split by domain (`app/` `config/` `task/` `auth/` `community/` `skin/`)
- **`utils/`** — pure helpers (formatting / keyboard focus / toasts)
- **`types/`** — TS types generated from Rust (do not edit by hand)

### 🦀 Backend `src-tauri/src/`

- **`commands/`** — command glue layer: 137 `#[tauri::command]`s, argument flattening and domain calls only
- **`downloader/`** — pure transport layer: multi-source fetch / concurrent batch / probing / segmented download / path locking
- **`game/`** — content & assembly: version metadata / resources / ModLoader install / community / modpacks / mod updates
- **`task/`** — task orchestration & event dispatch, SQLite snapshots & crash recovery
- **`tooldl/`** — toolbox multi-threaded downloader
- **`launcher/`** — launch argument building / env prep / process lifecycle / auto tasks
- **`auth/`** — Microsoft OAuth / official APIs / third-party skin sites
- **`jdk/`** — local JDK scan & managed runtime download
- **`online/`** — EasyTier LAN play
- **`persist/`** — TOML config & secrets
- **`util/`** — cross-domain pure helpers

### 📚 Others

- **`docs/`** — icon & preview images
- **`AGENTS.md`** — architecture layering & coding conventions
- **`LICENSE`** — GPL-3.0-only

Dependency direction: `commands` → domain layers (`game/` `task/` `launcher/` …) → `downloader/` (pure transport, depends on nothing above).

See 📖 [AGENTS.md](AGENTS.md) (repository development guide).

## 💻 Runtime

- 🪟 Windows · 🐧 Linux · 🍎 macOS 13+ (1008×648 window, borderless & transparent, not resizable)
- ⚙️ Config directory (`Settings.toml` / `secrets.bin` / `tasks.db` / logs):
  - Windows: `%APPDATA%\Elytra`
  - macOS: `~/Library/Application Support/Elytra`
  - Linux: `$XDG_DATA_HOME/Elytra` (defaults to `~/.local/share/Elytra`)
- 📁 Game directory: defaults next to the launcher on Windows (portable, move the whole folder to migrate); specify manually when the install directory is read-only (e.g. MSI into `Program Files`) or on macOS / Linux
- 🧪 Dev / test processes keep their config next to the executable (`Elytra/`), isolated from a normal install

## 🔒 Privacy & Network

- **No telemetry**: no analytics SDKs; usage data, crash reports, and account information are never uploaded to the author's servers.
- **Services contacted**: Microsoft / Mojang (login, profile, skins, version metadata, assets), BMCLAPI / MCIM (China mirrors), Modrinth / CurseForge (community resources and dependencies), Azul / Adoptium (JDK runtimes), the EasyTier coordination node (LAN play only), user-configured third-party skin sites (only when enabled), and user-entered server addresses (server status only).
- **Local data**: `secrets.bin` stores Microsoft refresh tokens, third-party skin-site credentials, and the user-provided CurseForge key — Windows uses DPAPI; macOS / Linux prefer the OS keychain plus ChaCha20-Poly1305, and fall back to plaintext protected only by file permissions when the keychain is unavailable. `Settings.toml` is plaintext config (game directory, download settings, etc.); `tasks.db` stores task snapshots; normal logs may contain local paths and request URLs.
- **Never uploaded automatically**: game directories, saves, mods, skins, and logs are not uploaded; crash-log bundles are only created locally when the user runs an auto task.
- **Third-party services**: the services above are governed by their own privacy policies; this project does not control their data handling.

## ⚠️ Known Limitations

| Scenario | Limitation |
|---|---|
| Fabric/Quilt + OptiFine | Requires the OptiFabric bridge and has limited compatibility; the combination is hidden with an error on 1.21+. Sodium + Iris, or vanilla/Forge + OptiFine, is recommended. |
| Cross-network LAN play | Depends on an EasyTier coordination server (configurable); offline accounts on MC 26.2 must use LAN discovery, and a manually entered IP may report an invalid session. |
| macOS | Not notarized; first launch requires right-click → Open. macOS 13+. |
| Windows uninstall | MSI does not remove `%APPDATA%\Elytra`; NSIS keeps app data by default; neither removes the game directory. |
| Windows portable build | Requires the system WebView2 runtime (usually present on Windows 10/11). |
| Linux | Built on Ubuntu 22.04 (glibc 2.35+); the portable build depends on the system WebKitGTK. |
| Skin wardrobe | Only available for genuine Microsoft accounts; offline mode uses a fixed default skin. |
| CurseForge | Requires a valid CurseForge API key; if the built-in key expires, enter your own key under Settings → Downloads → CurseForge key. The source is unavailable when no key is configured, while Modrinth keeps working. |
| OptiFine | No public API; some versions/combinations may fail to download or install. |

## 🤝 Contributing & Feedback

- **Bug reports / feature requests**: open an [issue](https://github.com/nefelibetas/Elytra/issues), including your launcher log ("Settings → Launcher log" opens its folder), app version and reproduction steps; issue templates are available
- **Code contributions**: start with [CONTRIBUTING-en-US.md](CONTRIBUTING-en-US.md) ([简体中文](CONTRIBUTING.md)) — dev environment, required checks before submitting, branch & commit conventions
- **Security issues**: do not open a public issue; see [SECURITY.md](SECURITY.md)
- Architecture layering and coding conventions: see [AGENTS.md](AGENTS.md)

## 🙏 Thanks

- [Tauri](https://tauri.app/) · [Vue 3](https://vuejs.org/) · [Vite](https://vite.dev/) — app framework & toolchain
- [EasyTier](https://github.com/EasyTier/EasyTier) — virtual network layer behind LAN play
- [Lucide](https://lucide.dev/) — icons
- [BMCLAPI](https://bmclapi2.bangbang93.com/) — China download mirror
- [MCIM](https://mod.mcimirror.top/) — Modrinth / CurseForge mirror & translation data
- [Modrinth](https://modrinth.com/) · [CurseForge](https://www.curseforge.com/minecraft) · [MCMod.cn](https://www.mcmod.cn/) — community resources & entry data
- [Mojang](https://www.minecraft.net/) · [Azul](https://www.azul.com/downloads/) — game metadata & JDK runtimes
- Fabric · Quilt · Forge · NeoForge — ModLoader Maven metadata
- [HMCL](https://github.com/HMCL-dev/HMCL) — fellow open-source launcher; our GPL-3.0-only license aligns with it
- [PCL2](https://github.com/Meloong-Git/PCL) — UI layout reference; the Chinese content search embeds its `WikiEntries.txt` snapshot (see [third-party notices](THIRD-PARTY-NOTICES.md))

## ⚖️ Disclaimer

Elytra is a third-party open-source project, not affiliated with, endorsed by, or associated with Mojang Studios or Microsoft.
"Minecraft" and related names and trademarks belong to Mojang Studios.
This project only provides tooling for downloading and managing game content; it does **not** bundle any game files.

## 📄 License

Released under the [GPL-3.0-only](LICENSE) license. Copyright (C) 2026 BridgeFishDev.

Bundled third-party content and data (the PCL WikiEntries snapshot, ModLoader icons, Minecraft assets and default skins) are covered separately in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
