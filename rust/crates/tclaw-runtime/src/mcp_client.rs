use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct McpClientSpec {
    pub client_name: String,
    pub version: String,
    pub capabilities: Vec<String>,
}
