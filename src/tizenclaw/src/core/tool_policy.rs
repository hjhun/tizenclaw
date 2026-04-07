//! Tool policy — controls tool execution limits, loop detection, risk levels.

use serde_json::Value;
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

struct PolicyConfig {
    max_repeat_count: usize,
    max_iterations: usize,
    blocked_skills: HashSet<String>,
    risk_levels: HashMap<String, RiskLevel>,
    aliases: HashMap<String, String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            max_repeat_count: 3,
            max_iterations: 0,
            blocked_skills: HashSet::new(),
            risk_levels: HashMap::new(),
            aliases: HashMap::new(),
        }
    }
}

pub struct ToolPolicy {
    config: PolicyConfig,
    call_history: Mutex<HashMap<String, HashMap<String, usize>>>,
    idle_history: Mutex<HashMap<String, Vec<String>>>,
}

const IDLE_WINDOW_SIZE: usize = 3;

impl Default for ToolPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPolicy {
    pub fn new() -> Self {
        ToolPolicy {
            config: PolicyConfig::default(),
            call_history: Mutex::new(HashMap::new()),
            idle_history: Mutex::new(HashMap::new()),
        }
    }

    pub fn load_config(&mut self, path: &str) -> bool {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                log::debug!("No tool policy config at {}, using defaults", path);
                return true;
            }
        };
        let j: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed to parse tool policy: {}", e);
                return false;
            }
        };

        if let Some(v) = j.get("max_repeat_count").and_then(|v| v.as_u64()) {
            self.config.max_repeat_count = v as usize;
        }
        if let Some(v) = j.get("max_iterations").and_then(|v| v.as_u64()) {
            self.config.max_iterations = v as usize;
        }
        if let Some(arr) = j.get("blocked_skills").and_then(|v| v.as_array()) {
            for s in arr {
                if let Some(name) = s.as_str() {
                    self.config.blocked_skills.insert(name.to_string());
                }
            }
        }
        if let Some(obj) = j.get("risk_overrides").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    self.config
                        .risk_levels
                        .insert(k.clone(), RiskLevel::from_str(s));
                }
            }
        }
        if let Some(obj) = j.get("aliases").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    self.config.aliases.insert(k.clone(), s.to_string());
                }
            }
        }
        log::info!(
            "Tool policy loaded: max_repeat={}, blocked={}, aliases={}",
            self.config.max_repeat_count,
            self.config.blocked_skills.len(),
            self.config.aliases.len()
        );
        true
    }

    pub fn check_policy(
        &self,
        session_id: &str,
        skill_name: &str,
        args: &Value,
    ) -> Result<(), String> {
        if self.config.blocked_skills.contains(skill_name) {
            return Err(format!(
                "Tool '{}' is blocked by security policy.",
                skill_name
            ));
        }

        let hash = Self::hash_call(skill_name, args);
        if let Ok(mut history) = self.call_history.lock() {
            let session = history.entry(session_id.to_string()).or_default();
            let count = session.entry(hash).or_insert(0);
            *count += 1;
            if *count > self.config.max_repeat_count {
                return Err(format!(
                    "Tool '{}' with identical arguments called {} times (limit: {}). Blocked to prevent infinite loop.",
                    skill_name, count, self.config.max_repeat_count
                ));
            }
        }
        Ok(())
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
            entries.iter().all(|e| e == first)
        } else {
            false
        }
    }

    pub fn reset_session(&self, session_id: &str) {
        if let Ok(mut h) = self.call_history.lock() {
            h.remove(session_id);
        }
        if let Ok(mut h) = self.idle_history.lock() {
            h.remove(session_id);
        }
    }

    pub fn reset_idle_tracking(&self, session_id: &str) {
        if let Ok(mut h) = self.idle_history.lock() {
            h.remove(session_id);
        }
    }

    pub fn get_max_iterations(&self) -> usize {
        self.config.max_iterations
    }
    pub fn get_aliases(&self) -> &HashMap<String, String> {
        &self.config.aliases
    }
    pub fn get_risk_level(&self, name: &str) -> RiskLevel {
        self.config
            .risk_levels
            .get(name)
            .cloned()
            .unwrap_or(RiskLevel::Normal)
    }

    fn hash_call(name: &str, args: &Value) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let input = format!("{}:{}", name, args);
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        format!("{:x}", hasher.finish())
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
    fn test_check_policy_blocks_repeated() {
        let policy = ToolPolicy::new();
        let args = json!({"key": "same"});
        assert!(policy.check_policy("s1", "t", &args).is_ok());
        assert!(policy.check_policy("s1", "t", &args).is_ok());
        assert!(policy.check_policy("s1", "t", &args).is_ok());
        assert!(policy.check_policy("s1", "t", &args).is_err());
    }

    #[test]
    fn test_different_args_not_blocked() {
        let policy = ToolPolicy::new();
        for i in 0..10 {
            assert!(policy.check_policy("s1", "t", &json!({"i": i})).is_ok());
        }
    }

    #[test]
    fn test_blocked_skill() {
        let mut policy = ToolPolicy::new();
        policy.config.blocked_skills.insert("danger".to_string());
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
    fn test_reset_session() {
        let policy = ToolPolicy::new();
        let args = json!({"k": "v"});
        for _ in 0..3 {
            policy.check_policy("s1", "t", &args).unwrap();
        }
        policy.reset_session("s1");
        assert!(policy.check_policy("s1", "t", &args).is_ok());
    }

    #[test]
    fn test_separate_sessions() {
        let policy = ToolPolicy::new();
        let args = json!({"k": "v"});
        for _ in 0..3 {
            policy.check_policy("s1", "t", &args).unwrap();
        }
        assert!(policy.check_policy("s2", "t", &args).is_ok());
    }

    #[test]
    fn test_default_risk_level() {
        let policy = ToolPolicy::new();
        assert_eq!(policy.get_risk_level("unknown"), RiskLevel::Normal);
    }
}
