# ElytraLauncher — Tauri Minecraft 启动器

## 架构

Tauri v2（Rust 后端 + Vue 3 前端）。前端管理状态，Rust 无状态只处理系统操作。

```
src-tauri/src/
├── greet/              ← 示例模块
│   └── mod.rs          ← #[tauri::command] 放这里
├── jdk/                ← JDK 检测模块
│   ├── mod.rs          ← Tauri command 导出层
│   ├── model.rs        ← JavaEnv, JavaConfig
│   ├── scanner.rs      ← 扫描 JAVA_HOME / 自定义目录
│   ├── storage.rs      ← 读写 .elytra/jdk.json
│   └── error.rs        ← JdkError 枚举 (含 From<io::Error>)
├── lib.rs              ← mod 声明 + generate_handler![]
└── main.rs             ← lib::run()
src/
├── App.vue             ← 根组件
└── main.ts
```

## 关键约定

### 模块模式
- 每个功能 = 一个目录模块，`mod.rs` 放 `#[tauri::command]` 函数（薄胶水层），核心逻辑放独立文件
- `generate_handler![]` 中路径必须与 `#[tauri::command]` 所在模块完全一致

### Tauri command
- 全部同步（`fn` 非 `async fn`）
- 返回 `Result<T, String>`，Rust 端错误在模块边界 `.map_err(|e| e.to_string())` 展平

### JDK 模块数据流
- 前端全权管理 config 状态（读写文件由前端编排）
- 4 个 command：`scan_jdk` / `scan_custom_jdk` / `get_jdk_config` / `save_jdk_config`
- 前端通过 `get_jdk_config` 启动时加载一次，`save_jdk_config` 持久化
- `JdkError` 实现了 `From<io::Error>`，`?` 可直接用于 io 操作

### 配置存储
- 存放在 exe 同级 `.elytra/jdk.json`（非 OS 标准配置目录）
- 路径通过 `std::env::current_exe()` 定位

## 开发命令

```bash
pnpm tauri dev          # 启动 Tauri 开发窗口
pnpm tauri build       # 构建发布包
cd src-tauri && cargo check   # Rust 编译检查
cd src-tauri && cargo clippy  # Rust 代码检查
pnpm build              # 前端类型检查 + 构建（vue-tsc + vite）
```

## 注意事项

- 包管理器必须用 **pnpm**（`beforeDevCommand` 中硬编码了 `pnpm`）
- `generate_handler!` 中路径必须精确匹配 command 函数所在模块（re-export 不能代替）
- 前端 TypeScript interface 需手动同步 Rust struct（无自动生成）
