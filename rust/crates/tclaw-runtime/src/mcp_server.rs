use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct McpServerRegistration {
    pub server_name: String,
    pub transport: String,
    pub exposed_tools: Vec<String>,
}
