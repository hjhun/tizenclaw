use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StdioTransportMode {
    Stdio,
    Pty,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpStdioServerSpec {
    pub server_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<String>,
    pub transport: StdioTransportMode,
    pub autostart: bool,
}

impl Default for McpStdioServerSpec {
    fn default() -> Self {
        Self {
            server_name: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            transport: StdioTransportMode::Stdio,
            autostart: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_server_spec_serializes_env_map() {
        let mut spec = McpStdioServerSpec::default();
        spec.server_name = "github".to_string();
        spec.command = "gh-mcp".to_string();
        spec.env.insert("TOKEN".to_string(), "redacted".to_string());

        let json = serde_json::to_string(&spec).expect("serialize server spec");
        let restored: McpStdioServerSpec =
            serde_json::from_str(&json).expect("deserialize server spec");

        assert_eq!(restored.server_name, "github");
        assert_eq!(restored.transport, StdioTransportMode::Stdio);
        assert_eq!(restored.env.get("TOKEN"), Some(&"redacted".to_string()));
    }
}
