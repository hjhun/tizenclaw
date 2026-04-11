use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub enabled: bool,
    pub profile_name: String,
    pub writable_roots: Vec<String>,
    pub network_access: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            profile_name: "workspace-write".to_string(),
            writable_roots: vec![".".to_string()],
            network_access: false,
        }
    }
}
