# 外置登录（authlib-injector / 自定义皮肤站）规划

> 状态：**已实现**（2026-08-30 全量落地）。`AuthMode::External` + authlib-injector 注入、
> 登录认证（标准 Yggdrasil + Ely.by 分支）、多账号/多角色、jar 双源下载（sha256）、
> 元数据探测（皮肤站名称）、3D/大头照头像。待实测：真实登录进服显示皮肤。
> 差异：jar 用下载缓存方案（登录时拉取）而非内置；Blessing Skin 无公共实例未内置（用户自建可加预设）。

## 背景与必要性

- 国内大量第三方皮肤站 / 离线生态用户无法使用正版登录，离线模式下高版本（1.19.3+）
  又无皮肤注入通道（authlib 新版不再解析 `--userProperties`，见 `launcher/args.rs` 注释），
  导致离线用户皮肤能力与正版割裂。
- 外置登录是同时解决"第三方登录"与"离线自定义皮肤"两条痛点的唯一通用路径：
  `authlib-injector` 以 `-javaagent` 注入，接管 Yggdrasil 认证与皮肤纹理，**1.19.3+ 皮肤同样生效**。

## 方案

### 认证流

```
用户选择"外置登录" → 填皮肤站 API URL + 账号密码
  → 请求 {api}/authserver/authenticate（authlib-injector 标准协议）
  → 返回 accessToken + selectedProfile{id(uuid), name}
  → 存储 {api_url, uuid, name, access_token} 到账号列表（secrets.bin 加密，同微软账号）
```

### 启动参数注入

- 前置：下载 `authlib-injector.jar`（GitHub `yushijinhun/authlib-injector` releases，固定版本 + sha256 校验，内置进应用资源或随 JDK 同目录缓存）
- 注入：`-javaagent:<jar路径>=<api_url>`（放在 JVM 参数区，必须在 main 前）
- 身份注入：`username=<name>`、`uuid=<uuid>`（来自 authlib 认证响应，不走 Mojang profile）
- `AuthMode` 扩展：`External { api_url, uuid, username, access_token }`

### 皮肤站 API

- 元数据探测：`GET {api}/api/yggdrasil` → `meta.serverName`（皮肤站名称，登录页展示）
- 皮肤纹理由 authlib-injector 注入（客户端自动处理，无需启动器干预）

## 实施步骤

1. **后端**：`AuthMode::External` 变体 + 启动参数注入（`launcher/args.rs`）
2. **authlib-injector 分发**：内置 jar 资源 + sha256 校验 + 首次使用解压到数据目录
3. **登录命令**：`commands/account.rs` 加 `external_login(api_url, username, password)`（authserver/authenticate）
4. **账号管理**：External 账号进多账号列表（复用 MsAccount 列表 UI），secrets.bin 加密存储 token
5. **前端**：登录方式三选（正版/离线/外置）+ 皮肤站 URL 管理 + 登录表单
6. **设置页**：常用皮肤站预设（LittleSkin `https://littleskin.cn/api/yggdrasil`、Blessing Skin 等）+ 自定义

## 风险与决策

- **jar 分发方式**：内置（离线可用，约 200KB）优先；下载缓存（跟随更新）为备选
- **多皮肤站**：支持任意符合 authlib-injector 协议的 URL（元数据探测做兼容校验）
- **与离线模式关系**：外置登录成功即代替离线；离线固定 Steve/Alex 现状保留（低版本注入通道不动）
- **依赖**：无新增 Rust crate（复用现有 reqwest/加密），前端新增登录表单组件
- **难度**：高（跨认证协议 + 启动注入 + 账号存储三块）

## 验收标准

- 外置登录成功后启动游戏：进服显示皮肤站账号皮肤（含 1.21+）
- 多账号列表可切换/移除外置账号
- 未联网时使用已存 token 启动不重复认证
