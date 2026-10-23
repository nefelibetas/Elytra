# 安全策略 / Security Policy

## 支持版本 / Supported versions

| 版本 / Version | 支持 / Supported |
| --- | --- |
| 1.x | ✅ |

## 报告漏洞 / Reporting a vulnerability

**请不要在公开 Issue 中披露漏洞。** 使用 GitHub 的私密漏洞报告：

**Do not disclose vulnerabilities in a public issue.** Use GitHub private vulnerability reporting:

https://github.com/nefelibetas/Elytra/security/advisories/new

请在报告中附上：受影响版本、操作系统、复现步骤、影响范围，以及必要的日志。日志请先移除 token、账号名、服务器地址与本地路径。

Include the affected version, OS, reproduction steps, impact, and relevant logs with tokens, account names, server addresses, and local paths redacted.

我们会在 7 天内确认收到，并在修复可用后发布公告。公开披露前请给我们合理的修复时间。

We aim to acknowledge within 7 days and publish an advisory once a fix is available. Please allow reasonable time before public disclosure.

## 范围 / Scope

- 微软 OAuth 与 token 存储（`secrets.bin`、系统钥匙串）/ Microsoft OAuth and token storage
- 第三方皮肤站凭据 / Third-party auth (authlib-injector) credentials
- 下载、整合包、图标、皮肤的路径穿越或任意文件写入 / Path traversal or arbitrary file writes
- 下载校验绕过（sha1 / sha256）/ Download integrity bypass
- 局域网联机（房间码、帧解码、代理与 ACL）/ LAN play (room codes, frame decoding, proxy, ACL)
- 自动任务与子进程调用的本地权限边界 / Local privilege boundaries in auto tasks and subprocesses

## 不在范围内 / Out of scope

- 需要用户先安装恶意 Mod / 整合包才能触发的问题
- 依赖上游（Mojang、Modrinth、CurseForge、EasyTier）自身服务的安全问题
- 仅影响开发者本机构建环境、且与发布产物无关的问题

- Issues that require the user to install a malicious mod or modpack first
- Vulnerabilities in upstream services (Mojang, Modrinth, CurseForge, EasyTier)
- Issues affecting only the local development environment, not release artifacts
