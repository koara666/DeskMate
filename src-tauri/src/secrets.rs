//! API Key 存储：只放在系统凭据管理器里（Windows 凭据管理器 / macOS 钥匙串）。
//! 规则：Key 不落盘、不进日志、不传给前端（前端只拿到 `mask_key` 的结果）。

use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "DeskMate";

pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

fn account(id: &str) -> String {
    format!("backend:{id}")
}

/// 系统凭据管理器
pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        let e = keyring::Entry::new(SERVICE, &account(id)).map_err(|e| e.to_string())?;
        match e.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("读取凭据失败：{e}")),
        }
    }

    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        let e = keyring::Entry::new(SERVICE, &account(id)).map_err(|e| e.to_string())?;
        e.set_password(secret)
            .map_err(|e| format!("保存凭据失败：{e}"))
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        let e = keyring::Entry::new(SERVICE, &account(id)).map_err(|e| e.to_string())?;
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("删除凭据失败：{e}")),
        }
    }
}

/// 内存存储（测试用）
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned())
    }
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), secret.to_string());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        Ok(())
    }
}

/// 校验并清理用户粘贴进来的 Key（去掉首尾空白；拒绝中间有空白、控制字符或过长的值）
pub fn normalize_key(raw: &str) -> Result<String, String> {
    let k = raw.trim();
    if k.is_empty() {
        return Err("Key 不能为空".into());
    }
    if k.len() > 512 {
        return Err("Key 太长了，确认是否粘贴错了".into());
    }
    if !k.chars().all(|c| c.is_ascii_graphic()) {
        return Err("Key 里有空格、中文或其他不支持的字符，确认是否粘贴错了".into());
    }
    Ok(k.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(normalize_key("  abc-123_XYZ \n").unwrap(), "abc-123_XYZ");
        assert!(normalize_key("").is_err());
        assert!(normalize_key("abc def").is_err());
        assert!(normalize_key("密钥").is_err());
        assert!(normalize_key(&"a".repeat(600)).is_err());
    }

    #[test]
    fn memory_store_roundtrip() {
        let s = MemoryStore::default();
        assert_eq!(s.get("x").unwrap(), None);
        s.set("x", "k1").unwrap();
        assert_eq!(s.get("x").unwrap().as_deref(), Some("k1"));
        s.delete("x").unwrap();
        s.delete("x").unwrap();
        assert_eq!(s.get("x").unwrap(), None);
    }
}
