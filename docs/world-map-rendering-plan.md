# 世界地图渲染与区块编辑（不进游戏）— 实施方案

> 状态：已调研、已定方案，**排期未定**。落地时按阶段拆 PR。
> 上游：对话调研（2026-08）+ librarian 源码级核实 + 官方文档/仓库；2026-09-04 复核全部依赖版本（docs.rs / GitHub / npm）并补全技术栈与落地细节。

## 1. 背景与目标

Elytra Launcher 需要在**不启动游戏**的情况下，直接读取 Minecraft 存档（Java 版）实现：

- **P0 地图预览**：可拖拽缩放的 2D 地图，支持输入 Y 查看水平切片（某层方块分布）
- **P0 2D/3D 完整渲染**：不进游戏即可查看游戏场景（3D 以 2.5D 等距为跳板，真 3D 用 three.js）
- **P0 区块复制**：自选区域，复制 chunk/region 到另一个地图
- 其余（存档统计、玩家数据、备份优化等）为 P1，排期后置

## 2. 技术调研结论

### 2.1 存档格式（读写基础）

```
世界目录
├── level.dat           世界元数据（种子在 Data.WorldGenSettings.seed，1.16+）
├── region/*.mca        Anvil 格式：512×512 方块 = 32×32 chunk
│   ├── 8 KiB 文件头（位置表 1024×4B + 时间戳表 1024×4B，扇区单位）
│   └── 数据区（4 KiB 扇区，chunk 数据 = 4B 长度 + 1B 压缩类型 + NBT 数据）
├── entities/、DIM-1/（下界）、DIM1/（末地）等
```

- chunk = 16×16 方块柱（1.18+ 高度 384，24 个 section）；section = 16×16×16
- chunk NBT 1.13+：palette + 打包位字段索引（写比读麻烦：palette 扩容触发整 section 重打包）
- **1.21.5+（2025-03）region 新增自定义压缩（LZ4/Zstd）**——压缩层变化，chunk NBT 结构本身变化小

### 2.2 版本支持矩阵（关键）

| 版本 | 解析层 | 说明 |
|---|---|---|
| 1.2 – 1.12 | fastanvil `Pre13` | 旧区块结构 |
| 1.13 – 1.17 | fastanvil `Pre18` | 平坦化后 |
| 1.18 – 1.21.4 | fastanvil `Post18` | 现代结构，主流 |
| **1.21.5+（含 26.x）** | **mca crate（VilleOlof）** | 组合方案：mca 读 region 解压 → fastanvil 解析/渲染。注意 fastanvil 0.32 已内置 `lz4-java-wrc` 解码，并非“只有 mca 能解新压缩”；选 mca 的真正理由见 §2.3 |

### 2.3 候选库

| crate / 包 | 定位 | 结论 |
|---|---|---|
| fastanvil 0.32.0（owengage） | Region + JavaChunk + **渲染**（`RenderedPalette` 官方颜色表 + `TopShadeRenderer`）；`render` feature 默认开，传递引入 image 0.24（作者已关 default-features，仅核心 buffer）+ tar + serde_json | 解析/渲染层主选；Region 层不用（统一走 mca）。**`JavaChunk` 是 lossy 的（实体丢失，官方明言不适合序列化回 region）——仅用于渲染读取，写路径禁用** |
| fastnbt 2.6.1 | NBT serde + `Value` 完整树 + stream parser | 底层；**渲染走 serde，复制走 `Value`（不丢字段）** |
| mca 2.1（VilleOlof） | Region 读写；GZip/Zlib/Raw/LZ4 内置 + `CustomCompression` trait 兜底任意 type-127 自定义算法；`into_writer()` 增量写（未动 chunk 原字节保留）；写路径默认并行（rayon，可关） | region 层主选（全版本一套代码）；`into_writer` 正好服务区块复制的目标 region 增量写 |
| png 0.17 | RGBA → PNG 瓦片编码 | 瓦片落盘定 PNG（无损 + alpha，前端 `<img>` 直读零处理） |
| three 0.185.1 + @types/three 0.185.4 | 前端 3D 视口；OrbitControls 经 `three/addons/controls/OrbitControls.js`（包内 ESM exports，vite 直用） | 3D 渲染；@types/three 声明 TS ≥5.6，引入后以 `npx vue-tsc --noEmit` 验证 |
| lru | chunk 几何/瓦片内存 LRU | 可选（也可手写淘汰）；`memmap2`（region mmap 随机读）C2 再议 |
| minedmap-resource | 方块→颜色函数（biome 着色） | 颜色逻辑参考（MIT） |
| MinedMap（CLI） | 完整渲染器（1.8–26.1，增量） | CLI 路线替代，**不采用**（二进制分发成本） |

注意：fastnbt-tools 的 `anvil` 二进制 = owengage 同仓库的 CLI，2023-04 停更，不采用。

### 2.4 依赖引入清单（按约定需确认后引入，版本核实于 2026-09-04）

- **新增 Rust crate（5）**：`mca 2.1`、`fastnbt 2.6`、`fastanvil 0.32`、`png 0.17`、`flate2 1.0`（fastanvil 传递已有，3D 几何 gzip 直接声明复用）
- **新增前端包（2）**：`three 0.185`、`@types/three 0.185`
- **可选**：`lru`（缓存）；`memmap2`（C2 阶段再议）
- **明确不引入**：`simdnbt` / `na_nbt`（mca README 推荐搭档）——NBT 库必须统一 fastnbt，否则 fastanvil 的 chunk 类型不可用；`rayon`（mca default feature，用 `default-features = false` 关掉，并发走项目统一的有界信号量惯例）
- **零新依赖**：3D 几何解压用浏览器原生 `DecompressionStream('gzip')`（WebView2 / WKWebView 16.4+ / WebKitGTK 2.40+ 均支持）；2D 拖拽查看器 Canvas 2D 自写
- **26.x 支持声明**：矩阵止于 1.21.5+，MC 现已进入 26.x 时代；MinedMap 支持 1.8–26.1 说明 region 格式未再大变，但 mca 对 26.x 未实测——阶段 A 用真实 26.x 存档验证，压缩兜底走 `CustomCompression` 回调
- **无更轻量整体替代**：自写 region 解析 + 渲染为千行级且需跟随版本格式演进，此组合为 Rust 生态现成覆盖最全方案
- **两条 NBT 通道（技术栈咬合核心）**：

```
.mca ── mca 2.1（region 层，全压缩 + 自定义回调）──▶ chunk NBT bytes
        │
        ├─ fastnbt serde ──▶ fastanvil JavaChunk（Pre13/Pre18/Post18）【渲染通道，lossy 可接受】
        │     ├─ 2D: RenderedPalette + TopShadeRenderer（palette.tar.gz 构建期 include_bytes! 嵌入）/ 自写 y 切片
        │     │      → RGBA → png → 瓦片 PNG → asset 协议 → <img>/Canvas2D
        │     └─ 3D: 自写面片化 → 量化顶点 → flate2 gzip → raw IPC → three.js + DecompressionStream
        │
        └─ fastnbt Value（原始树，坐标平移）→ mca into_writer/set_chunk → 目标 region【复制通道，不许丢字段】
```

## 3. 总体架构

```
Rust 后端 map/ 域模块                     WebView 前端
─────────────────────                    ─────────────────
region 层（mca：读+写）
  → chunk 解析（fastanvil/mca+fastnbt）
  → 渲染器：top-down / y切片 / 等距      地图查看器组件
  → 面片化器（3D）：可见面剔除+合并        Canvas 拖拽缩放 + Y 滑杆
  → LRU 缓存（chunk 几何/瓦片）             + 区域框选 + 写操作确认流
        │ 静态瓦片 → 写文件                    │
        │ convertFileSrc（asset 协议）  ───▶  <img>/Canvas 拼接
        │ 动态几何 → raw IPC                   │
        │ invoke → Vec<u8> → ArrayBuffer ───▶  three.js 加载
```

**传输双通道（核心决策）**：

| 数据 | 通道 | 理由 |
|---|---|---|
| 静态瓦片（2D top-down / 2.5D 等距） | asset 文件协议（`convertFileSrc`） | 一次生成多次复用，文件缓存天然契合 |
| 动态几何（3D 视口流式） | IPC raw bytes（`tauri::ipc::Response`，前端 ArrayBuffer） | 避开磁盘 IO 与文件协议 native 边界；每 chunk 小 payload 并行无压力 |

**禁止**：图像/几何走 IPC JSON（serde 序列化，几 MB 即卡）。

### 数据量控制链（传输瓶颈的治本措施）

```
chunk 原始数据（几万方块）
  → 面片化：可见面剔除 + 相邻面合并        → 1-2 万面
  → 顶点色（不送 UV/纹理）                 → ~300KB/chunk
  → 量化 + gzip                            → ~100KB/chunk
  → LOD：远 chunk 降采样                    → 视口外再降
  → 流式：只送视口内 chunk，出视口即弃      → 同时在线 ~25 chunk ≈ 2.5MB 压缩
```

### 项目落地约定

**asset 协议配置（必须补）**：当前 `tauri.conf.json` 无 `app.security.assetProtocol` 配置，且其 scope 为**静态白名单**，无法覆盖用户任意存档目录——瓦片因此**不写存档目录**，写应用缓存 `util::config_dir()/mapcache/`，scope 静态声明该目录（conf 层配置，非 capabilities）。顺带解决缓存污染存档；失效键 = 世界路径 + region mtime + Y 层 + 渲染样式，目录上限与清理策略随阶段 A 定。

- **命令层**：`commands/map.rs` 新模块；snake_case 动词开头；`Result<T, String>` + 中文短文案错误（`\n` + 技术细节）；`lib.rs` generate_handler! 全限定路径注册
- **任务整合**：全图渲染 / 批量瓦片生成走 task/ TaskManager（可取消 + task-progress 推送）；写目标 region 纳入 PathLock
- **并发与内存**：解析/面片化走 `spawn_blocking` + 有界信号量（2–4 线程，同镜像并发惯例）；chunk LRU 设容量上限（阶段 A 定数值），防大存档拖垮常驻内存
- **路径安全**：invoke 传入的世界目录必须校验（`level.dat` 存在、拒绝 `..` 逃逸），防任意路径读写

## 4. 功能规格

### 4.0 入口与世界选择（新增）
- 入口：版本/游戏目录相关页面（版本列表或下载页侧栏，排期时定）；复用 gamedirs 已配置目录
- 世界列表：扫描 saves/*/，读 level.dat（LevelName、LastPlayed）+ icon.png 缩略图
- 维度切换：主世界 / DIM-1 / DIM1，region 路径随维度切换（`region/`、`DIM-1/region/`、`DIM1/region/`）

### 4.1 2D 地图预览（top-down + Y 切片）
- 拖拽平移、滚轮缩放（瓦片金字塔或按需渲染）
- Y 滑杆：切换水平切片层（按需渲染模式，渲染器自写——fastanvil 的 `TopShadeRenderer` 是 top-down 专用）
- 窗口约束：主窗口 840×540 固定不可拉伸，工具栏（Y 滑杆/缩放/坐标显示/框选入口）按单屏紧凑布局设计
- 生物群系着色、光照/夜间层（P1）

### 4.2 3D 渲染（three.js）
- 后端面片化 → 分 chunk 几何文件/raw IPC → three.js 场景
- OrbitControls（旋转/缩放/平移免费）；MVP 顶点色，纹理图集为 P1（需从游戏资源提取）
- 流式加载：视口移动动态加载/卸载 chunk；透明方块（水/玻璃）材质排序

### 4.3 区块复制
- 框选区域 → 方块坐标 → chunk/region 坐标 → 读源 chunk **原始 NBT（fastnbt `Value`）** → 坐标平移 → 写目标 region（mca `into_writer`/`set_chunk`）
- **坐标平移（核心难点，“整块拷贝”不成立）**：chunk NBT 内含世界绝对坐标字段——`block_entities`（箱子/刷怪笼等 x/y/z）、实体 Position、`ScheduledTicks` 等；复制到不同偏移必须按 `(srcChunk - dstChunk)` 遍历改写，否则容器错位、实体堆叠
- **实体文件**：1.17+ 实体独立存于 `entities/*.mca`（与 region/ 平级）；复制编排必须同时处理两套 .mca，只复制 region 会丢全部实体
- **必做校验**：源/目标维度一致（DataVersion 相等、世界高度、维度类型）；目标存档未被游戏占用（检测到本启动器已启动该实例时阻断并提示——游戏运行中写 region 会真损坏）
- **事务性**：先全量备份目标（region + entities，备份至应用备份目录并保留 N 份），再两阶段写入；多 region 复制中途失败可整体回滚
- 区域跨 region 边界 → 多 region 编排；目标已有 chunk → 覆盖语义（确认流明示覆盖数量）

## 5. 阶段划分（排期后按此拆 PR）

| 阶段 | 内容 | 依赖 | 验收 |
|---|---|---|---|
| **A** | 2D 渲染器：top-down + y 切片 + 拖拽查看器（mca 组合方案） | 无 | 任意 1.21.5+/26.x 存档可预览，拖拽流畅；真实 26.x 存档压缩解析通过 |
| **B** | 区块复制：框选 → 校验 → 写目标 | A 的解析层 | 复制后目标游戏可正常加载 |
| **C1** | 3D 原型验证：单 chunk 面片化 → raw IPC → three.js | A | 性能数字达标（见 §6） |
| **C2** | 完整 3D：流式视口 + 面片化完善 + LOD | C1 | 视口 5×5 chunk 可流畅漫游 |

**2.5D 等距定位**：不设独立阶段。仅当 C1 性能验证失败，或 Linux WebKitGTK WebGL 降级软件渲染（发行版滞后/无 GPU）不可接受时，作为降级交付（Canvas 2D 实现，不依赖 WebGL）。

P1（后置）：存档统计、玩家数据、备份/空洞整理、纹理图集、夜间层。

**测试策略（延续项目单测风格，全部离线、可串行）**：
- 合成 region fixture：程序化构造 chunk NBT（fastnbt 序列化）→ 写 .mca → 解析断言 roundtrip；覆盖 Pre18 / Post18 / 1.21.5+ LZ4 三种布局
- 复制 roundtrip：写目标 → 读回 → 与预期 `Value` 树 diff（含坐标平移断言、entities 一并复制断言）
- 渲染纯函数：颜色映射、y 切片选择、面片化计数直接断言

## 6. 性能目标（C1 原型实测）

| 指标 | 目标 |
|---|---|
| 面片化后单 chunk 几何 | 100–300KB |
| 视口 25 chunk 全量刷新 | < 500ms（解析+传输+组装） |
| 静止视角帧率（25–50 万面） | 60fps；拖拽 30fps+ |
| 内存上限 | chunk LRU + 解析峰值设硬上限（阶段 A 定数值），大存档不拖垮常驻内存 |
| 渲染并发 | 解析/面片化 2–4 线程（有界信号量），不阻塞 UI 线程与下载任务 |

超标降级路径（不改架构）：几何写文件走 asset 通道 + 更激进 LOD。

## 7. 风险与对策

1. **真 3D 工作量低估**——C1 原型先验证再投 C2；2.5D 等距（Mapcrafter 式）为降级中间形态（定位见 §5）
2. **1.21.5+/26.x 压缩**——阶段 A 起就用 mca 组合（全版本统一 region 层 + `CustomCompression` 兜底），不用 fastanvil 的 Region 层
3. **写操作损坏数据**——区块复制是唯一写用户数据的功能：备份 + 维度校验 + 确认流 + **游戏运行互斥** + 两阶段写入可回滚
4. **按需渲染请求风暴**——拖拽/滑杆连续事件合并防抖，后端按视口批量渲染
5. **fastanvil 颜色表**——`RenderedPalette` 从官方 palette.tar.gz 构建期嵌入（include_bytes!），运行时零网络、不需要游戏客户端
6. **Linux WebGL 性能**——WebKitGTK 发行版滞后/无 GPU 时 WebGL 降级软件渲染（llvmpipe），3D 不可用则降级 2.5D（Canvas 2D）；三端一致性约定从 CSS 扩展到 JS API（`DecompressionStream` 已确认 WebKitGTK 2.40+ 可用）
7. **mca 对 26.x 未实测**——阶段 A 用真实 26.x 存档验证；遇到未识别压缩走 `CustomCompression` 回调兜底
8. **@types/three 与 ts 6 兼容**——类型包声明 TS ≥5.6；引入当天跑 `npx vue-tsc --noEmit` 验证
9. **大存档首开体验**——纯视口按需渲染首屏逐块浮现；批量预热/全图渲染走 TaskManager 后台任务（可取消 + 进度推送），缓存命中后秒开

## 8. 参考来源

- 存档格式：Minecraft Wiki（Anvil / NBT / chunk format）
- 库：docs.rs/fastanvil（0.32.0）、docs.rs/mca（2.1）、docs.rs/fastnbt（2.6.1）、github.com/owengage/fastnbt（monorepo：fastnbt/fastanvil）、github.com/VilleOlof/mca、npm：three / @types/three（0.185.x）
- 参考实现：github.com/neocturne/MinedMap（Rust 渲染器，MIT，1.8–26.1）、owengage.com/anvil（WASM 浏览器渲染演示）、unmined.net、mapcrafter、overviewer.org
