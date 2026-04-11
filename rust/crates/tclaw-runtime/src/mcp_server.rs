use serde::{Deserialize, Serialize};

use crate::mcp::{McpResourceDefinition, McpServerInfo, McpToolDefinition};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpServerState {
    Starting,
    Ready,
    Degraded,
    Failed,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct McpServerRegistration {
    pub server_name: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_info: Option<McpServerInfo>,
    #[serde(default)]
    pub state: McpServerState,
    #[serde(default)]
    pub exposed_tools: Vec<String>,
    #[serde(default)]
    pub tools: Vec<McpToolDefinition>,
    #[serde(default)]
    pub resources: Vec<McpResourceDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
}

impl McpServerRegistration {
    pub fn from_catalog(
        server_name: impl Into<String>,
        transport: impl Into<String>,
        server_info: Option<McpServerInfo>,
        state: McpServerState,
        tools: Vec<McpToolDefinition>,
        resources: Vec<McpResourceDefinition>,
        degraded_reason: Option<String>,
    ) -> Self {
        let exposed_tools = tools.iter().map(|tool| tool.name.clone()).collect();
        Self {
            server_name: server_name.into(),
            transport: transport.into(),
            server_info,
            state,
            exposed_tools,
            tools,
            resources,
            degraded_reason,
        }
    }
}
