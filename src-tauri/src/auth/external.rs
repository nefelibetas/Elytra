//! 第三方登录（authlib-injector / 皮肤站 / Yggdrasil 兼容服务器）。
//! 账号经 persist::secrets 加密存储（仿 MsAccount）；登录调
//! `POST {api}/authserver/authenticate`；启动前确保 authlib-injector.jar 存在。

use serde::{Deserialize, Serialize};

use crate::constants;
use crate::downloader::fetcher;

/// authlib-injector jar 文件名（数据目录 = exe 旁 Elytra/）。
pub const AUTHLIB_JAR: &str = "authlib-injector.jar";

/// Ely.by 使用非标准 Yggdrasil 路径（POST /auth/authenticate）且注入需别名
/// `ely.by`（authlib-injector 内置适配）。据此检测走特殊分支。
pub fn is_ely_by(api_url: &str) -> bool {
    api_url.contains("ely.by")
}

/// 从 Yggdrasil 错误响应文本提取可读信息：错误体常见 {"error": ..., "errorMessage": ...}；
/// 非 JSON/无 errorMessage 回退 HTTP 状态码。
fn extract_yggdrasil_error(text: &str, status: reqwest::StatusCode) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v["errorMessage"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("HTTP {status}"))
}


/// 第三方账号（含验证服务器地址与短期 access_token）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalAccount {
    /// 验证服务器根地址（如 https://littleskin.cn/api/yggdrasil）
    pub api_url: String,
    /// Minecraft profile uuid（无连字符）
    pub uuid: String,
    /// 游戏名
    pub name: String,
    /// 服务器签发的 access_token（启动游戏用）
    pub access_token: String,
    /// 皮肤站登录账号（authenticate 响应 user.username；旧账号为 None）
    #[serde(default)]
    pub site_username: Option<String>,
    /// 皮肤站名称（元数据探测 GET {api}/api/yggdrasil → meta.serverName；旧账号为 None）
    #[serde(default)]
    pub server_name: Option<String>,
    /// Yggdrasil clientToken（refresh 换角色/续期用；旧账号为 None）
    #[serde(default)]
    pub client_token: Option<String>,
    /// 账号全部角色（authenticate 响应 availableProfiles；切换角色用）
    #[serde(default)]
    pub profiles: Vec<ExternalProfile>,
}

/// 一个 Minecraft 角色（第三方账号可绑定多角色）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalProfile {
    pub id: String,
    pub name: String,
}

/// 读取全部第三方账号（登录时间升序，末尾 = 当前账号）。无账号返回空列表。
pub fn read_all() -> Vec<ExternalAccount> {
    crate::persist::secrets::read_json_list(constants::SECT_EXTERNAL_ACCOUNTS)
}

/// 读取当前第三方账号（列表最后一个，即最近登录/切换的）。无则 None。
pub fn read() -> Option<ExternalAccount> {
    read_all().into_iter().last()
}

/// 保存第三方账号：按 uuid 去重（同服务器下 uuid 唯一）并置为当前。
pub fn save(account: &ExternalAccount) -> Result<(), String> {
    let mut list = read_all();
    list.retain(|a| !(a.uuid == account.uuid && a.api_url == account.api_url));
    list.push(account.clone());
    crate::persist::secrets::save_json_list(constants::SECT_EXTERNAL_ACCOUNTS, &list)
}

/// 切换当前账号：从列表移除再重加（置末尾 = 启用）。
pub fn switch(api_url: &str, uuid: &str) -> Result<(), String> {
    let mut list = read_all();
    let Some(idx) = list.iter().position(|a| a.api_url == api_url && a.uuid == uuid) else {
        return Err(format!("账号不存在\n{api_url} {uuid}"));
    };
    let acc = list.remove(idx);
    list.push(acc);
    crate::persist::secrets::save_json_list(constants::SECT_EXTERNAL_ACCOUNTS, &list)
}

/// 移除指定账号（api_url + uuid 定位）。列表清空时删除存储键。
pub fn remove(api_url: &str, uuid: &str) -> Result<(), String> {
    let mut list = read_all();
    list.retain(|a| !(a.api_url == api_url && a.uuid == uuid));
    crate::persist::secrets::save_json_list(constants::SECT_EXTERNAL_ACCOUNTS, &list)
}

/// 登录：POST {api}/authserver/authenticate（Yggdrasil 协议）→ accessToken + profile。
/// 多角色时 selectedProfile 缺失 → 取 availableProfiles 第一个（多角色选择留待增强）。
pub async fn login(
    api_url: &str,
    username: &str,
    password: &str,
) -> Result<ExternalAccount, String> {
    let api = api_url.trim().trim_end_matches('/').to_string();
    if api.is_empty() {
        return Err("请输入验证服务器地址".into());
    }
    if username.trim().is_empty() || password.is_empty() {
        return Err("请输入账号与密码".into());
    }
    // Ely.by 非标准路径：/auth/authenticate；其余标准 /authserver/authenticate
    let url = if is_ely_by(&api) {
        format!("{api}/auth/authenticate")
    } else {
        format!("{api}/authserver/authenticate")
    };
    let client_token = uuid::Uuid::new_v4().to_string();
    let body = serde_json::json!({
        "agent": { "name": "Minecraft", "version": 1 },
        "username": username,
        "password": password,
        "requestUser": true,
        "clientToken": client_token,
    });
    let resp = fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("登录请求失败\n{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = extract_yggdrasil_error(&text, status);
        return Err(format!("登录失败\n{msg}"));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("响应解析失败\n{e}"))?;
    let token = json["accessToken"]
        .as_str()
        .ok_or("登录响应缺少 accessToken")?
        .to_string();
    // 客户端 token（refresh 换角色/续期必需；服务器通常回显，无则用请求生成的）
    let client_token = json["clientToken"]
        .as_str()
        .map(str::to_string)
        .or(Some(client_token));
    // selectedProfile 优先；缺失（多角色未选）取 availableProfiles 第一个
    let profile = json
        .get("selectedProfile")
        .filter(|p| !p.is_null())
        .or_else(|| json["availableProfiles"].as_array().and_then(|a| a.first()))
        .ok_or("账号没有可用角色（请先在皮肤站创建角色）")?;
    let uuid = profile["id"].as_str().ok_or("角色缺少 id")?.to_string();
    let name = profile["name"].as_str().ok_or("角色缺少 name")?.to_string();
    // 皮肤站名称（元数据探测；失败不阻塞）
    let server_name = server_name(&api).await;

    // 站点登录账号：优先响应 user.username（部分服务器返回 null），
    // 回退登录输入的账号名（LittleSkin 等支持用户名/邮箱登录，输入即账号标识）
    let site_username = json["user"]["username"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(username.trim().to_string()));
    // 全部角色（availableProfiles）：LittleSkin 等部分实现不返回该字段，
    // 空时用当前角色兜底（保证面板至少显示一个角色）
    let profiles = json["availableProfiles"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(ExternalProfile {
                        id: p["id"].as_str()?.to_string(),
                        name: p["name"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .filter(|list: &Vec<ExternalProfile>| !list.is_empty())
        .unwrap_or_else(|| vec![ExternalProfile {
            id: uuid.clone(),
            name: name.clone(),
        }]);
    Ok(ExternalAccount {
        api_url: api,
        uuid,
        name,
        access_token: token,
        site_username,
        server_name,
        client_token,
        profiles,
    })
}

/// 刷新账号：refresh（accessToken + clientToken）→ 新 token + 最新角色列表。
/// 覆盖极端场景：皮肤站增删角色（availableProfiles 变化）、当前角色被删/改名（selectedProfile 变化）。
pub async fn refresh_account(api_url: &str, uuid: &str) -> Result<ExternalAccount, String> {
    let mut list = read_all();
    let Some(idx) = list.iter().position(|a| a.api_url == api_url && a.uuid == uuid) else {
        return Err(format!("账号不存在\n{api_url} {uuid}"));
    };
    let acc = list[idx].clone();
    let client_token = acc
        .client_token
        .as_deref()
        .ok_or("该账号缺少凭据，请重新登录")?
        .to_string();
    let api = api_url.trim().trim_end_matches('/');
    let url = if is_ely_by(api) {
        format!("{api}/auth/refresh")
    } else {
        format!("{api}/authserver/refresh")
    };
    let body = serde_json::json!({
        "accessToken": acc.access_token,
        "clientToken": client_token,
        "requestUser": true,
    });
    let resp = fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("刷新请求失败\n{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = extract_yggdrasil_error(&text, status);
        // 401/ForbiddenOperationException = token 失效（密码改/账号删）→ 提示重新登录
        return Err(format!("登录凭据已失效，请重新登录\n{msg}"));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("响应解析失败\n{e}"))?;
    let new_token = json["accessToken"]
        .as_str()
        .ok_or("刷新响应缺少 accessToken")?
        .to_string();
    // 最新角色列表：部分服务器（Ely.by 等）refresh 响应不含 availableProfiles，
    // 缺失/为空时保留原列表（避免清空导致'暂无角色信息'）
    let profiles = json["availableProfiles"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(ExternalProfile {
                        id: p["id"].as_str()?.to_string(),
                        name: p["name"].as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .filter(|list: &Vec<ExternalProfile>| !list.is_empty())
        .unwrap_or_else(|| {
            if acc.profiles.is_empty() {
                vec![ExternalProfile {
                    id: uuid.to_string(),
                    name: acc.name.clone(),
                }]
            } else {
                acc.profiles.clone()
            }
        });
    // 当前角色（selectedProfile；可能被删/改名 → 同步）
    if let Some(sp) = json.get("selectedProfile").filter(|p| !p.is_null()) {
        if let (Some(id), Some(name)) = (sp["id"].as_str(), sp["name"].as_str()) {
            list[idx].uuid = id.to_string();
            list[idx].name = name.to_string();
        }
    }
    list[idx].access_token = new_token;
    list[idx].profiles = profiles;
    let updated = list.remove(idx);
    list.push(updated.clone());
    crate::persist::secrets::save_json_list(constants::SECT_EXTERNAL_ACCOUNTS, &list)?;
    Ok(updated)
}

/// 切换当前角色：refresh（accessToken + clientToken + selectedProfile）→ 新 token。
/// Yggdrasil 标准（authlib-injector 支持）；无 clientToken 时回退报错需重新登录。
pub async fn switch_role(
    api_url: &str,
    uuid: &str,
    profile_id: &str,
    profile_name: &str,
) -> Result<ExternalAccount, String> {
    let mut list = read_all();
    let Some(idx) = list.iter().position(|a| a.api_url == api_url && a.uuid == uuid) else {
        return Err(format!("账号不存在\n{api_url} {uuid}"));
    };
    let acc = list[idx].clone();
    let client_token = acc
        .client_token
        .as_deref()
        .ok_or("该账号缺少凭据，请重新登录")?
        .to_string();
    let api = api_url.trim().trim_end_matches('/');
    // Ely.by 非标准 refresh 路径（/auth/refresh）；标准 /authserver/refresh
    let url = if is_ely_by(api) {
        format!("{api}/auth/refresh")
    } else {
        format!("{api}/authserver/refresh")
    };
    let body = serde_json::json!({
        "accessToken": acc.access_token,
        "clientToken": client_token,
        "selectedProfile": { "id": profile_id, "name": profile_name },
        "requestUser": true,
    });
    let resp = fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("切换角色请求失败\n{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = extract_yggdrasil_error(&text, status);
        return Err(format!("切换角色失败\n{msg}"));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("响应解析失败\n{e}"))?;
    let new_token = json["accessToken"]
        .as_str()
        .ok_or("切换响应缺少 accessToken")?
        .to_string();
    // 更新账号：新角色 + 新 token
    list[idx].uuid = profile_id.to_string();
    list[idx].name = profile_name.to_string();
    list[idx].access_token = new_token;
    // 保持启用（置末尾）
    let updated = list.remove(idx);
    list.push(updated.clone());
    crate::persist::secrets::save_json_list(constants::SECT_EXTERNAL_ACCOUNTS, &list)?;
    Ok(updated)
}

/// 拉取第三方账号皮肤 URL + 模型类型（slim/classic）。
/// 标准 Yggdrasil：GET {api}/sessionserver/session/minecraft/profile/{uuid} →
/// properties[0].value（base64 JSON 含 textures.skin.url）；Ely.by 非标准路径
/// /session/profile/{uuid}。命令层两个皮肤命令共用（消除整段重复），
/// 各自再按需下载 base64 / 附带模型。
pub async fn external_skin_info(api_url: &str, uuid: &str) -> Result<(String, String), String> {
    let api = api_url.trim().trim_end_matches('/');
    if api.is_empty() {
        return Err("验证服务器地址为空".into());
    }
    let url = if is_ely_by(api) {
        format!("{api}/session/profile/{uuid}")
    } else {
        format!("{api}/sessionserver/session/minecraft/profile/{uuid}")
    };
    let resp = fetcher::http()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("请求皮肤失败\n{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("该账号未设置皮肤\nHTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("响应解析失败\n{e}"))?;
    // properties[0].value = base64 的 {textures:{SKIN:{url}}}（标准格式）
    let prop = json["properties"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["value"].as_str())
        .ok_or("该账号未设置皮肤")?;
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(prop)
        .map_err(|e| format!("皮肤数据解码失败\n{e}"))?;
    let textures: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(|e| format!("皮肤信息解析失败\n{e}"))?;
    let skin = textures["textures"]["SKIN"].clone();
    let skin_url = skin["url"]
        .as_str()
        .ok_or("该账号未设置皮肤")?
        .to_string();
    let model = skin["metadata"]["model"]
        .as_str()
        .unwrap_or("classic")
        .to_string();
    Ok((skin_url, model))
}

/// 探测皮肤站名称：GET {api}/api/yggdrasil → meta.serverName（失败/非标准返回 None，不阻塞登录）。
pub async fn server_name(api_url: &str) -> Option<String> {
    let api = api_url.trim().trim_end_matches('/');
    let url = format!("{api}/api/yggdrasil");
    let resp = fetcher::http()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    json["meta"]["serverName"].as_str().map(str::to_string)
}

/// 确保 authlib-injector.jar 存在（数据目录）。缺失时从官方/BMCLAPI 镜像拉取
/// 最新版（latest.json 拿 download_url + sha256 校验）。返回 jar 绝对路径。
pub async fn ensure_jar() -> Result<std::path::PathBuf, String> {
    let path = crate::util::config_dir().join(AUTHLIB_JAR);
    if path.exists() {
        return Ok(path);
    }
    // 双源：官方 authlib-injector.yushi.moe → BMCLAPI 镜像
    let official = "https://authlib-injector.yushi.moe/artifact/latest.json";
    let mirror = "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json";
    let mut last_err = String::new();
    for url in [official, mirror] {
        match fetcher::http()
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                let meta: serde_json::Value = match r.json().await {
                    Ok(v) => v,
                    Err(e) => {
                        last_err = format!("解析元数据失败\n{e}");
                        continue;
                    }
                };
                let dl = meta["download_url"].as_str().unwrap_or_default();
                let sha256 = meta["checksums"]["sha256"].as_str().unwrap_or_default();
                if dl.is_empty() {
                    last_err = "元数据缺少 download_url".into();
                    continue;
                }
                // 下载 jar（先临时文件再改名，失败不残留）
                let tmp = path.with_file_name(format!("{AUTHLIB_JAR}.part"));
                if let Err(e) = fetcher::fetch_verified(
                    &[dl.to_string()],
                    &tmp,
                    None::<fn(u64, u64)>,
                    "",
                )
                .await
                {
                    last_err = format!("下载失败\n{e}");
                    continue;
                }
                // sha256 校验（fetch_verified 只校验 sha1；authlib 元数据给的是 sha256）
                if !sha256.is_empty() {
                    let data = match std::fs::read(&tmp) {
                        Ok(d) => d,
                        Err(e) => {
                            last_err = format!("读取校验失败\n{e}");
                            let _ = std::fs::remove_file(&tmp);
                            continue;
                        }
                    };
                    let actual: String = {
                        use sha2::Digest;
                        sha2::Sha256::digest(&data)
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect()
                    };
                    if !actual.eq_ignore_ascii_case(sha256) {
                        last_err = "校验失败（sha256 不匹配）".into();
                        let _ = std::fs::remove_file(&tmp);
                        continue;
                    }
                }
                std::fs::rename(&tmp, &path).map_err(|e| format!("写入失败\n{e}"))?;
                return Ok(path);
            }
            Ok(_) => last_err = "元数据请求失败".into(),
            Err(e) => last_err = format!("请求失败\n{e}"),
        }
    }
    Err(format!("下载 authlib-injector 失败\n{last_err}"))
}

/// 预设验证服务器（label 展示名 + value API 地址，第三方登录 UI 预设列表）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThirdPartyServer {
    pub label: String,
    pub value: String,
}

/// 第三方登录配置（预设验证服务器列表，label + value）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ThirdPartyConfig {
    /// 常用验证服务器（皮肤站）预设列表
    #[serde(default)]
    pub servers: Vec<ThirdPartyServer>,
}

/// 默认内置预设（知名 Yggdrasil 兼容站）。
pub fn default_third_party_servers() -> Vec<ThirdPartyServer> {
    vec![
        ThirdPartyServer {
            label: "LittleSkin".into(),
            value: "https://littleskin.cn/api/yggdrasil".into(),
        },
        ThirdPartyServer {
            label: "Ely.by".into(),
            value: "https://authserver.ely.by".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(api: &str, uuid: &str, name: &str) -> ExternalAccount {
        ExternalAccount {
            api_url: api.into(),
            uuid: uuid.into(),
            name: name.into(),
            access_token: "at".into(),
            site_username: Some("site".into()),
            server_name: Some("TestSkin".into()),
            client_token: Some("ct".into()),
            profiles: vec![],
        }
    }

    fn cleanup() {
        crate::persist::secrets::remove_secret(constants::SECT_EXTERNAL_ACCOUNTS).ok();
    }

    /// 多账号：同服务器同 uuid upsert；不同服务器同 uuid 并存（不同站可同角色）。
    #[test]
    fn save_upserts_same_server_and_keeps_cross_server() {
        cleanup();
        save(&sample("https://a.example/api", "uuid-1", "Alice")).unwrap();
        save(&sample("https://b.example/api", "uuid-1", "Alice")).unwrap();
        assert_eq!(read_all().len(), 2, "不同服务器同 uuid 应并存");

        // 同服务器同 uuid → upsert 置当前
        save(&sample("https://a.example/api", "uuid-1", "Alice2")).unwrap();
        let all = read_all();
        assert_eq!(all.len(), 2);
        assert_eq!(all.last().unwrap().name, "Alice2");
        assert_eq!(all.last().unwrap().api_url, "https://a.example/api");
        cleanup();
    }

    /// remove：按 api_url + uuid 定位；清空后存储键删除。
    #[test]
    fn remove_targets_server_and_uuid() {
        cleanup();
        save(&sample("https://a.example/api", "uuid-1", "Alice")).unwrap();
        save(&sample("https://b.example/api", "uuid-1", "Bob")).unwrap();
        remove("https://a.example/api", "uuid-1").unwrap();
        let all = read_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].api_url, "https://b.example/api");
        remove("https://b.example/api", "uuid-1").unwrap();
        assert!(read().is_none());
        cleanup();
    }
}
