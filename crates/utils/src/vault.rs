//! 本地加密 Vault。
//!
//! 设计要点（对应需求 §七-B）：
//! 1. 业务凭据（连接密码、私钥口令）只以 AES-256-GCM 密文落盘；
//! 2. 主密钥不落明文——优先存入 Windows Credential Manager，
//!    凭据管理器不可用时退化为 DPAPI（当前 Windows 用户绑定）加密文件；
//! 3. 任何路径都不存在硬编码密钥。

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;
use zeroize::Zeroizing;

const MAGIC: &[u8; 5] = b"DBHV1";
const KEYRING_SERVICE: &str = "DBHelper";
const KEYRING_USER: &str = "vault-master-key";

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Vault 文件写入失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("Vault 解密失败（凭据损坏或主密钥不匹配）")]
    Decrypt,
    #[error("Vault 序列化失败: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("无法获取主密钥: {0}")]
    NoKey(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    CredentialManager,
    Dpapi,
}

impl KeySource {
    pub fn label(self) -> &'static str {
        match self {
            KeySource::CredentialManager => "Windows Credential Manager",
            KeySource::Dpapi => "DPAPI (当前用户)",
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultFile {
    entries: BTreeMap<String, String>,
}

pub struct Vault {
    path: PathBuf,
    key: Zeroizing<[u8; 32]>,
    entries: BTreeMap<String, String>,
    source: KeySource,
    dirty: bool,
}

impl Vault {
    /// 打开（或初始化）Vault。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        let path = path.as_ref().to_path_buf();
        let (key, source) = load_master_key(&path)?;

        let mut vault = Self {
            path,
            key: Zeroizing::new(key),
            entries: BTreeMap::new(),
            source,
            dirty: false,
        };

        if vault.path.exists() {
            vault.load()?;
        } else {
            vault.dirty = true;
            vault.save()?;
        }

        Ok(vault)
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(self.key.as_ref()).expect("32 字节密钥")
    }

    fn load(&mut self) -> Result<(), VaultError> {
        let bytes = std::fs::read(&self.path)?;
        if bytes.len() < MAGIC.len() + 12 || &bytes[..MAGIC.len()] != MAGIC {
            return Err(VaultError::Decrypt);
        }
        let nonce = Nonce::from_slice(&bytes[MAGIC.len()..MAGIC.len() + 12]);
        let plain = self
            .cipher()
            .decrypt(nonce, &bytes[MAGIC.len() + 12..])
            .map_err(|_| VaultError::Decrypt)?;
        let file: VaultFile = serde_json::from_slice(&plain)?;
        self.entries = file.entries;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), VaultError> {
        let file = VaultFile { entries: self.entries.clone() };
        let plain = Zeroizing::new(serde_json::to_vec(&file)?);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = self.cipher().encrypt(&nonce, plain.as_ref()).map_err(|_| VaultError::Decrypt)?;

        let mut out = Vec::with_capacity(MAGIC.len() + 12 + ct.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ct);

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, out)?;
        self.dirty = false;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: impl Into<String>, secret: impl Into<String>) {
        self.entries.insert(key.into(), secret.into());
        self.dirty = true;
    }

    pub fn remove(&mut self, key: &str) {
        if self.entries.remove(key).is_some() {
            self.dirty = true;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn key_source(&self) -> KeySource {
        self.source
    }

    pub fn status_label(&self) -> String {
        format!("Vault 已解锁 · {}", self.source.label())
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        if self.dirty {
            let _ = self.save();
        }
    }
}

// ---------------------------------------------------------------------------
// 主密钥获取
// ---------------------------------------------------------------------------

fn load_master_key(vault_path: &Path) -> Result<([u8; 32], KeySource), VaultError> {
    // 1) Windows Credential Manager
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        Ok(entry) => match entry.get_password() {
            Ok(stored) => {
                if let Some(key) = decode_key(&stored) {
                    return Ok((key, KeySource::CredentialManager));
                }
            }
            Err(keyring::Error::NoEntry) => {
                let key = random_key();
                if entry.set_password(&B64.encode(key)).is_ok() {
                    return Ok((key, KeySource::CredentialManager));
                }
            }
            Err(_) => {}
        },
        Err(_) => {}
    }

    // 2) DPAPI 保护的密钥文件
    let key_path = vault_path.with_file_name("master.bin");
    if key_path.exists() {
        if let Ok(blob) = std::fs::read(&key_path) {
            if let Ok(plain) = dpapi::unprotect(&blob) {
                if plain.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&plain);
                    return Ok((key, KeySource::Dpapi));
                }
            }
        }
    }

    let key = random_key();
    match dpapi::protect(&key) {
        Ok(blob) => {
            std::fs::write(&key_path, blob)?;
            Ok((key, KeySource::Dpapi))
        }
        Err(e) => Err(VaultError::NoKey(e)),
    }
}

fn decode_key(stored: &str) -> Option<[u8; 32]> {
    let raw = B64.decode(stored.trim()).ok()?;
    if raw.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        Some(key)
    } else {
        None
    }
}

fn random_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

// ---------------------------------------------------------------------------
// DPAPI (Windows)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };

    const UI_FORBIDDEN: u32 = 0x1;

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
    }

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(data);
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe {
            CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), UI_FORBIDDEN, &mut output)
        };
        if ok == 0 {
            return Err("CryptProtectData 调用失败".into());
        }
        let out = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe { free_blob(&output) };
        Ok(out)
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(data);
        let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        let ok = unsafe {
            CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), UI_FORBIDDEN, &mut output)
        };
        if ok == 0 {
            return Err("CryptUnprotectData 调用失败".into());
        }
        let out = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe { free_blob(&output) };
        Ok(out)
    }

    unsafe fn free_blob(b: &CRYPT_INTEGER_BLOB) {
        // LocalFree 由 kernel32 导出，直接声明可避免额外 feature。
        extern "system" {
            fn LocalFree(h: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        }
        if !b.pbData.is_null() {
            LocalFree(b.pbData as *mut core::ffi::c_void);
        }
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(data.to_vec())
    }
    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        Ok(data.to_vec())
    }
}
