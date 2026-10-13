#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use base64::{engine, Engine};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use parking_lot::Mutex;

use crate::util::config_dir;

fn secrets_path() -> PathBuf {
    config_dir().join("secrets.bin")
}

/// 串行化读写（与 persist 的 PERSIST_LOCK 同语义，避免并发覆盖）。
static SECRETS_LOCK: Mutex<()> = Mutex::new(());

/// 锁内版本（调用方已持 `SECRETS_LOCK`）。
fn save_secret_locked(key: &str, value: &[u8]) -> Result<(), String> {
    let encrypted = encrypt_bytes(value)?;
    let b64 = engine::general_purpose::STANDARD.encode(&encrypted);
    let mut map = load_map();
    map.insert(key.to_string(), b64);
    write_map(&map)
}

/// 读取一条敏感数据（解密）。
pub fn read_secret(key: &str) -> Option<Vec<u8>> {
    let _guard = SECRETS_LOCK.lock();
    let map = load_map();
    let b64 = map.get(key)?;
    let encrypted = engine::general_purpose::STANDARD.decode(b64).ok()?;
    decrypt_bytes(&encrypted).ok()
}

/// 锁内版本（调用方已持 `SECRETS_LOCK`）。
fn remove_secret_locked(key: &str) -> Result<(), String> {
    let mut map = load_map();
    if map.remove(key).is_some() {
        write_map(&map)
    } else {
        Ok(())
    }
}

/// 读取加密存储的 JSON 列表（无条目/解密失败回空列表）。
pub fn read_json_list<T: serde::de::DeserializeOwned>(key: &str) -> Vec<T> {
    read_secret(key)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// 对某个键做**原子**「读-改-写」（整段持锁）。
///
/// `read_json_list` / `save_json_list` 各自加锁，两次调用之间是竞争窗口：并发操作
/// （如「静默刷新后 save」与「登录/删除账号的 save」重叠）会让后写者用**旧列表**整体
/// 覆盖，表现为「新登录的账号重启后消失」或「已删账号复活（连同其 refresh_token）」。
/// 凡是「读列表 → 改列表 → 存回」的调用一律走这里。
pub fn update_json_list<T, F, R>(key: &str, f: F) -> Result<R, String>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce(&mut Vec<T>) -> Result<R, String>,
{
    let _guard = SECRETS_LOCK.lock();
    let mut list: Vec<T> = load_map()
        .get(key)
        .and_then(|b64| engine::general_purpose::STANDARD.decode(b64).ok())
        .and_then(|enc| decrypt_bytes(&enc).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let out = f(&mut list)?;
    if list.is_empty() {
        remove_secret_locked(key)?;
    } else {
        let bytes = serde_json::to_vec(&list).map_err(common_err_ctx!("序列化数据失败"))?;
        save_secret_locked(key, &bytes)?;
    }
    Ok(out)
}

fn load_map() -> std::collections::HashMap<String, String> {
    let path = secrets_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    match serde_json::from_str(&content) {
        Ok(map) => map,
        Err(e) => {
            // 解析失败**不能**静默当空表：下一次写入会用空表覆盖整个文件，凭据彻底丢失。
            // 先把损坏文件挪成 .bak 留证据（与 Settings.toml 的损坏恢复同思路）。
            let bak = path.with_extension("bin.bak");
            let _ = std::fs::rename(&path, &bak);
            eprintln!(
                "[secrets] secrets.bin 解析失败，已备份为 {}：{e}",
                bak.display()
            );
            std::collections::HashMap::new()
        }
    }
}

fn write_map(map: &std::collections::HashMap<String, String>) -> Result<(), String> {
    let path = secrets_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(common_err_ctx!("创建配置目录失败"))?;
    }
    let content = serde_json::to_string(map).map_err(common_err_ctx!("序列化 secrets 失败"))?;
    // 原子写：先写 .tmp 再 rename（与 Settings.toml 的 store::write_raw 同构）。
    // 直接 fs::write 会在中途崩溃/断电时留下半截 JSON，而解析失败等于凭据全丢
    // （表现为「账号与皮肤收藏一次性全没了」）。
    let tmp = path.with_extension("bin.tmp");
    std::fs::write(&tmp, content).map_err(common_err_ctx!("写入 secrets 失败"))?;
    // 权限必须落在**已存在**的 tmp 上：旧实现先 chmod 后 write，首次写入时目标还不存在
    // （ENOENT 被吞成日志），文件按 umask 落成 0644 —— 在「钥匙串不可用 → 明文降级」
    // 路径下，同机其他本地用户可直接读到 token。chmod 失败按写入失败上抛。
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(common_err_ctx!("设置密钥文件权限失败"))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("写入 secrets 失败\n{e}")
    })
}

/// 加密（Windows DPAPI / Linux-macOS 钥匙串主密钥 ChaCha20 / 钥匙串不可用降级明文）。
fn encrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        dpapi::encrypt(data)
    }
    #[cfg(target_os = "linux")]
    {
        key_utils::encrypt_linux(data)
    }
    #[cfg(target_os = "macos")]
    {
        keychain::encrypt_macos(data)
    }
}

fn decrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        dpapi::decrypt(data)
    }
    #[cfg(target_os = "linux")]
    {
        key_utils::decrypt_linux(data)
    }
    #[cfg(target_os = "macos")]
    {
        keychain::decrypt_macos(data)
    }
}

/// 钥匙串主密钥加密格式前缀（Linux/macOS 共用）：
/// `"ELY1" || nonce(12B) || ChaCha20-Poly1305 密文`。
/// 无前缀 → 明文（钥匙串不可用期间写入的降级数据，解密时原样返回）。
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAGIC: &[u8] = b"ELY1";

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn key_encrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes).map_err(common_err_ctx!("生成随机数失败"))?;
    // 定长来源走 fallible 转换（长度由类型保证，错误分支仅为消除 panic 路径）
    let key_ref = Key::try_from(key.as_slice())
        .map_err(|_| "内部错误：主密钥长度异常".to_string())?;
    let nonce_ref = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|_| "内部错误：nonce 长度异常".to_string())?;
    let cipher = ChaCha20Poly1305::new(&key_ref);
    let ct = cipher
        .encrypt(&nonce_ref, data)
        .map_err(common_err_ctx!("加密失败"))?;
    let mut out = Vec::with_capacity(MAGIC.len() + nonce_bytes.len() + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn key_decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    let Some(rest) = data.strip_prefix(MAGIC) else {
        return Ok(data.to_vec());
    };
    if rest.len() < 12 {
        return Err("密文格式异常".into());
    }
    let (nonce, ct) = rest.split_at(12);
    let key_ref = Key::try_from(key.as_slice())
        .map_err(|_| "内部错误：主密钥长度异常".to_string())?;
    let nonce_ref =
        Nonce::try_from(nonce).map_err(|_| "内部错误：nonce 长度异常".to_string())?;
    let cipher = ChaCha20Poly1305::new(&key_ref);
    cipher
        .decrypt(&nonce_ref, ct)
        .map_err(common_err_ctx!("解密失败"))
}

/// Linux Secret Service（GNOME Keyring / KWallet kwalletd5）：主密钥存 default collection，
/// secrets.bin 每条 value 用主密钥 ChaCha20-Poly1305 加密。
/// 无 Secret Service / collection 未解锁时降级明文（encrypt/decrypt 原样返回）。
#[cfg(target_os = "linux")]
mod key_utils {
    use std::collections::HashMap;
    use std::sync::LazyLock;

    use secret_service::{EncryptionType, SecretService};

    use crate::constants::{
        KEYCHAIN_MASTER_KEY_ACCOUNT, KEYCHAIN_MASTER_KEY_LABEL, KEYCHAIN_SERVICE,
    };
    use crate::util::block_on;

    /// 主密钥懒加载（进程内缓存）；钥匙串不可用 → None（调用方降级明文）。
    static MASTER_KEY: LazyLock<Option<[u8; 32]>> = LazyLock::new(|| {
        block_on(async {
            if let Ok(Some(key)) = fetch_key().await {
                return Some(key);
            }
            // 无条目或读取失败：生成新密钥并写入；写失败（未解锁/无 Secret Service）→ 降级明文
            let mut key = [0u8; 32];
            if getrandom::getrandom(&mut key).is_err() {
                return None;
            }
            if store_key(&key).await.is_ok() {
                Some(key)
            } else {
                None
            }
        })
    });

    fn attrs() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("service", KEYCHAIN_SERVICE),
            ("account", KEYCHAIN_MASTER_KEY_ACCOUNT),
        ])
    }

    async fn fetch_key() -> Result<Option<[u8; 32]>, String> {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(common_err_ctx!("连接 Secret Service 失败"))?;
        let collection = ss
            .get_default_collection()
            .await
            .map_err(common_err_ctx!("获取默认钥匙串失败"))?;
        let items = collection
            .search_items(attrs())
            .await
            .map_err(common_err_ctx!("查询钥匙串失败"))?;
        let Some(item) = items.first() else {
            return Ok(None);
        };
        let secret = item
            .get_secret()
            .await
            .map_err(common_err_ctx!("读取钥匙串失败"))?;
        let key: [u8; 32] = secret
            .as_slice()
            .try_into()
            .map_err(|_| "主密钥长度异常".to_string())?;
        Ok(Some(key))
    }

    async fn store_key(key: &[u8; 32]) -> Result<(), String> {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(common_err_ctx!("连接 Secret Service 失败"))?;
        let collection = ss
            .get_default_collection()
            .await
            .map_err(common_err_ctx!("获取默认钥匙串失败"))?;
        collection
            .create_item(
                KEYCHAIN_MASTER_KEY_LABEL,
                attrs(),
                key,
                true,
                "application/octet-stream",
            )
            .await
            .map_err(common_err_ctx!("写入钥匙串失败"))?;
        Ok(())
    }

    fn master_key() -> Option<&'static [u8; 32]> {
        MASTER_KEY.as_ref()
    }

    pub fn encrypt_linux(data: &[u8]) -> Result<Vec<u8>, String> {
        match master_key() {
            Some(key) => super::key_encrypt(key, data),
            None => Ok(data.to_vec()),
        }
    }

    pub fn decrypt_linux(data: &[u8]) -> Result<Vec<u8>, String> {
        match master_key() {
            Some(key) => super::key_decrypt(key, data),
            None => Ok(data.to_vec()),
        }
    }
}

/// macOS Keychain Services：主密钥存 generic password 条目，secrets.bin 每条 value 用
/// 主密钥 ChaCha20-Poly1305 加密。钥匙串不可用时降级明文。
#[cfg(target_os = "macos")]
mod keychain {
    use std::sync::LazyLock;

    use core_foundation::data::CFData;
    use security_framework::item::{
        ItemAddOptions, ItemAddValue, ItemClass, ItemSearchOptions, SearchResult,
    };

    use crate::constants::{KEYCHAIN_MASTER_KEY_ACCOUNT, KEYCHAIN_SERVICE};

    /// 主密钥懒加载（进程内缓存）；钥匙串不可用 → None（调用方降级明文）。
    static MASTER_KEY: LazyLock<Option<[u8; 32]>> = LazyLock::new(|| {
        if let Ok(Some(key)) = load_key() {
            return Some(key);
        }
        // 无条目或读取失败：生成新密钥并写入；写失败（钥匙串锁定/无登录钥匙串）→ 降级明文
        let mut key = [0u8; 32];
        if getrandom::getrandom(&mut key).is_err() {
            return None;
        }
        if store_key(&key).is_ok() {
            Some(key)
        } else {
            None
        }
    });

    fn load_key() -> Result<Option<[u8; 32]>, String> {
        let results = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .account(KEYCHAIN_MASTER_KEY_ACCOUNT)
            .load_data(true)
            .search()
            .map_err(common_err_ctx!("读取钥匙串失败"))?;
        let Some(SearchResult::Data(data)) = results.into_iter().next() else {
            return Ok(None);
        };
        let key: [u8; 32] = data
            .as_slice()
            .try_into()
            .map_err(common_err_ctx!("主密钥长度异常"))?;
        Ok(Some(key))
    }

    fn store_key(key: &[u8; 32]) -> Result<(), String> {
        // 幂等：先删后写（对齐 Linux create_item replace=true 语义）
        let _ = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .account(KEYCHAIN_MASTER_KEY_ACCOUNT)
            .delete();
        ItemAddOptions::new(ItemAddValue::Data {
            class: ItemClass::generic_password(),
            data: CFData::from_buffer(key),
        })
        .set_service(KEYCHAIN_SERVICE)
        .set_account_name(KEYCHAIN_MASTER_KEY_ACCOUNT)
        .add()
        .map_err(common_err_ctx!("写入钥匙串失败"))
    }

    fn master_key() -> Option<&'static [u8; 32]> {
        MASTER_KEY.as_ref()
    }

    pub fn encrypt_macos(data: &[u8]) -> Result<Vec<u8>, String> {
        match master_key() {
            Some(key) => super::key_encrypt(key, data),
            None => Ok(data.to_vec()),
        }
    }

    pub fn decrypt_macos(data: &[u8]) -> Result<Vec<u8>, String> {
        match master_key() {
            Some(key) => super::key_decrypt(key, data),
            None => Ok(data.to_vec()),
        }
    }
}

/// Windows DPAPI(CryptProtectData)：零依赖 FFI（链接系统 advapi32）。
#[cfg(target_os = "windows")]
mod dpapi {
    use std::ffi::c_void;
    use std::ptr;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_desc: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_desc: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut c_void,
            p_prompt_struct: *mut c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;
        fn LocalFree(h_mem: *mut c_void) -> *mut c_void;
    }

    /// 无 UI 提示（后台进程），绑定当前用户（CRYPTPROTECT_UI_FORBIDDEN）。
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    pub fn encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
        let in_blob = DataBlob {
            cb_data: data.len() as u32,
            pb_data: data.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };
        // SAFETY: in_blob 的 pb_data/cb_data 由 data 切片及其长度构造且 DPAPI 只读输入；
        // out_blob 是本栈帧可写变量，CryptProtectData 成功后由其填充，返回值已检查。
        let ok = unsafe {
            CryptProtectData(
                &in_blob,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if ok == 0 {
            return Err("CryptProtectData 失败".into());
        }
        // SAFETY: ok != 0 表示调用成功，out_blob.pb_data 指向 LocalAlloc 分配、长度恰为 cb_data 的有效缓冲。
        let out = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize).to_vec()
        };
        // SAFETY: out_blob.pb_data 是上一步输出的 LocalAlloc 内存句柄，此前未释放，此处释放且仅释放一次。
        unsafe {
            LocalFree(out_blob.pb_data as *mut c_void);
        }
        Ok(out)
    }

    pub fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
        let in_blob = DataBlob {
            cb_data: data.len() as u32,
            pb_data: data.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };
        // SAFETY: in_blob 的 pb_data/cb_data 由 data 切片及其长度构造且 DPAPI 只读输入；
        // out_blob 是本栈帧可写变量，CryptUnprotectData 成功后由其填充，返回值已检查。
        let ok = unsafe {
            CryptUnprotectData(
                &in_blob,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if ok == 0 {
            return Err("CryptUnprotectData 失败（凭据不可解密，可能已换 Windows 用户）".into());
        }
        // SAFETY: ok != 0 表示调用成功，out_blob.pb_data 指向 LocalAlloc 分配、长度恰为 cb_data 的有效缓冲。
        let out = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize).to_vec()
        };
        // SAFETY: out_blob.pb_data 是上一步输出的 LocalAlloc 内存句柄，此前未释放，此处释放且仅释放一次。
        unsafe {
            LocalFree(out_blob.pb_data as *mut c_void);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 清理测试 key（避免污染其他测试/真实数据）。
    /// 走与生产一致的原子写入口：清空列表即删除条目。
    fn cleanup(key: &str) {
        let _ = update_json_list::<String, _, ()>(key, |list| {
            list.clear();
            Ok(())
        });
    }

    #[test]
    fn json_list_roundtrip_and_remove() {
        let key = format!("test_secret_{}", std::process::id());
        cleanup(&key);
        assert!(read_json_list::<String>(&key).is_empty(), "未写入时不应命中");

        update_json_list::<String, _, ()>(&key, |list| {
            list.push("sensitive-token-123".to_string());
            Ok(())
        })
        .expect("写入应成功");
        assert_eq!(
            read_json_list::<String>(&key),
            vec!["sensitive-token-123".to_string()],
            "读取应还原列表"
        );
        // 文件内容不得出现明文（Windows DPAPI 密文；非 Windows 也独立于 TOML）
        if let Ok(content) = std::fs::read_to_string(secrets_path()) {
            assert!(
                !content.contains("sensitive-token-123"),
                "secrets.bin 不应含明文 token"
            );
        }

        // 清空 = 删除语义（条目不再命中）
        update_json_list::<String, _, ()>(&key, |list| {
            list.clear();
            Ok(())
        })
        .expect("清空应成功");
        assert!(read_json_list::<String>(&key).is_empty(), "清空后不应命中");
    }

    /// Windows DPAPI roundtrip（当前用户上下文）。
    #[cfg(target_os = "windows")]
    #[test]
    fn dpapi_roundtrip() {
        let data = b"ms-refresh-token-secret";
        let enc = dpapi::encrypt(data).expect("加密应成功");
        assert_ne!(enc, data, "密文不应等于明文");
        assert_eq!(
            dpapi::decrypt(&enc).expect("解密应成功"),
            data,
            "DPAPI 往返应还原原文"
        );
    }

    /// Linux Secret Service 钥匙串加密 roundtrip。
    /// 无 Secret Service 环境（headless/未解锁）降级明文，roundtrip 依然成立。
    #[cfg(target_os = "linux")]
    #[test]
    fn key_utils_roundtrip() {
        let data = b"ms-refresh-token-secret";
        let enc = encrypt_bytes(data).expect("加密应成功");
        assert_eq!(
            decrypt_bytes(&enc).expect("解密应成功"),
            data,
            "往返应还原原文"
        );
    }

    /// macOS Keychain 钥匙串加密 roundtrip（语义同 key_utils_roundtrip）。
    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_roundtrip() {
        let data = b"ms-refresh-token-secret";
        let enc = encrypt_bytes(data).expect("加密应成功");
        assert_eq!(
            decrypt_bytes(&enc).expect("解密应成功"),
            data,
            "往返应还原原文"
        );
    }
}
