//! Key store — manages API keys from config file.
//!
//! Uses serde_json for config parsing and openssl for local AES-256-CBC encryption.

use serde_json::Value;
use std::collections::HashMap;
use openssl::symm::{encrypt, decrypt, Cipher};

const FIXED_KEY: &[u8; 32] = b"tizenclaw_generic_secure_key1234";
const FIXED_IV: &[u8; 16] = b"tizenclaw_iv5678";

pub struct KeyStore {
    keys: HashMap<String, String>,
    config_path: String,
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore {
    pub fn new() -> Self {
        KeyStore {
            keys: HashMap::new(),
            config_path: String::new(),
        }
    }

    /// Load keys from an encrypted JSON config file.
    pub fn load(&mut self, config_path: &str) -> bool {
        self.config_path = config_path.to_string();
        let encrypted_content = match std::fs::read(config_path) {
            Ok(c) => c,
            Err(_) => return false,
        };
        
        // Decrypt the file
        let cipher = Cipher::aes_256_cbc();
        let decrypted_bytes = match decrypt(cipher, FIXED_KEY, Some(FIXED_IV), &encrypted_content) {
            Ok(b) => b,
            Err(_) => return false,
        };
        
        let content = String::from_utf8_lossy(&decrypted_bytes);

        match serde_json::from_str::<Value>(&content) {
            Ok(val) => {
                if let Some(obj) = val.as_object() {
                    for (k, v) in obj {
                        if let Some(s) = v.as_str() {
                            self.keys.insert(k.clone(), s.to_string());
                        }
                    }
                }
                true
            }
            Err(_) => false,
        }
    }

    /// Get a key by name. Environment variables take priority.
    pub fn get(&self, name: &str) -> Option<String> {
        if let Ok(val) = std::env::var(name) {
            if !val.is_empty() {
                return Some(val);
            }
        }
        self.keys.get(name).cloned()
    }

    /// Set a key (in memory only).
    pub fn set(&mut self, name: &str, value: &str) {
        self.keys.insert(name.to_string(), value.to_string());
    }

    /// Save keys back to disk, encrypted with AES-256-CBC.
    pub fn save(&self) -> bool {
        if self.config_path.is_empty() {
            return false;
        }
        let map: serde_json::Map<String, Value> = self
            .keys
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let json = Value::Object(map);
        let content = match serde_json::to_string_pretty(&json) {
            Ok(s) => s,
            Err(_) => return false,
        };
        
        // Encrypt the file content
        let cipher = Cipher::aes_256_cbc();
        let encrypted_bytes = match encrypt(cipher, FIXED_KEY, Some(FIXED_IV), content.as_bytes()) {
            Ok(b) => b,
            Err(_) => return false,
        };
        
        std::fs::write(&self.config_path, encrypted_bytes).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_generic_storage() {
        let mut ks = KeyStore::new();
        ks.config_path = "/tmp/tizenclaw_test_keys.enc".to_string();
        
        ks.set("test_key", "secret123");
        assert!(ks.save());
        
        let mut ks2 = KeyStore::new();
        assert!(ks2.load(&ks.config_path));
        assert_eq!(ks2.get("test_key").unwrap(), "secret123");
        
        let _ = std::fs::remove_file(&ks.config_path);
    }
}
