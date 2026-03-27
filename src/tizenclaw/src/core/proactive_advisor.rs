//! Proactive advisor — generates proactive suggestions based on context.

use serde_json::{json, Value};

pub struct ProactiveAdvisor;

impl ProactiveAdvisor {
    pub fn new() -> Self { ProactiveAdvisor }

    pub fn generate_suggestions(&self, context: &Value) -> Vec<String> {
        let mut suggestions = vec![];
        if let Some(battery) = context.get("battery_level").and_then(|v| v.as_str()) {
            if let Ok(level) = battery.parse::<u32>() {
                if level < 20 {
                    suggestions.push("Battery is low. Consider enabling power saving mode.".into());
                }
            }
        }
        if let Some(false) = context.get("network_available").and_then(|v| v.as_bool()) {
            suggestions.push("Network is offline. Some features may be limited.".into());
        }
        suggestions
    }
}
