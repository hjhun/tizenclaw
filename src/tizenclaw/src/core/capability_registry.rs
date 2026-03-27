//! Capability registry — tracks available agent capabilities and features.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Capability {
    pub name: String,
    pub description: String,
    pub provider: String,
    pub enabled: bool,
}

pub struct CapabilityRegistry {
    capabilities: HashMap<String, Capability>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        CapabilityRegistry { capabilities: HashMap::new() }
    }

    pub fn register(&mut self, cap: Capability) {
        log::info!("CapabilityRegistry: registered '{}'", cap.name);
        self.capabilities.insert(cap.name.clone(), cap);
    }

    pub fn unregister(&mut self, name: &str) {
        self.capabilities.remove(name);
    }

    pub fn is_available(&self, name: &str) -> bool {
        self.capabilities.get(name).map(|c| c.enabled).unwrap_or(false)
    }

    pub fn get_all(&self) -> Vec<&Capability> {
        self.capabilities.values().collect()
    }

    pub fn get_enabled(&self) -> Vec<&Capability> {
        self.capabilities.values().filter(|c| c.enabled).collect()
    }

    pub fn enable(&mut self, name: &str) -> bool {
        if let Some(c) = self.capabilities.get_mut(name) {
            c.enabled = true; true
        } else { false }
    }

    pub fn disable(&mut self, name: &str) -> bool {
        if let Some(c) = self.capabilities.get_mut(name) {
            c.enabled = false; true
        } else { false }
    }

    /// Auto-register capabilities based on available system features.
    pub fn detect_system_capabilities(&mut self) {
        // Code execution
        if std::path::Path::new("/usr/bin/python3").exists() {
            self.register(Capability {
                name: "code_execution".into(),
                description: "Python code execution on device".into(),
                provider: "python3".into(),
                enabled: true,
            });
        }

        // File management
        if std::path::Path::new("/usr/bin/tizenclaw-file-manager-cli").exists() {
            self.register(Capability {
                name: "file_management".into(),
                description: "File system operations".into(),
                provider: "tizenclaw-file-manager-cli".into(),
                enabled: true,
            });
        }

        // Network
        self.register(Capability {
            name: "network".into(),
            description: "HTTP/HTTPS network access".into(),
            provider: "ureq".into(),
            enabled: true,
        });

        // Storage
        self.register(Capability {
            name: "persistent_storage".into(),
            description: "SQLite persistent storage".into(),
            provider: "rusqlite".into(),
            enabled: true,
        });

        log::info!("CapabilityRegistry: detected {} system capabilities",
            self.capabilities.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(name: &str, enabled: bool) -> Capability {
        Capability {
            name: name.to_string(),
            description: format!("{} capability", name),
            provider: "test".into(),
            enabled,
        }
    }

    #[test]
    fn test_register_and_get_all() {
        let mut reg = CapabilityRegistry::new();
        reg.register(cap("code_exec", true));
        reg.register(cap("network", true));
        assert_eq!(reg.get_all().len(), 2);
    }

    #[test]
    fn test_is_available() {
        let mut reg = CapabilityRegistry::new();
        reg.register(cap("code_exec", true));
        reg.register(cap("gpu", false));
        assert!(reg.is_available("code_exec"));
        assert!(!reg.is_available("gpu"));
        assert!(!reg.is_available("nonexistent"));
    }

    #[test]
    fn test_enable_disable() {
        let mut reg = CapabilityRegistry::new();
        reg.register(cap("feat", false));
        assert!(!reg.is_available("feat"));
        assert!(reg.enable("feat"));
        assert!(reg.is_available("feat"));
        assert!(reg.disable("feat"));
        assert!(!reg.is_available("feat"));
    }

    #[test]
    fn test_enable_nonexistent() {
        let mut reg = CapabilityRegistry::new();
        assert!(!reg.enable("nope"));
    }

    #[test]
    fn test_unregister() {
        let mut reg = CapabilityRegistry::new();
        reg.register(cap("code_exec", true));
        reg.unregister("code_exec");
        assert!(!reg.is_available("code_exec"));
        assert_eq!(reg.get_all().len(), 0);
    }

    #[test]
    fn test_get_enabled() {
        let mut reg = CapabilityRegistry::new();
        reg.register(cap("a", true));
        reg.register(cap("b", false));
        reg.register(cap("c", true));
        assert_eq!(reg.get_enabled().len(), 2);
    }
}

