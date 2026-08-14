# 世界地图渲染与区块编辑（不进游戏）— 实施方案

> 状态：已调研、已定方案，**排期未定**。落地时按阶段拆 PR。
> 上游：对话调研（2026-08）+ librarian 源码级核实 + 官方文档/仓库。

## 1. 背景与目标

ElytraLauncher 需要在**不启动游戏**的情况下，直接读取 Minecraft 存档（Java 版）实现：

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
| **1.21.5+** | **mca crate（VilleOlof）** | 唯一支持新压缩；组合方案：mca 读 region 解压 → fastanvil 解析/渲染 |

### 2.3 候选库

| crate | 定位 | 结论 |
|---|---|---|
| fastanvil 0.32（owengage） | Region + JavaChunk + **渲染**（`RenderedPalette` 官方颜色表 + `TopShadeRenderer`） | 解析/渲染层主选；Region 层仅用于 ≤1.21.4 |
| fastnbt 2.x | NBT serde | 底层 |
| mca 2.1（VilleOlof） | Region 读写，**支持 1.21.5+ 自定义压缩** | region 层主选（全版本） |
| minedmap-resource | 方块→颜色函数（biome 着色） | 颜色逻辑参考（MIT） |
| MinedMap（CLI） | 完整渲染器（1.8–26.1，增量） | CLI 路线替代，**不采用**（二进制分发成本） |

注意：fastnbt-tools 的 `anvil` 二进制 = owengage 同仓库的 CLI，2023-04 停更，不采用。

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

## 4. 功能规格

### 4.1 2D 地图预览（top-down + Y 切片）
- 拖拽平移、滚轮缩放（瓦片金字塔或按需渲染）
- Y 滑杆：切换水平切片层（按需渲染模式，渲染器自写——fastanvil 的 `TopShadeRenderer` 是 top-down 专用）
- 生物群系着色、光照/夜间层（P1）

### 4.2 3D 渲染（three.js）
- 后端面片化 → 分 chunk 几何文件/raw IPC → three.js 场景
- OrbitControls（旋转/缩放/平移免费）；MVP 顶点色，纹理图集为 P1（需从游戏资源提取）
- 流式加载：视口移动动态加载/卸载 chunk；透明方块（水/玻璃）材质排序

### 4.3 区块复制
- 框选区域 → 方块坐标 → chunk/region 坐标 → 读源 chunk NBT 整块拷贝 → 写目标 region
- **必做校验**：源/目标维度一致（DataVersion、世界高度、维度类型）；写前目标备份 + 确认流
- 区域跨 region 边界 → 多 region 编排；目标已有 chunk → 覆盖语义

## 5. 阶段划分（排期后按此拆 PR）

| 阶段 | 内容 | 依赖 | 验收 |
|---|---|---|---|
| **A** | 2D 渲染器：top-down + y 切片 + 拖拽查看器（mca 组合方案） | 无 | 任意 1.21.5+ 存档可预览，拖拽流畅 |
| **B** | 区块复制：框选 → 校验 → 写目标 | A 的解析层 | 复制后目标游戏可正常加载 |
| **C1** | 3D 原型验证：单 chunk 面片化 → raw IPC → three.js | A | 性能数字达标（见 §6） |
| **C2** | 完整 3D：流式视口 + 面片化完善 + LOD | C1 | 视口 5×5 chunk 可流畅漫游 |

P1（后置）：存档统计、玩家数据、备份/空洞整理、纹理图集、夜间层。

## 6. 性能目标（C1 原型实测）

| 指标 | 目标 |
|---|---|
| 面片化后单 chunk 几何 | 100–300KB |
| 视口 25 chunk 全量刷新 | < 500ms（解析+传输+组装） |
| 静止视角帧率（25–50 万面） | 60fps；拖拽 30fps+ |

超标降级路径（不改架构）：几何写文件走 asset 通道 + 更激进 LOD。

## 7. 风险与对策

1. **真 3D 工作量低估**——C1 原型先验证再投 C2；2.5D 等距（Mapcrafter 式）为降级中间形态
2. **1.21.5+ 压缩**——阶段 A 起就用 mca 组合，不用 fastanvil 的 Region 层
3. **写操作损坏数据**——区块复制是唯一写用户数据的功能：备份 + 维度校验 + 确认流
4. **按需渲染请求风暴**——拖拽/滑杆连续事件合并防抖，后端按视口批量渲染
5. **fastanvil 颜色表**——`RenderedPalette` 从官方 palette.tar.gz 内置加载，不需要游戏客户端

## 8. 参考来源

- 存档格式：Minecraft Wiki（Anvil / NBT / chunk format）
- 库：docs.rs/fastanvil、crates.io/mca、github.com/owengage/fastnbt（monorepo：fastnbt/fastsnbt/fastanvil/tools）
- 参考实现：github.com/neocturne/MinedMap（Rust 渲染器，MIT，1.8–26.1）、owengage.com/anvil（WASM 浏览器渲染演示）、unmined.net、mapcrafter、overviewer.org
