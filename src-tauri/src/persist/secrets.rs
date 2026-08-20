use parking_lot::Mutex;
use std::path::PathBuf;

use crate::util::exe_dir;
use base64::{engine, Engine};

fn secrets_path() -> PathBuf {
    exe_dir()
        .join(crate::constants::CONFIG_DIR)
        .join("secrets.bin")
}

/// 串行化读写（与 persist 的 PERSIST_LOCK 同语义，避免并发覆盖）。
static SECRETS_LOCK: Mutex<()> = Mutex::new(());

/// 写入一条敏感数据（key → DPAPI 加密字节）。空值删除条目。
pub fn save_secret(key: &str, value: &[u8]) -> Result<(), String> {
    if value.is_empty() {
        return remove_secret(key);
    }
    let _guard = SECRETS_LOCK.lock();
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

/// 删除一条敏感数据。
pub fn remove_secret(key: &str) -> Result<(), String> {
    let _guard = SECRETS_LOCK.lock();
    let mut map = load_map();
    if map.remove(key).is_some() {
        write_map(&map)
    } else {
        Ok(())
    }
}

fn load_map() -> std::collections::HashMap<String, String> {
    let path = secrets_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return std::collections::HashMap::new();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn write_map(map: &std::collections::HashMap<String, String>) -> Result<(), String> {
    let path = secrets_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败\n{e}"))?;
    }
    let content = serde_json::to_string(map).map_err(|e| format!("序列化 secrets 失败\n{e}"))?;
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::write(&path, content).map_err(|e| format!("写入 secrets 失败\n{e}"))
}

/// 加密（Windows DPAPI / Linux-macOS 钥匙串主密钥 ChaCha20 / 钥匙串不可用降级明文）。
fn encrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        dpapi::encrypt(data)
    }
    #[cfg(target_os = "linux")]
    {
        key_utils::encrypt(data)
    }
    #[cfg(target_os = "macos")]
    {
        keychain::encrypt(data)
    }
}

fn decrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        dpapi::decrypt(data)
    }
    #[cfg(target_os = "linux")]
    {
        key_utils::decrypt(data)
    }
    #[cfg(target_os = "macos")]
    {
        keychain::decrypt(data)
    }
}

/// 钥匙串主密钥加密格式前缀（Linux/macOS 共用）：
/// `"ELY1" || nonce(12B) || ChaCha20-Poly1305 密文`。
/// 无前缀 → 明文（钥匙串不可用期间写入的降级数据，解密时原样返回）。
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAGIC: &[u8] = b"ELY1";

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn key_encrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        ChaCha20Poly1305, Key, Nonce,
    };
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| format!("生成随机数失败\n{e}"))?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), data)
        .map_err(|e| format!("加密失败\n{e}"))?;
    let mut out = Vec::with_capacity(MAGIC.len() + nonce_bytes.len() + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn key_decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        ChaCha20Poly1305, Key, Nonce,
    };
    let Some(rest) = data.strip_prefix(MAGIC) else {
        return Ok(data.to_vec());
    };
    if rest.len() < 12 {
        return Err("密文格式异常".into());
    }
    let (nonce, ct) = rest.split_at(12);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce), ct)
        .map_err(|e| format!("解密失败\n{e}"))
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
            .map_err(|e| format!("连接 Secret Service 失败\n{e}"))?;
        let collection = ss
            .get_default_collection()
            .await
            .map_err(|e| format!("获取默认钥匙串失败\n{e}"))?;
        let items = collection
            .search_items(attrs())
            .await
            .map_err(|e| format!("查询钥匙串失败\n{e}"))?;
        let Some(item) = items.first() else {
            return Ok(None);
        };
        let secret = item
            .get_secret()
            .await
            .map_err(|e| format!("读取钥匙串失败\n{e}"))?;
        secret
            .as_slice()
            .try_into()
            .map_err(|_| "主密钥长度异常".to_string())
    }

    async fn store_key(key: &[u8; 32]) -> Result<(), String> {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(|e| format!("连接 Secret Service 失败\n{e}"))?;
        let collection = ss
            .get_default_collection()
            .await
            .map_err(|e| format!("获取默认钥匙串失败\n{e}"))?;
        collection
            .create_item(
                KEYCHAIN_MASTER_KEY_LABEL,
                attrs(),
                key,
                true,
                "application/octet-stream",
            )
            .await
            .map_err(|e| format!("写入钥匙串失败\n{e}"))?;
        Ok(())
    }

    fn master_key() -> Option<&'static [u8; 32]> {
        MASTER_KEY.as_ref()
    }

    pub fn encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
        match master_key() {
            Some(key) => super::key_encrypt(key, data),
            None => Ok(data.to_vec()),
        }
    }

    pub fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
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
            .map_err(|e| format!("读取钥匙串失败\n{e:?}"))?;
        let Some(SearchResult::Data(data)) = results.into_iter().next() else {
            return Ok(None);
        };
        data.as_slice()
            .try_into()
            .map_err(|_| "主密钥长度异常".to_string())
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
        .map_err(|e| format!("写入钥匙串失败\n{e:?}"))
    }

    fn master_key() -> Option<&'static [u8; 32]> {
        MASTER_KEY.as_ref()
    }

    pub fn encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
        match master_key() {
            Some(key) => super::key_encrypt(key, data),
            None => Ok(data.to_vec()),
        }
    }

    pub fn decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
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
        let out = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize).to_vec()
        };
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
        let out = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize).to_vec()
        };
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
    fn cleanup(key: &str) {
        let _ = remove_secret(key);
        // 恢复锁后清理文件级残留
    }

    #[test]
    fn secret_roundtrip_and_remove() {
        let key = format!("test_secret_{}", std::process::id());
        cleanup(&key);
        assert!(read_secret(&key).is_none(), "未写入时不应命中");
        save_secret(&key, b"sensitive-token-123").expect("写入应成功");
        assert_eq!(
            read_secret(&key).as_deref(),
            Some(&b"sensitive-token-123"[..]),
            "读取应还原原始字节"
        );
        // 文件内容不得出现明文（Windows DPAPI 密文；非 Windows 也独立于 TOML）
        if let Ok(content) = std::fs::read_to_string(secrets_path()) {
            assert!(
                !content.contains("sensitive-token-123"),
                "secrets.bin 不应含明文 token"
            );
        }
        remove_secret(&key).expect("删除应成功");
        assert!(read_secret(&key).is_none(), "删除后不应命中");
    }

    /// 空值 = 删除语义（save_secret 空 slice 等价 remove）。
    #[test]
    fn empty_value_removes_entry() {
        let key = format!("test_secret_empty_{}", std::process::id());
        cleanup(&key);
        save_secret(&key, b"v").unwrap();
        save_secret(&key, b"").unwrap();
        assert!(read_secret(&key).is_none());
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
