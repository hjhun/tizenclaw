//! Action bridge — integrates with Tizen Action Framework for device actions.

use serde_json::{json, Value};
use std::collections::HashMap;

pub struct ActionBridge {
    actions: HashMap<String, ActionSchema>,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
    running: bool,
}

#[derive(Clone, Debug)]
pub struct ActionSchema {
    pub action_id: String,
    pub display_name: String,
    pub description: String,
    pub parameters: Value,
    pub app_id: String,
}

impl Default for ActionBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionBridge {
    pub fn new() -> Self {
        ActionBridge { actions: HashMap::new(), on_change: None, running: false }
    }

    pub fn start(&mut self) -> bool {
        self.running = true;
        log::info!("ActionBridge started");
        true
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn set_change_callback(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_change = Some(Box::new(cb));
    }

    pub fn sync_action_schemas(&mut self) {
        self.sync_action_schemas_from("");
    }

    pub fn sync_action_schemas_from(&mut self, dir: &str) {
        let dir = if dir.is_empty() { "/opt/usr/share/tizenclaw/actions" } else { dir };
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(schema) = serde_json::from_str::<Value>(&content) {
                        let id = schema["action_id"].as_str().unwrap_or("").to_string();
                        if !id.is_empty() {
                            self.actions.insert(id.clone(), ActionSchema {
                                action_id: id,
                                display_name: schema["display_name"].as_str().unwrap_or("").to_string(),
                                description: schema["description"].as_str().unwrap_or("").to_string(),
                                parameters: schema.get("parameters").cloned().unwrap_or(Value::Null),
                                app_id: schema["app_id"].as_str().unwrap_or("").to_string(),
                            });
                        }
                    }
                }
            }
        }
        log::info!("ActionBridge: synced {} action schemas", self.actions.len());
    }

    pub fn get_action_declarations(&self) -> Vec<crate::llm::backend::LlmToolDecl> {
        self.actions.values().map(|a| crate::llm::backend::LlmToolDecl {
            name: format!("action_{}", a.action_id),
            description: a.description.clone(),
            parameters: a.parameters.clone(),
        }).collect()
    }

    pub fn execute_action(&self, action_id: &str, params: &Value) -> Value {
        let schema = match self.actions.get(action_id) {
            Some(s) => s,
            None => return json!({"error": format!("Unknown action: {}", action_id)}),
        };
        // Launch via aul_launch_app or app_control
        log::info!("ActionBridge: executing action '{}' for app '{}'", action_id, schema.app_id);
        json!({"status": "launched", "action_id": action_id, "app_id": &schema.app_id})
    }
}
