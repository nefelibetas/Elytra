//! 第三方登录（authlib-injector / 皮肤站 / Yggdrasil 兼容服务器）；账号经 persist::secrets 加密存储。

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;

use crate::constants;
use crate::downloader;
use crate::persist;

/// authlib-injector jar 文件名（数据目录 = exe 旁 Elytra/）。
pub const AUTHLIB_JAR: &str = "authlib-injector.jar";

/// Ely.by 用非标准 Yggdrasil 路径且注入需别名 `ely.by`（authlib-injector 内置适配）。
pub fn is_ely_by(api_url: &str) -> bool {
    api_url.contains("ely.by")
}

/// 从 Yggdrasil 错误响应提取可读信息：`{"error":..,"errorMessage":..}`，非 JSON/无字段回退 HTTP 状态码。
fn extract_yggdrasil_error(text: &str, status: reqwest::StatusCode) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v["errorMessage"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("HTTP {status}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "auth/")]
pub struct ExternalAccount {
    /// 验证服务器根地址（如 https://littleskin.cn/api/yggdrasil）
    pub api_url: String,
    /// Minecraft profile uuid（无连字符）
    pub uuid: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "auth/")]
pub struct ExternalProfile {
    pub id: String,
    pub name: String,
}

/// 读取全部第三方账号（登录时间升序，末尾 = 当前账号）。无账号返回空列表。
pub fn read_all() -> Vec<ExternalAccount> {
    persist::secrets::read_json_list(constants::SECT_EXTERNAL_ACCOUNTS)
}

/// 读取当前第三方账号（列表最后一个，即最近登录/切换的）。无则 None。
pub fn read() -> Option<ExternalAccount> {
    read_all().into_iter().last()
}

/// 保存第三方账号：按 uuid 去重（同服务器下 uuid 唯一）并置为当前。
pub fn save(account: &ExternalAccount) -> Result<(), String> {
    let account = account.clone();
    persist::secrets::update_json_list(
        constants::SECT_EXTERNAL_ACCOUNTS,
        move |list: &mut Vec<ExternalAccount>| {
            list.retain(|a| !(a.uuid == account.uuid && a.api_url == account.api_url));
            list.push(account);
            Ok(())
        },
    )
}

/// 切换当前账号：从列表移除再重加（置末尾 = 启用）。
pub fn switch(api_url: &str, uuid: &str) -> Result<(), String> {
    let (api, id) = (api_url.to_string(), uuid.to_string());
    persist::secrets::update_json_list(
        constants::SECT_EXTERNAL_ACCOUNTS,
        move |list: &mut Vec<ExternalAccount>| {
            let Some(idx) = list.iter().position(|a| a.api_url == api && a.uuid == id) else {
                bail_key!("errors.auth.accountNotFound", "账号不存在\n{api} {id}");
            };
            let acc = list.remove(idx);
            list.push(acc);
            Ok(())
        },
    )
}

/// 移除指定账号（api_url + uuid 定位）。列表清空时删除存储键。
pub fn remove(api_url: &str, uuid: &str) -> Result<(), String> {
    let (api, id) = (api_url.to_string(), uuid.to_string());
    persist::secrets::update_json_list(
        constants::SECT_EXTERNAL_ACCOUNTS,
        move |list: &mut Vec<ExternalAccount>| {
            list.retain(|a| !(a.api_url == api && a.uuid == id));
            Ok(())
        },
    )
}

/// 登录：POST {api}/authserver/authenticate（Yggdrasil）→ accessToken + profile；
/// 多角色时 selectedProfile 缺失则取 availableProfiles 第一个。
pub async fn login(
    api_url: &str,
    username: &str,
    password: &str,
) -> Result<ExternalAccount, String> {
    let api = api_url.trim().trim_end_matches('/').to_string();
    if api.is_empty() {
        return Err(keyed_msg!(
            "errors.auth.serverUrlRequired",
            "请输入验证服务器地址"
        ));
    }
    if username.trim().is_empty() || password.is_empty() {
        return Err(keyed_msg!(
            "errors.auth.credentialsRequired",
            "请输入账号与密码"
        ));
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
    let resp = downloader::fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(common_err_ctx_key!("errors.login.failed", "登录请求失败"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = extract_yggdrasil_error(&text, status);
        bail_key!("errors.login.failed", "登录失败\n{msg}");
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx_key!("errors.parse", "响应解析失败"))?;
    let token = json["accessToken"]
        .as_str()
        .ok_or_else(|| keyed_msg!("errors.auth.loginNoToken", "登录响应缺少 accessToken"))?
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
        .ok_or_else(|| {
            keyed_msg!(
                "errors.auth.noProfiles",
                "账号没有可用角色（请先在皮肤站创建角色）"
            )
        })?;
    let uuid = profile["id"]
        .as_str()
        .ok_or_else(|| keyed_msg!("errors.auth.profileMissingField?field=id", "角色缺少 id"))?
        .to_string();
    let name = profile["name"]
        .as_str()
        .ok_or_else(|| {
            keyed_msg!(
                "errors.auth.profileMissingField?field=name",
                "角色缺少 name"
            )
        })?
        .to_string();
    // 皮肤站名称（元数据探测；失败不阻塞）
    let server_name = server_name(&api).await;

    // 站点登录账号：优先 user.username（部分服务器返回 null），回退登录输入的用户名。
    let site_username = json["user"]["username"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(username.trim().to_string()));
    // 全部角色：部分实现不返回 availableProfiles，空时用当前角色兜底
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
            vec![ExternalProfile {
                id: uuid.clone(),
                name: name.clone(),
            }]
        });
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

/// 刷新账号：refresh → 新 token + 最新角色列表（同步角色增删/改名）。
pub async fn refresh_account(api_url: &str, uuid: &str) -> Result<ExternalAccount, String> {
    let list = read_all();
    let Some(idx) = list
        .iter()
        .position(|a| a.api_url == api_url && a.uuid == uuid)
    else {
        bail_key!(
            "errors.auth.accountNotFound",
            "账号不存在\n{api_url} {uuid}"
        );
    };
    let acc = list[idx].clone();
    let client_token = acc
        .client_token
        .as_deref()
        .ok_or_else(|| keyed_msg!("errors.auth.noCredentials", "该账号缺少凭据，请重新登录"))?
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
    let resp = downloader::fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(common_err_ctx_key!("errors.login.refresh", "刷新请求失败"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let code = status.as_u16();
        let text = resp.text().await.unwrap_or_default();
        let msg = extract_yggdrasil_error(&text, status);
        // 401/403 = 凭据彻底失效 → 注销该账号；其它非 2xx（5xx/网关）→ 保留账号。
        if code == 401 || code == 403 {
            let _ = remove(api_url, uuid); // 幂等；注销失败也不影响把错误报给前端
            bail_key!(
                "errors.login.expired",
                "登录凭据已失效，已退出该账号，请重新登录\n{msg}"
            );
        }
        bail_key!(
            "errors.login.refresh",
            "刷新失败（服务端返回 {status}），账号已保留\n{msg}"
        );
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx_key!("errors.parse", "响应解析失败"))?;
    let new_token = json["accessToken"]
        .as_str()
        .ok_or_else(|| keyed_msg!("errors.auth.refreshNoToken", "刷新响应缺少 accessToken"))?
        .to_string();
    // 部分服务器（Ely.by 等）refresh 响应无 availableProfiles，缺失/为空时保留原列表
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
    let selected = json
        .get("selectedProfile")
        .filter(|p| !p.is_null())
        .and_then(|sp| {
            Some((
                sp["id"].as_str()?.to_string(),
                sp["name"].as_str()?.to_string(),
            ))
        });
    // 原子读-改-写：网络 await 期间列表可能被并发改动，快照整体写回会丢改动。
    let (api, id) = (api_url.to_string(), uuid.to_string());
    let updated = persist::secrets::update_json_list(
        constants::SECT_EXTERNAL_ACCOUNTS,
        move |list: &mut Vec<ExternalAccount>| {
            let Some(idx) = list.iter().position(|a| a.api_url == api && a.uuid == id) else {
                return Err(keyed_msg!(
                    "errors.auth.accountRemoved",
                    "账号已不存在（可能已被移除）"
                ));
            };
            if let Some((new_id, new_name)) = &selected {
                list[idx].uuid = new_id.clone();
                list[idx].name = new_name.clone();
            }
            list[idx].access_token = new_token;
            list[idx].profiles = profiles;
            let acc = list.remove(idx);
            list.push(acc.clone());
            Ok(acc)
        },
    )?;
    Ok(updated)
}

/// 切换当前角色：refresh（accessToken + clientToken + selectedProfile）→ 新 token；无 clientToken 时报错。
pub async fn switch_role(
    api_url: &str,
    uuid: &str,
    profile_id: &str,
    profile_name: &str,
) -> Result<ExternalAccount, String> {
    let list = read_all();
    let Some(idx) = list
        .iter()
        .position(|a| a.api_url == api_url && a.uuid == uuid)
    else {
        bail_key!(
            "errors.auth.accountNotFound",
            "账号不存在\n{api_url} {uuid}"
        );
    };
    let acc = list[idx].clone();
    let client_token = acc
        .client_token
        .as_deref()
        .ok_or_else(|| keyed_msg!("errors.auth.noCredentials", "该账号缺少凭据，请重新登录"))?
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
    let resp = downloader::fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(common_err_ctx_key!(
            "errors.account.failed",
            "切换角色请求失败"
        ))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = extract_yggdrasil_error(&text, status);
        bail_key!("errors.account.failed", "切换角色失败\n{msg}");
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx_key!("errors.parse", "响应解析失败"))?;
    let new_token = json["accessToken"]
        .as_str()
        .ok_or_else(|| keyed_msg!("errors.auth.switchNoToken", "切换响应缺少 accessToken"))?
        .to_string();
    // 更新账号并置末尾启用；同样原子读-改-写，避免并发改动被陈旧快照覆盖。
    let (api, id) = (api_url.to_string(), uuid.to_string());
    let (new_uuid, new_name) = (profile_id.to_string(), profile_name.to_string());
    let updated = persist::secrets::update_json_list(
        constants::SECT_EXTERNAL_ACCOUNTS,
        move |list: &mut Vec<ExternalAccount>| {
            let Some(idx) = list.iter().position(|a| a.api_url == api && a.uuid == id) else {
                return Err(keyed_msg!(
                    "errors.auth.accountRemoved",
                    "账号已不存在（可能已被移除）"
                ));
            };
            list[idx].uuid = new_uuid;
            list[idx].name = new_name;
            list[idx].access_token = new_token;
            let acc = list.remove(idx);
            list.push(acc.clone());
            Ok(acc)
        },
    )?;
    Ok(updated)
}

/// 拉取第三方账号皮肤 URL + 模型（slim/classic）。标准 Yggdrasil：
/// GET {api}/sessionserver/session/minecraft/profile/{uuid} → properties[0].value
/// （base64 JSON 含 textures.skin.url）；Ely.by 路径为 /session/profile/{uuid}。
pub async fn external_skin_info(api_url: &str, uuid: &str) -> Result<(String, String), String> {
    let api = api_url.trim().trim_end_matches('/');
    if api.is_empty() {
        return Err(keyed_msg!(
            "errors.auth.serverUrlEmpty",
            "验证服务器地址为空"
        ));
    }
    let url = if is_ely_by(api) {
        format!("{api}/session/profile/{uuid}")
    } else {
        format!("{api}/sessionserver/session/minecraft/profile/{uuid}")
    };
    let resp = downloader::fetcher::http()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(common_err_ctx_key!("errors.skin.load", "请求皮肤失败"))?;
    if !resp.status().is_success() {
        bail_key!(
            "errors.account.noSkin",
            "该账号未设置皮肤\nHTTP {}",
            resp.status()
        );
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(common_err_ctx_key!("errors.parse", "响应解析失败"))?;
    // properties[0].value = base64 的 {textures:{SKIN:{url}}}（标准格式）
    let prop = json["properties"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["value"].as_str())
        .ok_or_else(|| keyed_msg!("errors.account.noSkin", "该账号未设置皮肤"))?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(prop)
        .map_err(common_err_ctx_key!("errors.skin.load", "皮肤数据解码失败"))?;
    let textures: serde_json::Value = serde_json::from_slice(&decoded)
        .map_err(common_err_ctx_key!("errors.skin.load", "皮肤信息解析失败"))?;
    let skin = textures["textures"]["SKIN"].clone();
    let skin_url = skin["url"]
        .as_str()
        .ok_or_else(|| keyed_msg!("errors.account.noSkin", "该账号未设置皮肤"))?
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
    let resp = downloader::fetcher::http()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(common_err_ctx_key!(
            "errors.auth.serverNameFailed",
            "请求服务器名称失败"
        ))
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    json["meta"]["serverName"].as_str().map(str::to_string)
}

/// 确保 authlib-injector.jar 存在（数据目录）；缺失时从官方/BMCLAPI 镜像拉取 latest.json
/// （download_url + sha256 校验）。返回 jar 绝对路径。
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
        match downloader::fetcher::http()
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
                if let Err(e) = downloader::fetcher::fetch_verified(
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
                std::fs::rename(&tmp, &path)
                    .map_err(common_err_ctx_key!("errors.file.write", "写入失败"))?;
                return Ok(path);
            }
            Ok(_) => last_err = "元数据请求失败".into(),
            Err(e) => last_err = format!("请求失败\n{e}"),
        }
    }
    Err(keyed_msg!(
        "errors.auth.injectorDownloadFailed",
        "下载 authlib-injector 失败\n{last_err}"
    ))
}

/// 预设验证服务器（label 展示名 + value API 地址，第三方登录 UI 预设列表）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "auth/")]
pub struct ExternalServer {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "auth/")]
pub struct ExternalConfig {
    #[serde(default)]
    pub servers: Vec<ExternalServer>,
}

impl ExternalConfig {
    /// 读取第三方登录配置；仅 section 缺失时注入内置默认预设，用户删空的列表必须保持为空。
    pub fn load() -> Self {
        match persist::read::<Self>(constants::SECT_EXTERNAL) {
            Ok(cfg) => cfg,
            Err(persist::PersistError::NotFound) => Self {
                servers: default_external_servers(),
            },
            Err(_) => Self::default(),
        }
    }
}

/// 默认内置预设（知名 Yggdrasil 兼容站）。
pub fn default_external_servers() -> Vec<ExternalServer> {
    vec![
        ExternalServer {
            label: "LittleSkin".into(),
            value: "https://littleskin.cn/api/yggdrasil".into(),
        },
        ExternalServer {
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
        persist::secrets::update_json_list::<ExternalAccount, _, ()>(
            constants::SECT_EXTERNAL_ACCOUNTS,
            |list| {
                list.clear();
                Ok(())
            },
        )
        .ok();
    }

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

    #[test]
    fn empty_external_config_is_not_reseeded() {
        persist::save(
            constants::SECT_EXTERNAL,
            &ExternalConfig {
                servers: Vec::new(),
            },
        )
        .unwrap();
        assert!(ExternalConfig::load().servers.is_empty());
        persist::remove(constants::SECT_EXTERNAL).unwrap();
    }

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
