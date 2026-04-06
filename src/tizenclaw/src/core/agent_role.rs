//! Agent role — defines agent roles/personas with system prompts and tool restrictions.

use crate::core::prompt_builder::{PromptMode, ReasoningPolicy};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct AgentRole {
    pub name: String,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
    pub max_iterations: usize,
    pub description: String,
    pub prompt_mode: Option<PromptMode>,
    pub reasoning_policy: Option<ReasoningPolicy>,
}

pub struct AgentRoleRegistry {
    roles: HashMap<String, AgentRole>,
    dynamic_roles: HashMap<String, AgentRole>,
}

impl Default for AgentRoleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRoleRegistry {
    pub fn new() -> Self {
        AgentRoleRegistry {
            roles: HashMap::new(),
            dynamic_roles: HashMap::new(),
        }
    }

    pub fn load_roles(&mut self, path: &str) -> bool {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let config: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if let Some(roles) = config["roles"].as_array() {
            for r in roles {
                let name = r["name"].as_str().unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }
                let allowed: Vec<String> = r["allowed_tools"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                self.roles.insert(
                    name.clone(),
                    AgentRole {
                        name,
                        system_prompt: r["system_prompt"].as_str().unwrap_or("").to_string(),
                        allowed_tools: allowed,
                        max_iterations: r["max_iterations"].as_u64().unwrap_or(10) as usize,
                        description: r["description"].as_str().unwrap_or("").to_string(),
                        prompt_mode: parse_prompt_mode(r.get("prompt_mode")),
                        reasoning_policy: parse_reasoning_policy(r.get("reasoning_policy")),
                    },
                );
            }
        }
        log::info!("AgentRoleRegistry: loaded {} roles", self.roles.len());
        true
    }

    pub fn ensure_builtin_roles(&mut self) {
        for role in builtin_roles() {
            self.roles.entry(role.name.clone()).or_insert(role);
        }
    }

    pub fn get_role(&self, name: &str) -> Option<&AgentRole> {
        self.roles
            .get(name)
            .or_else(|| self.dynamic_roles.get(name))
    }

    pub fn get_role_names(&self) -> Vec<String> {
        self.roles
            .keys()
            .chain(self.dynamic_roles.keys())
            .cloned()
            .collect()
    }

    pub fn add_dynamic_role(&mut self, role: AgentRole) {
        log::debug!("Added dynamic role: {}", role.name);
        self.dynamic_roles.insert(role.name.clone(), role);
    }

    pub fn remove_dynamic_role(&mut self, name: &str) -> bool {
        self.dynamic_roles.remove(name).is_some()
    }
}

fn parse_prompt_mode(value: Option<&Value>) -> Option<PromptMode> {
    match value.and_then(Value::as_str).map(|value| value.trim()) {
        Some("full") => Some(PromptMode::Full),
        Some("minimal") => Some(PromptMode::Minimal),
        _ => None,
    }
}

fn parse_reasoning_policy(value: Option<&Value>) -> Option<ReasoningPolicy> {
    match value.and_then(Value::as_str).map(|value| value.trim()) {
        Some("native") => Some(ReasoningPolicy::Native),
        Some("tagged") => Some(ReasoningPolicy::Tagged),
        _ => None,
    }
}

fn builtin_roles() -> Vec<AgentRole> {
    vec![
        AgentRole {
            name: "default".into(),
            system_prompt:
                "You are TizenClaw's default generalist agent. Solve end-user requests directly, using tools when needed.".into(),
            allowed_tools: Vec::new(),
            max_iterations: 10,
            description: "Balanced default role for general requests.".into(),
            prompt_mode: Some(PromptMode::Full),
            reasoning_policy: Some(ReasoningPolicy::Native),
        },
        AgentRole {
            name: "subagent".into(),
            system_prompt:
                "You are a focused sub-agent. Stay narrow, execute only the assigned task, and return concise progress or results.".into(),
            allowed_tools: Vec::new(),
            max_iterations: 6,
            description: "Focused role for delegated or background tasks.".into(),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Native),
        },
        AgentRole {
            name: "local-reasoner".into(),
            system_prompt:
                "You are a local-backend helper. Prefer short plans, compact tool usage, and backend-safe formatting.".into(),
            allowed_tools: Vec::new(),
            max_iterations: 6,
            description: "Minimal profile optimized for local or constrained backends.".into(),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Tagged),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_role(name: &str) -> AgentRole {
        AgentRole {
            name: name.to_string(),
            system_prompt: format!("You are {}", name),
            allowed_tools: vec!["test_tool".into()],
            max_iterations: 10,
            description: format!("{} role", name),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Tagged),
        }
    }

    #[test]
    fn test_add_dynamic_role() {
        let mut reg = AgentRoleRegistry::new();
        reg.add_dynamic_role(sample_role("coder"));
        assert!(reg.get_role("coder").is_some());
    }

    #[test]
    fn test_get_nonexistent_role() {
        let reg = AgentRoleRegistry::new();
        assert!(reg.get_role("nope").is_none());
    }

    #[test]
    fn test_get_role_names() {
        let mut reg = AgentRoleRegistry::new();
        reg.add_dynamic_role(sample_role("coder"));
        reg.add_dynamic_role(sample_role("tester"));
        let names = reg.get_role_names();
        assert!(names.contains(&"coder".to_string()));
        assert!(names.contains(&"tester".to_string()));
    }

    #[test]
    fn test_remove_dynamic_role() {
        let mut reg = AgentRoleRegistry::new();
        reg.add_dynamic_role(sample_role("temp"));
        assert!(reg.remove_dynamic_role("temp"));
        assert!(!reg.remove_dynamic_role("temp"));
        assert!(reg.get_role("temp").is_none());
    }

    #[test]
    fn test_role_fields() {
        let mut reg = AgentRoleRegistry::new();
        reg.add_dynamic_role(sample_role("analyst"));
        let r = reg.get_role("analyst").unwrap();
        assert_eq!(r.max_iterations, 10);
        assert!(r.allowed_tools.contains(&"test_tool".to_string()));
        assert_eq!(r.prompt_mode, Some(PromptMode::Minimal));
        assert_eq!(r.reasoning_policy, Some(ReasoningPolicy::Tagged));
    }

    #[test]
    fn test_empty_registry() {
        let reg = AgentRoleRegistry::new();
        assert!(reg.get_role_names().is_empty());
    }

    #[test]
    fn test_builtin_roles_seeded() {
        let mut reg = AgentRoleRegistry::new();
        reg.ensure_builtin_roles();
        assert!(reg.get_role("default").is_some());
        assert!(reg.get_role("subagent").is_some());
        assert!(reg.get_role("local-reasoner").is_some());
    }
}
