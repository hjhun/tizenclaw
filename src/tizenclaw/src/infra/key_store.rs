//! Key store — manages API keys from config file.
//!
//! Uses serde_json for config parsing.

use serde_json::Value;
use std::collections::HashMap;

pub struct KeyStore {
    keys: HashMap<String, String>,
    config_path: String,
}

impl KeyStore {
    pub fn new() -> Self {
        KeyStore {
            keys: HashMap::new(),
            config_path: String::new(),
        }
    }

    /// Load keys from a JSON config file.
    pub fn load(&mut self, config_path: &str) -> bool {
        self.config_path = config_path.to_string();
        let content = match std::fs::read_to_string(config_path) {
            Ok(c) => c,
            Err(_) => return false,
        };
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

    /// Save keys back to disk.
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
        match serde_json::to_string_pretty(&json) {
            Ok(s) => std::fs::write(&self.config_path, s).is_ok(),
            Err(_) => false,
        }
    }
}
