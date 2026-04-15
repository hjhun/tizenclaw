use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::conversation::{
    ToolCallRequest, ToolDefinition, ToolExecutionOutput, ToolExecutor, ToolRuntimeError,
};
use crate::mcp::{bridged_tool_name, McpRuntimeState};
use crate::mcp_lifecycle_hardened::ManagedMcpServer;
use crate::mcp_server::McpServerState;
use crate::permissions::{PermissionLevel, PermissionScope};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpBridgePolicy {
    pub permission_scope: PermissionScope,
    pub minimum_permission_level: PermissionLevel,
}

impl Default for McpBridgePolicy {
    fn default() -> Self {
        Self {
            permission_scope: PermissionScope::Execute,
            minimum_permission_level: PermissionLevel::Standard,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BridgedMcpToolManifest {
    pub full_name: String,
    pub server_name: String,
    pub original_name: String,
    pub description: String,
    pub input_schema: Value,
    pub permission_scope: PermissionScope,
    pub minimum_permission_level: PermissionLevel,
}

pub struct McpToolBridge {
    policy: McpBridgePolicy,
    servers: BTreeMap<String, ManagedMcpServer>,
    tool_index: BTreeMap<String, BridgedMcpToolManifest>,
}

impl Default for McpToolBridge {
    fn default() -> Self {
        Self::new(McpBridgePolicy::default())
    }
}

impl McpToolBridge {
    pub fn new(policy: McpBridgePolicy) -> Self {
        Self {
            policy,
            servers: BTreeMap::new(),
            tool_index: BTreeMap::new(),
        }
    }

    pub fn register_server(&mut self, server: ManagedMcpServer) {
        self.servers
            .insert(server.server_name().to_string(), server);
        self.rebuild_index();
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_index
            .values()
            .map(|manifest| ToolDefinition {
                name: manifest.full_name.clone(),
                description: manifest.description.clone(),
                permission_scope: manifest.permission_scope.clone(),
                minimum_permission_level: manifest.minimum_permission_level,
            })
            .collect()
    }

    pub fn bridged_tool_manifests(&self) -> Vec<BridgedMcpToolManifest> {
        self.tool_index.values().cloned().collect()
    }

    pub fn execute(
        &mut self,
        call: &ToolCallRequest,
    ) -> Option<Result<ToolExecutionOutput, ToolRuntimeError>> {
        let manifest = self.tool_index.get(&call.name)?.clone();
        let server_name = manifest.server_name;
        let original_name = manifest.original_name;
        let server = self.servers.get_mut(&server_name)?;

        Some(
            server
                .call_tool(&original_name, call.input.clone())
                .map(|result| ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})),
                    summary: Some(format!("MCP tool {} completed", original_name)),
                })
                .map_err(|err| ToolRuntimeError::Execution {
                    tool_name: call.name.clone(),
                    message: err.to_string(),
                }),
        )
    }

    pub fn runtime_state(&self) -> McpRuntimeState {
        let mut state = McpRuntimeState::default();
        state.configured_servers = self.servers.keys().cloned().collect();
        state.bridged_tools = self.tool_index.keys().cloned().collect();

        for (name, server) in &self.servers {
            match server.health().state {
                McpServerState::Ready => state.connected_servers.push(name.clone()),
                McpServerState::Degraded => state.degraded_servers.push(name.clone()),
                McpServerState::Failed => state.failed_servers.push(name.clone()),
                _ => {}
            }

            if state.last_error.is_none() {
                state.last_error = server.health().last_error.clone();
            }
        }

        state
    }

    fn rebuild_index(&mut self) {
        self.tool_index.clear();

        for (server_name, server) in &self.servers {
            let Some(registration) = server.registration() else {
                continue;
            };

            if !server.health().initialized && registration.tools.is_empty() {
                continue;
            }

            for tool in &registration.tools {
                let full_name = bridged_tool_name(server_name, &tool.name);
                self.tool_index.insert(
                    full_name,
                    BridgedMcpToolManifest {
                        full_name: bridged_tool_name(server_name, &tool.name),
                        server_name: server_name.clone(),
                        original_name: tool.name.clone(),
                        description: tool.description.clone(),
                        input_schema: tool.input_schema.clone(),
                        permission_scope: self.policy.permission_scope.clone(),
                        minimum_permission_level: self.policy.minimum_permission_level,
                    },
                );
            }
        }
    }
}

pub struct McpBridgedToolExecutor<T> {
    inner: T,
    bridge: McpToolBridge,
}

impl<T> McpBridgedToolExecutor<T> {
    pub fn new(inner: T, bridge: McpToolBridge) -> Self {
        Self { inner, bridge }
    }

    pub fn bridge(&self) -> &McpToolBridge {
        &self.bridge
    }
}

impl<T> ToolExecutor for McpBridgedToolExecutor<T>
where
    T: ToolExecutor,
{
    fn definitions(&self) -> Vec<ToolDefinition> {
        let mut definitions = self.inner.definitions();
        definitions.extend(self.bridge.tool_definitions());
        definitions
    }

    fn execute(&mut self, call: &ToolCallRequest) -> Result<ToolExecutionOutput, ToolRuntimeError> {
        if let Some(result) = self.bridge.execute(call) {
            return result;
        }

        self.inner.execute(call)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::mcp::{
        JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpInitializeResult,
        McpListResourcesResult, McpListToolsResult, McpPeerCapabilities, McpServerInfo,
        McpToolCallResult, McpToolDefinition,
    };
    use crate::mcp_client::{McpClient, McpClientSpec};
    use crate::mcp_lifecycle_hardened::McpLifecyclePolicy;
    use crate::mcp_stdio::{McpStdioServerSpec, McpTransport, McpTransportError};

    struct ScriptedTransport {
        responses: VecDeque<JsonRpcResponse>,
    }

    impl McpTransport for ScriptedTransport {
        fn send_request(
            &mut self,
            _request: &JsonRpcRequest,
            _timeout: Duration,
        ) -> Result<JsonRpcResponse, McpTransportError> {
            self.responses
                .pop_front()
                .ok_or(McpTransportError::ProcessExited {
                    message: "script exhausted".to_string(),
                })
        }

        fn send_notification(
            &mut self,
            _notification: &JsonRpcNotification,
        ) -> Result<(), McpTransportError> {
            Ok(())
        }

        fn is_running(&mut self) -> bool {
            true
        }

        fn close(&mut self) {}

        fn description(&self) -> String {
            "scripted".to_string()
        }
    }

    struct LocalTools;

    impl ToolExecutor for LocalTools {
        fn definitions(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "local_echo".to_string(),
                description: "Local tool".to_string(),
                permission_scope: PermissionScope::Read,
                minimum_permission_level: PermissionLevel::Low,
            }]
        }

        fn execute(
            &mut self,
            call: &ToolCallRequest,
        ) -> Result<ToolExecutionOutput, ToolRuntimeError> {
            Ok(ToolExecutionOutput {
                tool_call_id: call.id.clone(),
                output: json!({"tool": "local"}),
                summary: None,
            })
        }
    }

    #[test]
    fn bridge_exposes_deterministic_tool_names_and_dispatches() {
        let responses = VecDeque::from(vec![
            JsonRpcResponse::success(
                1,
                serde_json::to_value(McpInitializeResult {
                    protocol_version: crate::mcp::MCP_PROTOCOL_VERSION.to_string(),
                    capabilities: McpPeerCapabilities::default(),
                    server_info: McpServerInfo {
                        name: "fake".to_string(),
                        version: "1.0.0".to_string(),
                    },
                })
                .expect("init value"),
            ),
            JsonRpcResponse::success(
                2,
                serde_json::to_value(McpListToolsResult {
                    tools: vec![McpToolDefinition {
                        name: "echo".to_string(),
                        description: "Echo tool".to_string(),
                        input_schema: json!({"type": "object"}),
                    }],
                })
                .expect("tool list value"),
            ),
            JsonRpcResponse::success(
                3,
                serde_json::to_value(McpListResourcesResult { resources: vec![] })
                    .expect("resource list value"),
            ),
            JsonRpcResponse::success(
                4,
                serde_json::to_value(McpToolCallResult {
                    content: vec![crate::mcp::McpContentBlock::Text {
                        text: "from mcp".to_string(),
                    }],
                    structured_content: Some(json!({"ok": true})),
                    is_error: false,
                })
                .expect("call value"),
            ),
        ]);

        let client = McpClient::from_transport(
            "fake",
            McpClientSpec::default(),
            ScriptedTransport { responses },
        );
        let mut server = ManagedMcpServer::with_client(
            McpStdioServerSpec {
                server_name: "fake".to_string(),
                command: "python3".to_string(),
                ..McpStdioServerSpec::default()
            },
            McpClientSpec::default(),
            McpLifecyclePolicy::default(),
            client,
        );
        server.start().expect("start managed server");

        let mut bridge = McpToolBridge::default();
        bridge.register_server(server);

        let mut executor = McpBridgedToolExecutor::new(LocalTools, bridge);
        let definitions = executor.definitions();
        assert!(definitions.iter().any(|tool| tool.name == "local_echo"));
        assert!(definitions
            .iter()
            .any(|tool| tool.name == "mcp__fake__echo"));

        let result = executor
            .execute(&ToolCallRequest {
                id: "call-1".to_string(),
                name: "mcp__fake__echo".to_string(),
                input: json!({"message": "hi"}),
            })
            .expect("execute bridged tool");
        assert_eq!(result.output["structuredContent"]["ok"], true);
    }
}
