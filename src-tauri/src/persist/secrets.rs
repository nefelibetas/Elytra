//! ── 敏感凭据存储（DPAPI 加密）：MsAccount token 等离开 Settings.toml ──
//! 安全问题：微软 refresh_token（长期凭证）此前明文存于 Settings.toml 的 ms_account section，
//! 文件泄露 = 账号可被接管。本模块提供独立加密存储：
//! - Windows：CryptProtectData/CryptUnprotectData（DPAPI，绑定当前用户账户，
//!   系统级保护，离线可用，无需额外服务）——Chrome/VS Code 同款方案；
//! - 非 Windows：明文 fallback + Unix 600 权限（独立文件，不再混入可分享的 TOML）。
//!
//! 存储：`Elytra/secrets.bin`（JSON：key → base64(加密字节)）；API 与 persist::read/save
//! 同构（key = 原 section 名），调用方透明替换。

use std::path::PathBuf;
use parking_lot::Mutex;

use base64::Engine;

fn secrets_path() -> PathBuf {
    crate::persist::exe_dir()
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
    let b64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);
    let mut map = load_map();
    map.insert(key.to_string(), b64);
    write_map(&map)
}

/// 读取一条敏感数据（解密）。
pub fn read_secret(key: &str) -> Option<Vec<u8>> {
    let _guard = SECRETS_LOCK.lock();
    let map = load_map();
    let b64 = map.get(key)?;
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .ok()?;
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
    let content =
        serde_json::to_string(map).map_err(|e| format!("序列化 secrets 失败\n{e}"))?;
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::write(&path, content).map_err(|e| format!("写入 secrets 失败\n{e}"))
}

/// 加密（Windows DPAPI / 非 Windows 明文）。
fn encrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        dpapi::encrypt(data)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(data.to_vec())
    }
}

fn decrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        dpapi::decrypt(data)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(data.to_vec())
    }
}

/// Windows DPAPI（CryptProtectData）：零依赖 FFI（链接系统 advapi32）。
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
}
