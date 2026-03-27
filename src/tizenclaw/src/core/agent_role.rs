//! Agent role — defines agent roles/personas with system prompts and tool restrictions.

use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct AgentRole {
    pub name: String,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
    pub max_iterations: usize,
    pub description: String,
}

pub struct AgentRoleRegistry {
    roles: HashMap<String, AgentRole>,
    dynamic_roles: HashMap<String, AgentRole>,
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
                if name.is_empty() { continue; }
                let allowed: Vec<String> = r["allowed_tools"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                self.roles.insert(name.clone(), AgentRole {
                    name,
                    system_prompt: r["system_prompt"].as_str().unwrap_or("").to_string(),
                    allowed_tools: allowed,
                    max_iterations: r["max_iterations"].as_u64().unwrap_or(10) as usize,
                    description: r["description"].as_str().unwrap_or("").to_string(),
                });
            }
        }
        log::info!("AgentRoleRegistry: loaded {} roles", self.roles.len());
        true
    }

    pub fn get_role(&self, name: &str) -> Option<&AgentRole> {
        self.roles.get(name).or_else(|| self.dynamic_roles.get(name))
    }

    pub fn get_role_names(&self) -> Vec<String> {
        self.roles.keys().chain(self.dynamic_roles.keys()).cloned().collect()
    }

    pub fn add_dynamic_role(&mut self, role: AgentRole) {
        log::info!("Added dynamic role: {}", role.name);
        self.dynamic_roles.insert(role.name.clone(), role);
    }

    pub fn remove_dynamic_role(&mut self, name: &str) -> bool {
        self.dynamic_roles.remove(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_role(name: &str) -> AgentRole {
        AgentRole {
            name: name.to_string(),
            system_prompt: format!("You are {}", name),
            allowed_tools: vec!["execute_code".into()],
            max_iterations: 10,
            description: format!("{} role", name),
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
        assert!(r.allowed_tools.contains(&"execute_code".to_string()));
    }

    #[test]
    fn test_empty_registry() {
        let reg = AgentRoleRegistry::new();
        assert!(reg.get_role_names().is_empty());
    }
}

