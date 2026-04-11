use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct McpRuntimeState {
    pub configured_servers: Vec<String>,
    pub connected_servers: Vec<String>,
    pub last_error: Option<String>,
}
