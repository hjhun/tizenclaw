//! Tool router — resolves tool name aliases and routes calls.

use std::collections::HashMap;
use serde_json::Value;

pub struct ToolRouter {
    aliases: HashMap<String, String>,
}

impl ToolRouter {
    pub fn new() -> Self {
        ToolRouter { aliases: HashMap::new() }
    }

    /// Load aliases from JSON config.
    pub fn load_aliases(&mut self, config: &Value) {
        if let Some(obj) = config.as_object() {
            for (alias, target) in obj {
                if let Some(t) = target.as_str() {
                    self.aliases.insert(alias.clone(), t.to_string());
                }
            }
        }
        if !self.aliases.is_empty() {
            log::info!("ToolRouter: loaded {} aliases", self.aliases.len());
        }
    }

    /// Resolve a tool name through aliases.
    pub fn resolve(&self, name: &str) -> String {
        self.aliases.get(name).cloned().unwrap_or_else(|| name.to_string())
    }

    /// Check if a name is an alias.
    pub fn is_alias(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }
}
