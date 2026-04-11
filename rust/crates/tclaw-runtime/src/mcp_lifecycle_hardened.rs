use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpLifecyclePolicy {
    pub restart_on_disconnect: bool,
    pub max_restart_attempts: u32,
    pub enforce_handshake_timeout_ms: u64,
}

impl Default for McpLifecyclePolicy {
    fn default() -> Self {
        Self {
            restart_on_disconnect: true,
            max_restart_attempts: 3,
            enforce_handshake_timeout_ms: 5_000,
        }
    }
}
