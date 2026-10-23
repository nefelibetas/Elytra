# Elytra i18n 设计规划

> 状态：已实施（`zh-CN` + `en-US`），已通过类型检查、前端构建、Rust 单测/clippy 与浏览器双语冒烟。
> 本文只覆盖「启动器界面与启动器自身产生的运行时文本」；游戏内语言、第三方资源描述、代码注释与开发日志不在范围内。

## 0. 结论摘要

| 决策 | 推荐方案 | 理由 |
|---|---|---|
| 前端 i18n 库 | `vue-i18n` v11（Composition API） | 事实标准；插值/复数/回退/日期数字格式齐全；桌面应用不敏感于约 30 KB 体积 |
| 消息文件 | `src/i18n/messages/{locale}/{domain}.ts` | 按域拆分，TS 模块可用 `satisfies` 做 key 对称的编译期校验 |
| 语言持久化 | `localStorage`（与主题一致） | 语言只影响渲染，无需 IPC；挂载前同步读取，避免首帧闪烁 |
| 语言选择 | `auto`（跟随系统）/ `zh-CN` / `en-US` | 默认 `auto`；`zh*` → `zh-CN`，其余 → `en-US` |
| 前端文案 | 全部走 `t()`；禁止裸中文 | 含模板文本、属性、选项常量、toast、状态标签 |
| 后端步骤名 | `StepInfo` 增加 `kind`，前端按枚举翻译 | 类型安全、改动小、无需字符串协议 |
| 后端任务名/阶段名 | 增加 `name_key` / `current_phase_key` + params | 语言切换对进行中任务也即时生效；旧中文任务名回退原文 |
| 后端错误 | `@i18n:` 首行协议 + 中文原文 fallback 行 | 不破坏 `Result<T, String>` 契约；可逐条迁移；前端未识别时仍可读 |
| 后端日志 | 保持中文，不翻译 | 诊断信息；翻译日志会破坏搜索与技术排障 |
| MC 数据表 | `GiveItem` 等增加 `nameEn`，缺省回退注册 id | 约 600 条物品/附魔名应使用官方英文名，而非人工直译 |

分阶段落地，先前端 UI、后后端文本；后端长尾错误允许渐进迁移，不阻塞首发英文版。

## 1. 目标与非目标

### 1.1 目标

1. 运行时切换 `zh-CN` / `en-US`，界面即时刷新，无需重启。
2. 切换结果持久化；`auto` 模式跟随系统语言。
3. 覆盖全部 9 个页面、公共组件、设置项、工具集、错误提示、任务进度。
4. 语言文件成为文案唯一事实源；缺 key、占位符不一致、裸中文可被 CI 拦截。
5. 不改变现有前后端契约的总体形态：命令仍返回 `Result<T, String>`，事件 payload 仍是序列化快照。

### 1.2 非目标（首期）

- 不引入翻译平台（Crowdin/Weblate）与在线热更新；语言包随应用打包。
- 不支持 RTL、不新增繁体/日语/俄语；语言代码按前缀回退。
- 不翻译代码注释、Rust 日志、第三方 API 返回的 Mod 简介/MOTD/版本 JSON 字段。
- 不翻译系统原生对话框（文件选择器按钮由 OS 决定）。
- 不改游戏启动参数中的语言（Minecraft 语言由游戏内设置决定）。

### 1.3 成功标准

- 英文模式下逐页无中文残留（白名单除外：注册 id、日志、外部数据）。
- 下载/安装/登录/启动四条主链路的步骤名、任务名、错误首行均为英文。
- 1008×648 固定窗口下英文布局不溢出、不截断关键按钮。
- `pnpm build`、`npx vue-tsc --noEmit`、`pnpm lint:css`、`cargo check --lib`、`cargo clippy --lib`、`cargo test --lib -- --test-threads=1` 全部通过。
- `scripts/i18n-check.mjs` 在 CI 与本地均通过。

## 2. 现状盘点

统计口径：`src/**/*.{vue,ts}` 与 `src-tauri/src/**/*.rs`，已排除 Rust 行注释与前端注释；数字为粗略量级，用于排期而非精确计数。

| 项目 | 数量 | 说明 |
|---|---|---|
| 前端 Vue/TS 文件 | 184 | 9 个页面 + 按域组件/store |
| 含中文的文件 | 170 | 几乎全量 |
| 模板文本行 | ~412 | 主要集中 `component/`（357） |
| 模板属性中文 | ~85 | `title` / `placeholder` / `aria-label` / `alt` |
| 脚本字符串字面量 | ~1042 | `component/` 203、`utils/` 638、`store/` 149、`pages/` 52 |
| Rust 非测试中文字面量 | ~912 | `game/` 250、`task/` 152、`auth/` 114、`commands/` 73、`jdk/` 70、`launcher/` 65、`online/` 50、`downloader/` 45 |
| Rust 测试中文字面量 | ~772 | 不参与翻译，但部分断言依赖步骤名/错误首行，迁移时需同步 |

现有事实：

- 无任何 i18n 依赖；`package.json` 只有 Vue、Vue Router、Tauri API 与图标库。
- `index.html` 固定 `lang="en"`，与默认中文界面不一致。
- 全局字体栈在 `App.vue`：`"Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", "Noto Sans SC", sans-serif`。
- 主题（纯 UI 偏好）已用 `localStorage` 持久化：`elytra.theme` / `elytra.customThemes`，可作为语言持久化的先例。
- 配置（下载源、JDK、启动参数等）走后端 TOML `persist`；语言不参与后端逻辑，暂不进入该体系。
- 错误契约：后端返回 `"中文短文案\n技术细节"`，前端 `errMsg()` 取首行，`AlertDialog` 显示全文。
- 任务事件：`TaskProgress { name, current_phase, steps[].name, error }` 中的文本由 Rust 生成；`download.ts` 原样存入 `TaskItem` 并渲染。
- 启动流程的 4 个步骤名在前端 `src/store/app/launch.ts` 中定义，可直接 `t()`。
- 自动任务的内置动作名称/描述在前端 `src/store/config/autoTasks.ts`；后端 `AutoTaskAction::kind_label()` 返回稳定英文 id。
- 无前端测试；前端质量门禁是 `vue-tsc` + stylelint。后端有 388 个单测与 loader/OptiFine 矩阵、e2e。

### 2.1 主要难点

1. **后端生成文本**：任务名、步骤名、阶段名、错误首行、崩溃恢复的任务类型描述都在 Rust 侧，直接改成前端 `t()` 需要协议或结构化字段。
2. **动态拼接**：大量 `` `${count} 个` ``、三元表达式文案、`label + "失败"` 形式，必须改成整句消息 + 插值。
3. **模块级常量**：选项数组（下载源/网络模式/JDK 源/社区源）、主题名、自动任务动作在模块加载时求值，切换语言不会更新；需改为 key 或 computed。
4. **数据表**：`utils/give.ts`（约 400 条物品、42 条附魔、13 条属性）、`utils/mcData.ts`（49 条游戏规则）等属于游戏术语，不能按普通 UI 文案直译。
5. **固定窗口**：1008×648、不可缩放；英文通常比中文长 30%–50%，按钮、侧栏、表头、步骤名最易溢出。
6. **错误契约**：`Result<T, String>` 遍布 134 个命令；不能为了 i18n 一次性改成结构化错误。

## 3. 总体架构

```mermaid
flowchart LR
  subgraph Frontend
    L[localStorage<br/>elytra.language] --> I[i18n/index.ts<br/>locale + fallback]
    M[messages/zh-CN<br/>messages/en-US] --> I
    I --> T[t() / $t()]
    T --> UI[页面 / 组件 / toast / 状态标签]
    T --> F[format.ts<br/>Intl 格式化]
  end
  subgraph Backend
    C[commands/*] -- Result<T,String> --> E[errMsg / formatError]
    P[TaskProgress<br/>name_key / step.kind / phase_key] --> TL[任务展示层]
    E --> T
    TL --> T
  end
```

核心原则：

- **展示层统一由前端翻译**。后端不持有语言状态，不读 locale，不加载语言包。
- **后端输出稳定标识**：步骤用枚举 `kind`，任务名/阶段名用 key + params，错误用 `@i18n:` 协议；未迁移的旧文本原样透传。
- **单一事实源**：所有翻译在 `src/i18n/messages/`；Rust 侧不维护第二份翻译表。
- **渐进兼容**：旧 SQLite 任务快照、旧错误字符串、旧事件 payload 都能被新前端正常显示（回退原文）。

## 4. 前端设计

### 4.1 目录结构

```
src/i18n/
  index.ts                 # createI18n、locale 状态、setLocale、检测、t 导出
  types.ts                 # MessageSchema（typeof zh-CN）
  messages/
    zh-CN/
      common.ts            # 通用按钮、状态、单位、确认/取消、空状态
      nav.ts               # 标题栏、侧栏、页面标题
      launch.ts            # 启动页、账号、版本卡片、启动进度
      download.ts          # 资源中心、社区搜索、资源详情
      task.ts              # 任务中心、下载状态、等待决策
      settings.ts          # 设置各 tab、主题、自动任务
      tools.ts             # 工具箱、生成器、下载器、联机
      skin.ts              # 皮肤衣柜、绘画器
      errors.ts            # 前端通用错误 + 后端错误 key
    en-US/                 # 同构，key 必须与 zh-CN 完全一致
```

- 用 `.ts` 而不是 `.json`：可直接写 `import type`、`satisfies`、注释，避免 JSON 转义；Vite 打包无额外配置。
- 不按组件拆文件，按页面/域拆，避免 100+ 个碎片文件。
- 首期不做按路由懒加载：全部消息约 100–200 KB，桌面应用可接受；懒加载会让切换语言与回退逻辑复杂化。

### 4.2 key 命名

格式：`<域>.<子域>.<元素>[.<变体>]`，全小写，单词用 `-` 或直接连写；禁止把整句中文当 key。

```
common.action.confirm
common.action.cancel
common.status.loading
nav.launch
launch.account.login
launch.version.repair
download.community.result-count
download.community.empty
settings.download.source.auto.label
settings.download.source.auto.desc
task.status.running
task.step.file-progress
tools.give.item-stack
errors.download.failed
backend.task.name.install
backend.task.phase.cancelled
backend.err.login.failed
```

约定：

- 同一语义只允许一个 key；跨页面复用放 `common`。
- 选项类用 `.label` / `.desc` 成对；状态类用 `.status.*`。
- 错误 key 以 `errors.` 开头；后端返回的 key 与前端本地错误共用命名空间。
- 复数消息只写一个 key，由 vue-i18n 的 `|` 分支处理。
- key 一旦发布视为 API：重命名必须保留旧 key 别名至少一个版本，或一次性全量替换（桌面端前后端同版本发布，可接受硬替换；但错误 key 会被旧快照引用，见 §5.3）。

### 4.3 消息写法

```ts
// zh-CN/task.ts
export default {
  status: {
    running: "进行中",
    done: "已完成",
    error: "失败",
    cancelled: "已取消",
  },
  resultCount: "共 {count} 个结果",
  eta: {
    seconds: "{count} 秒",
    minutes: "{m} 分 {s} 秒",
    hours: "{h} 时 {m} 分",
  },
};
```

```ts
// en-US/task.ts
export default {
  status: {
    running: "In progress",
    done: "Done",
    error: "Failed",
    cancelled: "Cancelled",
  },
  resultCount: "{count} result | {count} results",
  eta: {
    seconds: "{count}s",
    minutes: "{m}m {s}s",
    hours: "{h}h {m}m",
  },
};
```

规则：

1. **禁止句子拼接**：不要 `t("a") + t("b")` 或 `` `${t("prefix")} ${name}` ``；用完整消息 + `{name}`。
2. **语序可变**：中文 `共 {count} 个结果` → 英文 `{count} results`，参数位置由译文决定。
3. **复数**：英文用 `|` 分支；中文不需要复数时可只写一个形式。调用时传 `count`：`t("task.resultCount", { count }, count)`。
4. **特殊字符**：vue-i18n 中 `@` 是 linked message 前缀、`|` 是复数分隔符；文案含这些字符用 `{'@'}` / `{'|'}` 转义。Minecraft 命令、邮箱、`@e` 选择器尤其注意。
5. **富文本**：需要强调/链接时用 `<i18n-t>` 组件或分段 key，禁止 `v-html` 拼接翻译。
6. **不翻译的变量**：版本号、loader 名、文件名、URL、注册 id 一律作为参数原样插入。

### 4.4 类型安全与回退

```ts
// src/i18n/types.ts
import type zhCN from "./messages/zh-CN";

export type MessageSchema = typeof zhCN;
```

```ts
// src/i18n/messages/en-US/index.ts
import type { MessageSchema } from "../../types";
export default { /* ... */ } satisfies MessageSchema;
```

- `en-US` 的 key 集合与值类型由 TypeScript 强制对齐；缺 key 直接 `vue-tsc` 报错。
- 不引入 `@intlify/unplugin-vue-i18n`：构建期预编译收益有限，且增加 Vite 配置与版本兼容面；类型 + 脚本已覆盖主要风险。
- 运行时 `fallbackLocale: "zh-CN"`；开发环境保留 `missingWarn`，生产关闭。缺 key 时回退中文，不显示 raw key。
- 模板中用 `$t("key")`（全局注入）；脚本中用 `src/i18n/index.ts` 导出的 `t()`。两者都指向同一个 `i18n.global`。

### 4.5 语言状态与持久化

```ts
// src/i18n/index.ts（示意）
export type LanguageSetting = "auto" | "zh-CN" | "en-US";
const STORAGE_KEY = "elytra.language";

export function detectLanguage(): "zh-CN" | "en-US" {
  const nav = navigator.language.toLowerCase();
  return nav.startsWith("zh") ? "zh-CN" : "en-US";
}

export function resolveLanguage(setting: LanguageSetting) {
  return setting === "auto" ? detectLanguage() : setting;
}
```

- `main.ts` 在 `createApp` 之前调用 `initI18n()`，同步读取 localStorage，避免首帧中文/英文闪烁。
- `setLocale(setting)`：
  1. 写入 localStorage；
  2. 更新 `i18n.global.locale`；
  3. 设置 `document.documentElement.lang`；
  4. 更新 Tauri 窗口标题（`getCurrentWindow().setTitle(t("app.title"))`，品牌名可保持 `Elytra Launcher`）；
  5. 触发依赖 locale 的格式化函数重新计算（见 §4.6）。
- `auto` 只在启动时解析一次；系统语言在运行中变化不跟随（桌面端重启生效，简化实现）。
- 语言设置 UI 放「设置 → 外观」顶部，使用现有 `DropSelect`：
  - `auto`：跟随系统（当前解析为…）
  - `中文（简体）`
  - `English`
- 切换后给一条 toast（用新语言）确认，非必需。

### 4.6 格式化

`src/utils/format.ts` 改为读取当前 locale：

| 函数 | 处理 |
|---|---|
| `fmtCount` | `Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 })`；中文输出 `1.2万`，英文 `1.2K` |
| `fmtSize` / `fmtSpeed` / `fmtGb` | 单位 `KB/MB/GB` 语言中立，保留现有实现 |
| `fmtEta` | 走 `task.eta.*` 消息 key，按秒/分/时分支；`null` 返回 `—` |
| `fmtUnix` / `fmtDateTime` | 保留 `YYYY-MM-DD HH:mm` 的 ISO 风格（语言中立、行为不变）；如后续需要本地化，再切 `Intl.DateTimeFormat` |
| `fmtModDate` | 保留 ISO 日期 |
| 列表排序 | 字符串比较统一改用 `a.localeCompare(b, locale)`；检查版本列表、资源列表、Mod 列表 |

- 格式化函数不是纯函数了（依赖当前 locale）；调用点签名保持不变，内部读 `getResolvedLocale()`。
- 不要把 `toLocaleString()` 直接散落在组件里；统一收敛到 `format.ts`。
- 数字/日期格式的浏览器基线：Chrome 111 / Safari 16.4，`Intl.NumberFormat` 的 compact 与 `Intl.RelativeTimeFormat` 均可用。

### 4.7 模块级常量与选项

现状问题：`downloaderConfig.ts` 的 `sourceModeOptions`、`theme.ts` 的 `THEMES`、`autoTasks.ts` 的 `builtinCatalog` / `phases` 在模块加载时写死中文。

改法（统一模式）：

```ts
// 之前
export const sourceModeOptions = [
  { value: "auto", label: "智能模式", desc: "..." },
];

// 之后
export const sourceModeOptions = [
  { value: "auto", labelKey: "settings.download.source.auto.label", descKey: "settings.download.source.auto.desc" },
];
```

- 组件渲染时 `t(option.labelKey)`，语言切换自动更新。
- 需要按条件拼接的（如 `备份存档（全局，保留 3 份）`）改为整句消息 + 参数：
  `t("settings.auto-tasks.backup.summary", { scope: t(scopeKey), keep })`。
- 不要在模块顶层调用 `t()` 生成常量数组；模块只加载一次，locale 切换不会重算。

### 4.8 错误提示

前端本地错误直接 `t()`；后端错误经统一入口解析：

```ts
// src/utils/error.ts
export function errMsg(e: unknown): string;          // 兼容现有调用，返回摘要
export function formatError(e: unknown): { summary: string; detail: string };
```

- `errMsg` 保持现有签名，内部识别 `@i18n:` 首行并翻译；未识别按旧逻辑取首行。
- `AlertDialog` 改为使用 `formatError`：摘要用翻译后的首行，详情为技术细节（不含 fallback 原文行）。
- `showToast(errMsg(e))` 的调用点无需改动。
- 未翻译的后端错误继续显示中文原文；这是允许的中间态。

## 5. 后端文本设计

### 5.1 分层

| 文本类型 | 方案 | 优先级 |
|---|---|---|
| 任务步骤名 | `StepInfo` 增加 `kind` 枚举；`File` 步骤保留文件名 | P0（首期） |
| 任务名 | `TaskProgress` 增加 `name_key` + `name_params` | P1 |
| 崩溃恢复的任务类型 | `InterruptedJob.kind` / `JobSpec::kind_label()` 改为 key 或前端按任务类型映射 | P1 |
| 当前阶段 `current_phase` | 增加 `current_phase_key` + `current_phase_params` | P1 |
| 命令/任务错误 | `@i18n:` 协议 + 高频迁移 | P1（先 150 条） |
| 枚举状态 | 前端映射（任务/步骤/联机/JDK/资源分类） | P0 |
| 日志、内部诊断 | 保持中文 | 不迁 |
| 第三方数据 | 原样显示 | 不迁 |

### 5.2 步骤名：`StepInfo.kind`

当前 `StepInfo { name, status, progress }`，`name` 由 `task/steps.rs` 生成。改为：

```rust
#[derive(Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    VersionJson,
    ClientJar,
    Libraries,
    Assets,
    Natives,
    LoaderConfig,
    LoaderLibs,
    OptiFine,
    FabricApi,
    File, // name 为具体文件名，不翻译
}

pub struct StepInfo {
    pub name: String,              // 保留：旧快照 / 日志 / 回退
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<StepKind>,    // 新增
    pub status: StepStatus,
    pub progress: f64,
}
```

- 前端 `stepLabel(step)`：`kind` 存在且非 `File` → `t("backend.task.step." + kind)`；否则显示 `step.name`。
- `TaskProgress` 不落库（SQLite 快照只存 `JobSpec` 与任务名）；`Option` + `default` 主要兼容旧事件 payload 与旧前端，新任务都会带 `kind`。
- `task/steps.rs` 构造 `StepSpec` 时同时给 `kind`；`StepKind` 从内部枚举提升为可序列化类型（或保留内部枚举并做映射）。
- 任务名 `TaskSpec::name()` 保留，用于日志与快照名称回退；`InterruptedJob.kind` 来自 `JobSpec::kind_label()`，需一并 key 化。

### 5.3 任务名/阶段名

`TaskProgress` 增加可选字段（不改旧字段）：

```rust
pub struct TaskProgress {
    // ... 现有字段
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_params: Option<std::collections::BTreeMap<String, String>>,
    pub current_phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_phase_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_phase_params: Option<std::collections::BTreeMap<String, String>>,
}
```

前端：

```ts
function taskLabel(t: TaskItem) {
  return t.nameKey ? t(t.nameKey, t.nameParams) : t.name;
}
```

- key 示例：
  - `backend.task.name.download`：`{mc}` → `Download {mc}`
  - `backend.task.name.install`：`{mc}`, `{loader}`, `{version}`
  - `backend.task.phase.fetch-assets`、`backend.task.phase.scan-local`
  - `backend.task.phase.cancelled` → `Cancelled`
  - `backend.task.phase.files-progress`：`{done}`, `{total}`
- 参数值（版本号、loader 名、文件名）不翻译。
- 迁移点：`TaskSpec` 实现（`task/steps.rs`、`task/spawn.rs`、`task/modpack.rs`、`task/jdk.rs`、`task/repair.rs`）、`progress.rs` 的阶段赋值、`files.rs` 的 `已完成 i/n`。
- 兼容：快照只存 `JobSpec` 与保存时的 `name`，不存 `TaskProgress`；恢复后的任务由后端重新生成 key。旧快照的 `name` 仍可能是中文，恢复横幅直接显示（回退原文）；新任务即时跟随语言。

### 5.4 错误：`@i18n:` 协议

#### 格式

首行：

```
@i18n:<key>[?<k>=<v>&<k>=<v>...]
```

后续行：

```
<中文原文（fallback）>
<技术细节>
```

示例：

```
@i18n:errors.download.failed?name=Fabric%20API&status=404
下载 Fabric API 失败
HTTP 404
```

- key 只允许 `[a-z0-9._-]`；参数值 `encodeURIComponent`。
- 无参数时只写 key。
- 中文原文行可选；迁移期保留，便于前端缺 key 时降级，也便于中文日志阅读。
- 技术细节保持原样（可能是 reqwest/io/serde 的英文错误）。

#### 前端解析

```ts
const I18N_PREFIX = "@i18n:";

function parseBackendError(s: string) {
  const [first, ...rest] = s.split("\n");
  if (!first.startsWith(I18N_PREFIX)) return null;
  const raw = first.slice(I18N_PREFIX.length);
  const [key, query = ""] = raw.split("?");
  const params = Object.fromEntries(new URLSearchParams(query));
  const fallback = rest[0]?.trim() || key;
  return { key, params, fallback, detail: rest.slice(1).join("\n") };
}
```

- key 在消息表中存在 → `t(key, params)`。
- 不存在 → 用 `fallback` 行；仍无 → 显示 key（开发环境 `console.warn`）。
- `detail` 为剩余技术细节；`AlertDialog` 显示 `summary + "\n" + detail`。
- 旧格式（无前缀）完全走原逻辑。

#### 后端宏

在 `src-tauri/src/macros.rs` 增加：

```rust
// 生成 @i18n 首行 + 中文 fallback + 技术细节
bail_key!("errors.download.failed", ("name", name), ("status", status); "下载 {} 失败", name);
err_key_ctx!("errors.download.failed", ("name", name); "下载 {} 失败", name)
```

具体宏语法在实施时定稿；约束：

- 不改变函数返回类型（仍是 `Result<_, String>`）。
- 参数值统一 `urlencoding`（手写小函数即可，避免新依赖）。
- 迁移是逐调用点的，可与旧 `bail!` / `common_err_ctx!` 共存。
- 现有 `common_err_ctx!` 的「内联捕获必须走位置参数」坑沿用同一规则。

#### 迁移优先级

| 批次 | 范围 | 覆盖场景 |
|---|---|---|
| 1 | `commands/downloader.rs`、`task/runner.rs`、`task/files.rs` | 下载/安装失败、磁盘/网络错误 |
| 2 | `auth/*`、`commands/account.rs`、`commands/offline_account.rs` | 微软/第三方登录、皮肤/披风 |
| 3 | `game/loader/*`、`task/modpack.rs`、`task/jdk.rs` | ModLoader/整合包/JDK 安装 |
| 4 | `launcher/*`、`online/*`、`server_status` | 启动、联机、服务器状态 |
| 5 | 长尾 `game/community`、`downloader` 内部错误 | 按用户反馈补齐 |

目标：批次 1–4 覆盖高频路径的约 150–200 条首行；长尾保持中文并在 backlog 跟踪。技术细节永远不翻译。

### 5.5 枚举与状态

- `TaskStatus`、`StepStatus` 已是英文枚举，前端 `statusChip` 等映射改为 `t("task.status.*")`。
- 版本类型 `release/snapshot/old_beta/old_alpha`、资源分类、联机状态、JDK 来源等，前端统一映射表 `t()`。
- `LoaderKind::display_name()` 返回 `Fabric/Forge/NeoForge/Quilt`，语言中立，保留。
- `AutoTaskAction::kind_label()` 返回英文 id，前端已有 `actionKind` 映射；将 `builtinCatalog` / `phases` 的 label/desc 改为 key。

## 6. Minecraft 数据表与术语

### 6.1 数据表

`utils/give.ts` 的 `GIVE_ITEMS`（约 400 条）、`ENCHANTMENTS`（42 条）、`ATTRIBUTES`（13 条），以及 `utils/mcData.ts` 的 `GAMERULES`（49 条）不是普通 UI 文案：

- 物品/附魔/属性名是 Minecraft 官方术语，应使用官方语言文件，不应自由直译。
- 游戏规则的 `label` 是自定义描述（如「死亡保留物品」），需要人工翻译。
- 方案：给数据项增加 `nameEn` 字段；英文模式 `nameEn ?? id`；中文继续用现有 `name`。
- `nameEn` 来源：Mojang 官方 `en_us.json`（版本差异由 id 映射处理）；可写一次性脚本生成，不引入运行时网络依赖。
- 未覆盖项回退注册 id（如 `diamond_sword`），技术用户可读，且不会出现机翻。
- 不在 locale 文件中放 600+ 个 `mc.item.*` key：数据表与 UI 文案的生命周期不同，分开放更易维护。

### 6.2 术语表

建立 `docs/i18n-glossary.md`（实施时创建），至少覆盖：

| 中文 | English | 备注 |
|---|---|---|
| 版本 | version | |
| 实例 | instance | 版本目录 + 配置 |
| 整合包 | modpack | |
| 资源中心 | Resource Center | 页面名 |
| 任务中心 | Task Center | 页面名 |
| 启动器日志 | Launcher Log | |
| 正版登录 | Microsoft login | |
| 离线模式 | Offline mode | |
| 第三方登录 | Third-party login | |
| 衣柜 | Wardrobe | 皮肤衣柜 |
| 皮肤 / 披风 | Skin / Cape | |
| 支持库 | Library | |
| 资源索引 | Asset index | |
| 原生库 | Natives | |
| 启动参数 | Launch options | |
| 联机 | Multiplayer | 本项目的 easytier 局域网 |
| 下载源 | Download source | |
| 镜像源 | Mirror | |
| 智能模式 | Auto | |
| 修复 | Repair | |
| 快照 | Snapshot | MC 版本类型；任务快照用 snapshot 上下文区分 |

## 7. 分阶段实施

| 阶段 | 内容 | 主要文件 | 文案量 | 验证 |
|---|---|---|---|---|
| 0 基础设施 | 引入 vue-i18n；`src/i18n/`；语言切换 UI；`errMsg` 协议解析；格式化改造；`i18n-check` 脚本；`index.html lang` | `main.ts`、`App.vue`、`utils/error.ts`、`utils/format.ts`、`SettingsAppearance.vue` | 骨架 | 切换语言壳层即时变化；脚本通过 |
| 1 壳层与启动 | TitleBar、SideNav、AlertDialog、toast、空状态、Launch、账号、版本卡片、LaunchProgress、启动错误 | `component/common/*`、`component/launch/*`、`store/app/launch.ts`、`store/auth/*` | ~300 | 启动页双语截图；启动流程步骤英文 |
| 2 资源中心与任务 | Download、DownloadDetail、社区搜索/结果、VanillaDetail、ResourceDetail、下载任务状态、等待决策弹窗 | `pages/Download*.vue`、`component/download/*`、`store/task/download.ts`、`store/community/*` | ~350 | 搜索/下载/任务中心双语；错误弹窗 |
| 3 设置与版本装配 | 设置左侧 Tab（含工具 Grid）、AutoTasksEditor、VersionSelect、VersionSetup 全部子组件、主题名 | `component/settings/*`、`component/version-setup/*`、`pages/Version*.vue`、`store/config/*`、`store/app/theme.ts` | ~450 | 设置项英文不溢出；自动任务摘要 |
| 4 工具与数据表 | 工具 Grid（设置 → 工具）、各生成器/编辑器、DownloadTool、OnlinePanel（启动页右栏）、SkinStudio、SkinPainterTool、give/mcData 数据表 | `component/tools/*`、`component/settings/SettingsTools.vue`、`component/launch/OnlinePanel.vue`、`pages/SkinStudio.vue`、`utils/give.ts`、`utils/mcData.ts`、`utils/commands.ts` | ~400 + 数据表 | 9 页面双语冒烟；英文物品名/回退 id |
| 5 后端文本 | `StepInfo.kind`、任务名/阶段 key、恢复任务类型 key、`@i18n:` 错误宏与批次 1–4 | `task/model.rs`、`task/steps.rs`、`task/progress.rs`、`task/store.rs`、`task/{runner,files,jdk,repair,modpack,spawn}.rs`、`macros.rs`、相关 commands | ~150–200 错误 + 60 任务文本 | 下载/登录/安装/启动真实链路英文；旧快照名称回退 |
| 6 审校与门禁 | en-US 全文审校、术语统一、布局回归、README/截图、CI 接入 | `docs/`、`.github/workflows/release.yml`、`package.json` | 全量 | 全部门禁 + 手工冒烟清单 |

阶段 0–2 完成即可交付「英文 UI + 中文后端错误」的预览版；阶段 5 完成才算完整英文体验。阶段之间不要求一次性合并，可按页面增量提交。

## 8. 质量门禁

新增 `scripts/i18n-check.mjs`（Node 22，无新依赖）：

1. **key 对称**：`zh-CN` 与 `en-US` 递归比较，报告缺失/多余 key。TS 的 `satisfies` 已能覆盖大部分，脚本兜底动态构造的 key。
2. **占位符一致**：同一 key 的 `{var}` 集合必须一致；英文复数分支的占位符也要一致。
3. **裸中文扫描**：`src/**/*.{vue,ts}` 的模板文本、`title/placeholder/aria-label/alt`、字符串字面量中出现中文即报错；白名单：
   - 注释（脚本先剥离）；
   - `src/types/**`（生成文件）；
   - `src/i18n/messages/zh-CN/**`；
   - 数据表（`utils/give.ts`、`utils/mcData.ts`、`utils/nbt.ts` 等）与测试；
   - 故意的中文示例/占位符（需行内 `// i18n-ignore`）。
4. **后端 key 存在**：扫描 `src-tauri/src/**/*.rs` 中 `@i18n:` 或 `bail_key!` 的 key，确认在 `errors`/`backend` 命名空间存在。
5. **未使用 key**（可选，warning）：报告 locale 中未被源码引用的 key，避免死文案。

接入：

- `package.json`：`"i18n:check": "node scripts/i18n-check.mjs"`。
- `pnpm build` 是否串联？建议**不**放进 build（开发期残中文会频繁阻断），而是放进 CI 的 `check` job 与发布前手动执行；阶段 6 后可改为强制。
- `.github/workflows/release.yml` 的 `check` job 增加 `pnpm i18n:check`。

手工冒烟矩阵（1008×648）：

- 9 个页面 × 2 语言。
- 重点：TitleBar 5 个 tab、设置侧栏 6 项、任务卡片步骤名、资源详情依赖链、工具页长描述、弹窗按钮。
- 交互：切换语言不重启 → 当前页即时刷新；重启后保持；`auto` 在系统英文下默认英文。
- 运行时：真实下载一个版本（或复用测试环境），步骤名/错误提示英文；旧快照任务仍可显示。
- 构建：`npx vue-tsc --noEmit`、`pnpm build`、`pnpm lint:css`、`cargo check --lib`、`cargo clippy --lib`、`cargo test --lib -- --test-threads=1`。

## 9. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| 英文溢出固定窗口 | 按钮/侧栏/步骤名截断 | 阶段 1 起逐页截图；关键按钮用短词；允许步骤名换行；检查 `white-space: nowrap` 与固定 `width` |
| 动态拼接语序错误 | 英文句子不通 | 整句 key + 参数；禁止 `+` 拼接；翻译时按目标语序重排 |
| 模块级常量不随语言更新 | 切换后选项仍是旧语言 | 统一 `labelKey/descKey`；禁止模块顶层 `t()` |
| 后端长尾错误仍中文 | 英文体验不完整 | 高频批次优先；技术细节不译；backlog 跟踪覆盖率 |
| 旧快照任务名/旧错误 | 显示中文或 raw key | `Option` + 回退原文；协议带 fallback 行 |
| key 漂移/拼写错误 | 运行时缺 key | TS `satisfies` + `i18n-check` + CI |
| `@` / `|` 特殊字符 | 消息解析异常 | 统一转义规则；扫描含特殊字符的消息 |
| 字体英文观感 | 英文使用中文字体 | 评估 `system-ui` 优先字体栈；先在 Windows/macOS/WSLg 截图对比 |
| 日期格式变化 | 既有用户不习惯 | 首期保留 ISO 风格，仅本地化计数/ETA |
| 翻译质量 | 术语不一致 | 术语表 + 人工审校；Minecraft 术语以官方为准 |
| 性能 | 全量消息体积 | 2000 条以内无需拆包；若超 5000 条再考虑按路由懒加载 |

## 10. 待确认决策

1. **依赖**：是否允许引入 `vue-i18n@^11`？（推荐；替代方案是约 100 行的自研 `t()`，但会失去复数、回退、格式化与生态支持。）
2. **首期语言**：只做 `zh-CN` + `en-US`，其他语言回退；是否现在就要繁体中文？
3. **后端错误范围**：接受「高频 150–200 条 + 长尾渐进」还是要求首期全量迁移？全量约 400–500 条首行，工作量显著增加。
4. **任务名/阶段名**：是否接受新增 `name_key`/`current_phase_key` 字段（需重跑 `gen_ts` 与更新快照展示）？替代方案是首期只做 `StepInfo.kind`，任务名保留中文。
5. **数据表英文名**：英文模式显示官方英文名（需一次性生成 `nameEn`）还是直接显示注册 id？
6. **默认语言**：`auto`（推荐）还是固定中文？`auto` 下英文系统用户首次启动即为英文。
7. **持久化**：`localStorage`（推荐，与主题一致）还是后端 TOML `[app] language`？后者需要新增 section/命令，且后端并不消费语言。

## 11. 验收清单

- [ ] 设置 → 外观可切换 `auto / 中文 / English`，即时生效并持久化。
- [ ] 9 个页面在 `en-US` 下无裸中文（白名单除外）。
- [ ] 下载/安装/登录/启动主链路：任务名、步骤名、阶段名、错误首行为英文。
- [ ] 旧快照恢复的任务名、旧错误字符串可正常显示（回退原文，不崩、不显示 raw key）。
- [ ] `pnpm i18n:check`、`npx vue-tsc --noEmit`、`pnpm build`、`pnpm lint:css` 通过。
- [ ] `cargo check --lib`、`cargo clippy --lib`、`cargo test --lib -- --test-threads=1` 通过。
- [ ] 英文布局在 1008×648 下无关键控件溢出。
- [ ] 术语表与 README 英文版一致；发布截图更新。

## 12. 实施记录

- 前端：`vue-i18n` + `src/i18n/`（按域消息、`zh-CN`/`en-US`），语言设置位于「设置 → 外观」，持久化 `elytra.language`；`fmtCount`/`fmtEta` 已按 locale 格式化。
- 防抖：语言切换期间给 `<html>` 加 `i18n-switching`，短暂禁用过渡/动画；标题栏导航按中英较长标签预留宽度，避免整组横移。
- 后端：`StepInfo.kind`（含修复/整合包步骤）由前端翻译；`TaskProgress.name_key`/`current_phase_key` + params 承载任务名/阶段；`@i18n:` 协议 + `common_err_ctx_key!`/`bail_key!` 已迁移 283 个用户可见错误调用点。
- 数据表：give/enchant/attribute/game rule/entity/potion/firework/villager/MOTD 颜色均补英文名并用 locale-aware accessor 取用。
- 门禁：`scripts/i18n-check.mjs` 检查 key 对称、占位符一致、后端 key 存在、裸中文；`pnpm i18n:check` 通过。
- 仍保留中文（有意）：代码注释、日志、文件名、注册 id、用户数据；整合包内容文件步骤显示文件路径（语言中立）；少量 schema/校验类长尾错误保留中文兜底。
