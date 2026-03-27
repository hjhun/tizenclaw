//! Autonomous trigger — triggers agent actions based on system events.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct TriggerRule {
    pub id: String,
    pub event_type: String,
    pub condition: String,
    pub action_prompt: String,
    pub session_id: String,
    pub enabled: bool,
}

pub struct AutonomousTrigger {
    rules: HashMap<String, TriggerRule>,
}

impl AutonomousTrigger {
    pub fn new() -> Self {
        AutonomousTrigger { rules: HashMap::new() }
    }

    pub fn load_config(&mut self, path: &str) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let config: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(rules) = config["triggers"].as_array() {
            for r in rules {
                let id = r["id"].as_str().unwrap_or("").to_string();
                if id.is_empty() { continue; }
                self.rules.insert(id.clone(), TriggerRule {
                    id,
                    event_type: r["event_type"].as_str().unwrap_or("").to_string(),
                    condition: r["condition"].as_str().unwrap_or("").to_string(),
                    action_prompt: r["action_prompt"].as_str().unwrap_or("").to_string(),
                    session_id: r["session_id"].as_str().unwrap_or("autonomous").to_string(),
                    enabled: r["enabled"].as_bool().unwrap_or(true),
                });
            }
        }
        log::info!("AutonomousTrigger: loaded {} rules", self.rules.len());
    }

    pub fn check_event(&self, event_type: &str, data: &Value) -> Vec<&TriggerRule> {
        self.rules.values()
            .filter(|r| r.enabled && r.event_type == event_type)
            .collect()
    }

    pub fn add_rule(&mut self, rule: TriggerRule) {
        self.rules.insert(rule.id.clone(), rule);
    }

    pub fn remove_rule(&mut self, id: &str) -> bool {
        self.rules.remove(id).is_some()
    }

    pub fn list_rules(&self) -> Vec<Value> {
        self.rules.values().map(|r| json!({
            "id": r.id, "event_type": r.event_type,
            "condition": r.condition, "enabled": r.enabled
        })).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rule(id: &str, event_type: &str) -> TriggerRule {
        TriggerRule {
            id: id.into(),
            event_type: event_type.into(),
            condition: String::new(),
            action_prompt: "Do something".into(),
            session_id: "auto".into(),
            enabled: true,
        }
    }

    #[test]
    fn test_add_and_check_event() {
        let mut trigger = AutonomousTrigger::new();
        trigger.add_rule(sample_rule("r1", "AppInstalled"));
        let matches = trigger.check_event("AppInstalled", &json!({}));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "r1");
    }

    #[test]
    fn test_check_event_no_match() {
        let mut trigger = AutonomousTrigger::new();
        trigger.add_rule(sample_rule("r1", "AppInstalled"));
        let matches = trigger.check_event("BatteryLow", &json!({}));
        assert!(matches.is_empty());
    }

    #[test]
    fn test_remove_rule() {
        let mut trigger = AutonomousTrigger::new();
        trigger.add_rule(sample_rule("r1", "AppInstalled"));
        assert!(trigger.remove_rule("r1"));
        assert!(!trigger.remove_rule("r1"));
    }

    #[test]
    fn test_list_rules() {
        let mut trigger = AutonomousTrigger::new();
        trigger.add_rule(sample_rule("r1", "A"));
        trigger.add_rule(sample_rule("r2", "B"));
        let list = trigger.list_rules();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_disabled_rule_not_matched() {
        let mut trigger = AutonomousTrigger::new();
        let mut rule = sample_rule("r1", "AppInstalled");
        rule.enabled = false;
        trigger.add_rule(rule);
        let matches = trigger.check_event("AppInstalled", &json!({}));
        assert!(matches.is_empty());
    }

    #[test]
    fn test_multiple_rules_same_event() {
        let mut trigger = AutonomousTrigger::new();
        trigger.add_rule(sample_rule("r1", "AppInstalled"));
        trigger.add_rule(sample_rule("r2", "AppInstalled"));
        let matches = trigger.check_event("AppInstalled", &json!({}));
        assert_eq!(matches.len(), 2);
    }
}

