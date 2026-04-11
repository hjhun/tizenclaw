//! Tool policy — controls tool execution limits, loop detection, risk levels.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub enum RiskLevel {
    Low,
    Normal,
    High,
}

impl RiskLevel {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "low" => RiskLevel::Low,
            "high" => RiskLevel::High,
            _ => RiskLevel::Normal,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Normal => "normal",
            RiskLevel::High => "high",
        }
    }
}

const IDLE_WINDOW_SIZE: usize = 3;

pub struct ToolPolicy {
    max_repeat_count: usize,
    max_iterations: usize,
    blocked_skills: HashSet<String>,
    risk_levels: HashMap<String, RiskLevel>,
    aliases: HashMap<String, String>,
    call_counts: Mutex<HashMap<String, usize>>,
    idle_history: Mutex<HashMap<String, Vec<String>>>,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPolicy {
    pub fn new() -> Self {
        Self {
            max_repeat_count: 0,
            max_iterations: 0,
            blocked_skills: HashSet::new(),
            risk_levels: HashMap::new(),
            aliases: HashMap::new(),
            call_counts: Mutex::new(HashMap::new()),
            idle_history: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_config(config: &Value) -> Self {
        let mut policy = Self::new();

        if let Some(v) = config.get("max_repeat_count").and_then(Value::as_u64) {
            policy.max_repeat_count = v as usize;
        }
        if let Some(v) = config.get("max_iterations").and_then(Value::as_u64) {
            policy.max_iterations = v as usize;
        }
        if let Some(items) = config.get("blocked_skills").and_then(Value::as_array) {
            policy.blocked_skills = items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect();
        }

        let risk_object = config
            .get("risk_levels")
            .and_then(Value::as_object)
            .or_else(|| config.get("risk_overrides").and_then(Value::as_object));
        if let Some(items) = risk_object {
            policy.risk_levels = items
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .as_str()
                        .map(|level| (name.clone(), RiskLevel::from_str(level)))
                })
                .collect();
        }

        if let Some(items) = config.get("aliases").and_then(Value::as_object) {
            policy.aliases = items
                .iter()
                .filter_map(|(alias, value)| {
                    value.as_str().map(|canonical| (alias.clone(), canonical.to_string()))
                })
                .collect();
        }

        policy
    }

    pub fn load_config(&mut self, path: &str) -> bool {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                log::debug!("No tool policy config at {}, using defaults", path);
                return true;
            }
        };
        let parsed: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(err) => {
                log::error!("Failed to parse tool policy: {}", err);
                return false;
            }
        };

        let loaded = Self::from_config(&parsed);
        self.max_repeat_count = loaded.max_repeat_count;
        self.max_iterations = loaded.max_iterations;
        self.blocked_skills = loaded.blocked_skills;
        self.risk_levels = loaded.risk_levels;
        self.aliases = loaded.aliases;
        self.reset();

        log::info!(
            "Tool policy loaded: max_repeat={}, max_iterations={}, blocked={}, aliases={}",
            self.max_repeat_count,
            self.max_iterations,
            self.blocked_skills.len(),
            self.aliases.len()
        );
        true
    }

    pub fn check_policy(
        &self,
        _session_id: &str,
        skill_name: &str,
        _args: &Value,
    ) -> Result<(), String> {
        if self.blocked_skills.contains(skill_name) {
            return Err(format!(
                "Tool '{}' is blocked by security policy.",
                skill_name
            ));
        }

        Ok(())
    }

    pub fn record_call(&self, tool_name: &str) {
        if let Ok(mut counts) = self.call_counts.lock() {
            *counts.entry(tool_name.to_string()).or_insert(0) += 1;
        }
    }

    pub fn is_loop_detected(&self, tool_name: &str) -> bool {
        if self.max_repeat_count == 0 {
            return false;
        }

        self.call_counts
            .lock()
            .ok()
            .and_then(|counts| counts.get(tool_name).copied())
            .map(|count| count >= self.max_repeat_count)
            .unwrap_or(false)
    }

    pub fn total_calls(&self) -> usize {
        self.call_counts
            .lock()
            .map(|counts| counts.values().sum())
            .unwrap_or(0)
    }

    pub fn is_iteration_limit_reached(&self) -> bool {
        self.max_iterations > 0 && self.total_calls() >= self.max_iterations
    }

    pub fn reset(&self) {
        if let Ok(mut counts) = self.call_counts.lock() {
            counts.clear();
        }
    }

    pub fn reset_session(&self, session_id: &str) {
        self.reset();
        self.reset_idle_tracking(session_id);
    }

    pub fn check_idle_progress(&self, session_id: &str, output: &str) -> bool {
        if let Ok(mut history) = self.idle_history.lock() {
            let entries = history.entry(session_id.to_string()).or_default();
            entries.push(output.to_string());
            while entries.len() > IDLE_WINDOW_SIZE {
                entries.remove(0);
            }
            if entries.len() < IDLE_WINDOW_SIZE {
                return false;
            }
            let first = &entries[0];
            entries.iter().all(|entry| entry == first)
        } else {
            false
        }
    }

    pub fn reset_idle_tracking(&self, session_id: &str) {
        if let Ok(mut history) = self.idle_history.lock() {
            history.remove(session_id);
        }
    }

    pub fn get_max_iterations(&self) -> usize {
        self.max_iterations
    }

    pub fn get_aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }

    pub fn get_risk_level(&self, name: &str) -> RiskLevel {
        self.risk_levels
            .get(name)
            .cloned()
            .unwrap_or(RiskLevel::Normal)
    }

    pub fn status_json(&self) -> Value {
        json!({
            "max_repeat_count": self.max_repeat_count,
            "max_iterations": self.max_iterations,
            "current_iteration_count": self.total_calls(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_default_max_iterations() {
        let policy = ToolPolicy::new();
        assert_eq!(policy.get_max_iterations(), 0);
    }

    #[test]
    fn test_default_aliases_empty() {
        let policy = ToolPolicy::new();
        assert!(policy.get_aliases().is_empty());
    }

    #[test]
    fn test_risk_level_from_str() {
        assert_eq!(RiskLevel::from_str("low"), RiskLevel::Low);
        assert_eq!(RiskLevel::from_str("high"), RiskLevel::High);
        assert_eq!(RiskLevel::from_str("normal"), RiskLevel::Normal);
        assert_eq!(RiskLevel::from_str("unknown"), RiskLevel::Normal);
    }

    #[test]
    fn test_risk_level_as_str() {
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Normal.as_str(), "normal");
        assert_eq!(RiskLevel::High.as_str(), "high");
    }

    #[test]
    fn test_check_policy_allows_first_call() {
        let policy = ToolPolicy::new();
        assert!(policy
            .check_policy("s1", "test_tool", &json!({"k": "v"}))
            .is_ok());
    }

    #[test]
    fn test_loop_detected_after_repeat_limit() {
        let mut policy = ToolPolicy::new();
        policy.max_repeat_count = 5;

        for _ in 0..5 {
            policy.record_call("get_battery");
        }

        assert!(policy.is_loop_detected("get_battery"));
    }

    #[test]
    fn test_zero_repeat_limit_means_unlimited() {
        let policy = ToolPolicy::new();
        for _ in 0..32 {
            policy.record_call("t");
        }
        assert!(!policy.is_loop_detected("t"));
    }

    #[test]
    fn test_total_calls_sum_all_recorded_calls() {
        let policy = ToolPolicy::new();
        for _ in 0..3 {
            policy.record_call("a");
        }
        for _ in 0..2 {
            policy.record_call("b");
        }
        assert_eq!(policy.total_calls(), 5);
    }

    #[test]
    fn test_blocked_skill() {
        let mut policy = ToolPolicy::new();
        policy.blocked_skills.insert("danger".to_string());
        assert!(policy.check_policy("s1", "danger", &json!({})).is_err());
    }

    #[test]
    fn test_idle_detection_same_output() {
        let policy = ToolPolicy::new();
        assert!(!policy.check_idle_progress("s1", "same"));
        assert!(!policy.check_idle_progress("s1", "same"));
        assert!(policy.check_idle_progress("s1", "same"));
    }

    #[test]
    fn test_idle_detection_different_output() {
        let policy = ToolPolicy::new();
        assert!(!policy.check_idle_progress("s1", "A"));
        assert!(!policy.check_idle_progress("s1", "B"));
        assert!(!policy.check_idle_progress("s1", "C"));
    }

    #[test]
    fn test_iteration_limit_reached() {
        let mut policy = ToolPolicy::new();
        policy.max_iterations = 4;
        for _ in 0..4 {
            policy.record_call("tick");
        }
        assert!(policy.is_iteration_limit_reached());
    }

    #[test]
    fn test_reset_clears_counts() {
        let policy = ToolPolicy::new();
        for _ in 0..3 {
            policy.record_call("t");
        }
        policy.reset();
        assert_eq!(policy.total_calls(), 0);
        assert!(!policy.is_loop_detected("t"));
    }

    #[test]
    fn test_from_config_parses_supported_fields() {
        let policy = ToolPolicy::from_config(&json!({
            "max_repeat_count": 5,
            "max_iterations": 50,
            "blocked_skills": ["danger"],
            "risk_levels": {
                "format_disk": "high",
                "get_battery": "low"
            },
            "aliases": {
                "battery": "get_battery"
            }
        }));

        assert_eq!(policy.max_repeat_count, 5);
        assert_eq!(policy.max_iterations, 50);
        assert!(policy.blocked_skills.contains("danger"));
        assert_eq!(policy.get_risk_level("format_disk"), RiskLevel::High);
        assert_eq!(policy.get_risk_level("get_battery"), RiskLevel::Low);
        assert_eq!(
            policy.get_aliases().get("battery").map(String::as_str),
            Some("get_battery")
        );
    }

    #[test]
    fn test_default_risk_level() {
        let policy = ToolPolicy::new();
        assert_eq!(policy.get_risk_level("unknown"), RiskLevel::Normal);
    }
}
