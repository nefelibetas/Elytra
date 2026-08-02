# ElytraLauncher — Tauri v2 Minecraft 启动器

## 架构

```
src-tauri/src/
├── commands/         ← #[tauri::command] 薄胶水层
│   ├── jdk.rs        ← 4 个 JDK 命令
│   ├── downloader.rs ← 版本列表 / 下载 / 配置
│   ├── loader.rs     ← Forge/Fabric/Quilt/NeoForge/OptiFine/LiteLoader 版本查询
│   ├── installer.rs  ← Fabric/Forge/NeoForge/OptiFine 安装
│   └── task.rs       ← 任务生命周期（start/cancel/list/remove）
├── config/           ← 配置模型（无状态，纯数据）
│   ├── jdk/          ← JavaEnv, JavaConfig, scanner, storage
│   ├── downloader/   ← DownloadSource, DownloaderConfig（含 fallback 下载逻辑）
│   └── launcher/     ← LauncherConfig, GcType, AuthMode, MemoryMode
├── downloader/       ← Minecraft 下载引擎
│   ├── manifest.rs   ← 版本清单获取
│   ├── version.rs    ← 版本 JSON / client.jar 下载
│   ├── library.rs    ← 依赖库下载
│   ├── assets.rs     ← 资源文件下载
│   ├── installer.rs  ← ModLoader 安装器
│   ├── fetcher.rs    ← HTTP 客户端
│   ├── hash.rs       ← SHA1 校验
│   └── model.rs      ← VersionJson, Library, AssetIndex 等
├── persist/          ← TOML 持久化（read<T>/save<T> 按 section 存取）
├── task/             ← 全局 TaskManager（进度跟踪、取消、清理）
├── lib.rs            ← mod 声明 + generate_handler![]
└── main.rs           ← lib::run()
src/
├── main.ts           ← createApp + router
├── App.vue           ← 根布局（TitleBar + Transition）
├── router/index.ts   ← 4 路由：/ /download /settings /download-detail
├── store/download.ts ← 下载任务状态（reactive）+ Tauri event 监听
├── pages/            ← Launch / Download / Settings / DownloadDetail
└── component/        ← TitleBar / DownloadFAB / DownloadTab / VersionCard / VersionItem
```

## 关键约定

### 模块模式
- `commands/` 下每个文件只放 `#[tauri::command]` 函数（薄胶水层），核心逻辑放其他模块
- `generate_handler!` 中路径必须与 `#[tauri::command]` 所在模块完全一致（re-export 不能代替）
- 前端 TypeScript interface 需手动同步 Rust struct（无自动生成）

### Tauri command
- **同步和异步混用**：JDK 命令同步，downloader/loader/installer/task 命令全为 `async fn`
- 返回 `Result<T, String>`，错误在模块边界展平为 String

### 配置存储
- 单文件 `Elytra/Settings.toml`（exe 同级），TOML 格式，按 key section 存取
- `persist::read<T>("downloader")` / `persist::save("downloader", &config)` 读写指定 section
- 前端负责编排读写时机，后端无状态

### 下载引擎
- 支持多数据源（官方 + 国内镜像），`active_source` 索引决定主源，`fetch_with_fallback`/`fetch_json_with_fallback` 自动回退
- BMCLAPI 源自动 URL 重写（`mirror_url`）
- 任务通过 `TaskManager` 全局单例管理，`task-progress` event 推送进度到前端

## 开发命令

```bash
pnpm tauri dev                     # 启动 Tauri 开发窗口（前端 :1420 + Rust）
pnpm tauri build                   # 构建发布包
cd src-tauri && cargo check        # Rust 编译检查
cd src-tauri && cargo clippy       # Rust 代码检查
cd src-tauri && cargo test         # 运行 Rust 测试
pnpm build                         # 前端类型检查 + 构建（vue-tsc --noEmit + vite build）
pnpm dev                           # 仅启动 vite 前端 dev server（不含 Tauri）
```

## 注意事项
- 包管理器必须用 **pnpm**（`beforeDevCommand` 硬编码）
- 窗口无边框透明（`decorations: false, transparent: true`），800×520，不可调整大小
- Rust 测试在 `src-tauri/tests/downloader_test.rs`，需要有效的 `minecraft_dir` 配置
- `capabilities/default.json` 控制 Tauri v2 权限（window 拖拽/最小化/关闭 + opener + dialog）
- 前端路由使用 `createWebHashHistory`（hash 路由）
- 下载任务基于 `tokio::spawn` + event 推送，非 command 返回
