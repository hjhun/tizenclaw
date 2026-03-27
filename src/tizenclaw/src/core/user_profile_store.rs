//! User profile store — manages user profiles for context-aware interactions.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct UserProfile {
    pub user_id: String,
    pub name: String,
    pub role: String,  // "admin", "adult", "child", "guest"
    pub preferences: Value,
}

pub struct UserProfileStore {
    profiles: HashMap<String, UserProfile>,
    active_user: Option<String>,
}

impl UserProfileStore {
    pub fn new() -> Self {
        UserProfileStore {
            profiles: HashMap::new(),
            active_user: None,
        }
    }

    pub fn initialize(&mut self, config_path: &str) {
        let content = match std::fs::read_to_string(config_path) {
            Ok(c) => c,
            Err(_) => {
                // Create default admin profile
                self.profiles.insert("admin_01".into(), UserProfile {
                    user_id: "admin_01".into(),
                    name: "Admin".into(),
                    role: "admin".into(),
                    preferences: json!({}),
                });
                self.active_user = Some("admin_01".into());
                return;
            }
        };

        let config: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        if let Some(users) = config["users"].as_array() {
            for u in users {
                let id = u["user_id"].as_str().unwrap_or("").to_string();
                if id.is_empty() { continue; }
                self.profiles.insert(id.clone(), UserProfile {
                    user_id: id,
                    name: u["name"].as_str().unwrap_or("").to_string(),
                    role: u["role"].as_str().unwrap_or("adult").to_string(),
                    preferences: u.get("preferences").cloned().unwrap_or(json!({})),
                });
            }
        }

        self.active_user = config["active_user"].as_str().map(|s| s.to_string())
            .or_else(|| self.profiles.keys().next().cloned());

        log::info!("UserProfileStore: loaded {} profiles, active={}",
            self.profiles.len(),
            self.active_user.as_deref().unwrap_or("none"));
    }

    pub fn get_active_user(&self) -> Option<&UserProfile> {
        self.active_user.as_ref().and_then(|id| self.profiles.get(id))
    }

    pub fn switch_user(&mut self, user_id: &str) -> bool {
        if self.profiles.contains_key(user_id) {
            self.active_user = Some(user_id.to_string());
            log::info!("Switched to user: {}", user_id);
            true
        } else {
            log::warn!("User not found: {}", user_id);
            false
        }
    }

    pub fn get_all_profiles(&self) -> Vec<&UserProfile> {
        self.profiles.values().collect()
    }

    pub fn format_for_prompt(&self) -> String {
        match self.get_active_user() {
            Some(u) => format!("[User] name={}, role={}", u.name, u.role),
            None => String::new(),
        }
    }
}
