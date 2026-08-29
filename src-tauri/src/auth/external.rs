//! 第三方登录（authlib-injector / 皮肤站 / Yggdrasil 兼容服务器）。
//! 账号经 persist::secrets 加密存储（仿 MsAccount）；登录调
//! `POST {api}/authserver/authenticate`；启动前确保 authlib-injector.jar 存在。

use serde::{Deserialize, Serialize};

use crate::constants;

/// authlib-injector jar 文件名（数据目录 = exe 旁 Elytra/）。
pub const AUTHLIB_JAR: &str = "authlib-injector.jar";

/// Ely.by 使用非标准 Yggdrasil 路径（POST /auth/authenticate）且注入需别名
/// `ely.by`（authlib-injector 内置适配）。据此检测走特殊分支。
pub fn is_ely_by(api_url: &str) -> bool {
    api_url.contains("ely.by")
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
    match crate::persist::secrets::read_secret(constants::SECT_EXTERNAL_ACCOUNTS) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        None => vec![],
    }
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
    let bytes = serde_json::to_vec(&list).map_err(|e| format!("序列化数据失败\n{e}"))?;
    crate::persist::secrets::save_secret(constants::SECT_EXTERNAL_ACCOUNTS, &bytes)
}

/// 切换当前账号：从列表移除再重加（置末尾 = 启用）。
pub fn switch(api_url: &str, uuid: &str) -> Result<(), String> {
    let mut list = read_all();
    let Some(idx) = list.iter().position(|a| a.api_url == api_url && a.uuid == uuid) else {
        return Err(format!("账号不存在\n{api_url} {uuid}"));
    };
    let acc = list.remove(idx);
    list.push(acc);
    let bytes = serde_json::to_vec(&list).map_err(|e| format!("序列化数据失败\n{e}"))?;
    crate::persist::secrets::save_secret(constants::SECT_EXTERNAL_ACCOUNTS, &bytes)
}

/// 移除指定账号（api_url + uuid 定位）。列表清空时删除存储键。
pub fn remove(api_url: &str, uuid: &str) -> Result<(), String> {
    let mut list = read_all();
    list.retain(|a| !(a.api_url == api_url && a.uuid == uuid));
    if list.is_empty() {
        return crate::persist::secrets::remove_secret(constants::SECT_EXTERNAL_ACCOUNTS);
    }
    let bytes = serde_json::to_vec(&list).map_err(|e| format!("序列化数据失败\n{e}"))?;
    crate::persist::secrets::save_secret(constants::SECT_EXTERNAL_ACCOUNTS, &bytes)
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
    let resp = crate::downloader::fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("登录请求失败\n{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // 皮肤站错误体常见 {"error": "...", "errorMessage": "..."}
        let msg = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["errorMessage"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {status}"));
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
        .or_else(|| Some(client_token));
    // 全部角色（availableProfiles）
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
        .unwrap_or_default();
    // selectedProfile 优先；缺失（多角色未选）取 availableProfiles 第一个
    let profile = json
        .get("selectedProfile")
        .filter(|p| !p.is_null())
        .or_else(|| json["availableProfiles"].as_array().and_then(|a| a.first()))
        .ok_or("账号没有可用角色（请先在皮肤站创建角色）")?;
    let uuid = profile["id"].as_str().ok_or("角色缺少 id")?.to_string();
    let name = profile["name"].as_str().ok_or("角色缺少 name")?.to_string();
    // 站点登录账号：优先响应 user.username（部分服务器返回 null），
    // 回退登录输入的账号名（LittleSkin 等支持用户名/邮箱登录，输入即账号标识）
    let site_username = json["user"]["username"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(username.trim().to_string()));
    Ok(ExternalAccount {
        api_url: api,
        uuid,
        name,
        access_token: token,
        site_username,
        client_token,
        profiles,
    })
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
    let resp = crate::downloader::fetcher::http()
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("切换角色请求失败\n{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["errorMessage"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {status}"));
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
    let bytes = serde_json::to_vec(&list).map_err(|e| format!("序列化数据失败\n{e}"))?;
    crate::persist::secrets::save_secret(constants::SECT_EXTERNAL_ACCOUNTS, &bytes)?;
    Ok(updated)
}

/// 确保 authlib-injector.jar 存在（数据目录）。缺失时从官方/BMCLAPI 镜像拉取
/// 最新版（latest.json 拿 download_url + sha256 校验）。返回 jar 绝对路径。
pub async fn ensure_jar() -> Result<std::path::PathBuf, String> {
    let path = crate::util::exe_dir()
        .join(constants::CONFIG_DIR)
        .join(AUTHLIB_JAR);
    if path.exists() {
        return Ok(path);
    }
    // 双源：官方 authlib-injector.yushi.moe → BMCLAPI 镜像
    let official = "https://authlib-injector.yushi.moe/artifact/latest.json";
    let mirror = "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/latest.json";
    let mut last_err = String::new();
    for url in [official, mirror] {
        match crate::downloader::fetcher::http()
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
                if let Err(e) = crate::downloader::fetcher::fetch_verified(
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
