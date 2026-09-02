//! Secrets the user must not hand-edit.
//!
//! The Director API key is one: it belongs in the OS secret store, not in
//! `settings.json`, where a sync or a backup would copy it in plain text.
//! `SecretStore` is the seam — `MemoryStore` in tests, `KeyringStore` in the
//! process — so later tasks can read and write the key without touching Keychain
//! from `#[cfg(test)]`.

#![expect(dead_code)] // Task 4 wires KeyringStore; Tasks 2–3 consume the seam.

use std::collections::HashMap;
use std::sync::Mutex;

use keyring::Entry;

/// Account name for the Director API key in the OS secret store.
pub const DIRECTOR_API_KEY: &str = "director-api-key";

/// Read and write secrets without naming where they live.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// In-memory store for tests. Never touches Keychain.
pub struct MemoryStore {
    inner: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let map = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(map.get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        let mut map = self.inner.lock().map_err(|e| e.to_string())?;
        map.insert(account.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let mut map = self.inner.lock().map_err(|e| e.to_string())?;
        map.remove(account);
        Ok(())
    }
}

/// OS secret store via `keyring`. Constructed only at process startup.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self {
            service: "ai-buddy".to_string(),
        }
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let entry = Entry::new(&self.service, account).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        let entry = Entry::new(&self.service, account).map_err(|e| e.to_string())?;
        entry.set_password(value).map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = Entry::new(&self.service, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_item_is_none_not_an_error() {
        let store = MemoryStore::new();
        assert_eq!(store.get(DIRECTOR_API_KEY).unwrap(), None);
    }

    #[test]
    fn a_set_item_is_what_get_returns() {
        let store = MemoryStore::new();
        store.set(DIRECTOR_API_KEY, "sk-test-key").unwrap();
        assert_eq!(
            store.get(DIRECTOR_API_KEY).unwrap().as_deref(),
            Some("sk-test-key")
        );
    }

    #[test]
    fn deleting_an_item_leaves_none() {
        let store = MemoryStore::new();
        store.set(DIRECTOR_API_KEY, "sk-test-key").unwrap();
        store.delete(DIRECTOR_API_KEY).unwrap();
        assert_eq!(store.get(DIRECTOR_API_KEY).unwrap(), None);
    }

    #[test]
    fn deleting_a_missing_item_is_ok() {
        let store = MemoryStore::new();
        store.delete(DIRECTOR_API_KEY).unwrap();
    }
}
