use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mcp::{McpReadResourceResult, McpToolCallResult};
use crate::mcp_client::{McpClient, McpClientError, McpClientSpec};
use crate::mcp_server::{McpServerRegistration, McpServerState};
use crate::mcp_stdio::McpStdioServerSpec;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpLifecyclePolicy {
    pub restart_on_disconnect: bool,
    pub max_restart_attempts: u32,
    pub enforce_handshake_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub allow_degraded_startup: bool,
}

impl Default for McpLifecyclePolicy {
    fn default() -> Self {
        Self {
            restart_on_disconnect: true,
            max_restart_attempts: 3,
            enforce_handshake_timeout_ms: 5_000,
            request_timeout_ms: 5_000,
            allow_degraded_startup: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct McpServerHealth {
    #[serde(default)]
    pub state: McpServerState,
    #[serde(default)]
    pub restart_attempts: u32,
    #[serde(default)]
    pub initialized: bool,
    #[serde(default)]
    pub tool_count: usize,
    #[serde(default)]
    pub resource_count: usize,
    #[serde(default)]
    pub stderr_tail: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
}

pub struct ManagedMcpServer {
    spec: McpStdioServerSpec,
    client_spec: McpClientSpec,
    policy: McpLifecyclePolicy,
    health: McpServerHealth,
    registration: Option<McpServerRegistration>,
    client: Option<McpClient>,
}

impl ManagedMcpServer {
    pub fn new(
        spec: McpStdioServerSpec,
        client_spec: McpClientSpec,
        policy: McpLifecyclePolicy,
    ) -> Self {
        Self {
            spec,
            client_spec,
            policy,
            health: McpServerHealth::default(),
            registration: None,
            client: None,
        }
    }

    pub fn with_client(
        spec: McpStdioServerSpec,
        client_spec: McpClientSpec,
        policy: McpLifecyclePolicy,
        client: McpClient,
    ) -> Self {
        Self {
            spec,
            client_spec,
            policy,
            health: McpServerHealth::default(),
            registration: None,
            client: Some(client),
        }
    }

    pub fn start(&mut self) -> Result<(), McpClientError> {
        self.health.state = McpServerState::Starting;

        if self.client.is_none() {
            match McpClient::connect_stdio(self.spec.clone(), self.client_spec.clone()) {
                Ok(client) => self.client = Some(client),
                Err(err) => return self.handle_start_error(err),
            }
        }

        let client = match self.client.as_mut() {
            Some(client) => client,
            None => {
                return self.handle_start_error(McpClientError::Protocol {
                    message: "client unavailable".to_string(),
                });
            }
        };

        if let Err(err) = client.initialize(self.policy.enforce_handshake_timeout_ms) {
            return self.handle_start_error(err);
        }
        if let Err(err) = client.send_initialized_notification() {
            return self.handle_start_error(err);
        }

        self.health.initialized = true;
        self.refresh_catalog()
    }

    pub fn refresh_catalog(&mut self) -> Result<(), McpClientError> {
        let client = self.client.as_mut().ok_or_else(|| McpClientError::Protocol {
            message: "client unavailable".to_string(),
        })?;

        let tools = client.list_tools(self.policy.request_timeout_ms)?;
        let resources = match client.list_resources(self.policy.request_timeout_ms) {
            Ok(resources) => {
                self.health.state = McpServerState::Ready;
                self.health.degraded_reason = None;
                resources
            }
            Err(err) => {
                self.health.state = McpServerState::Degraded;
                self.health.degraded_reason = Some(err.to_string());
                Vec::new()
            }
        };

        self.health.stderr_tail = client.stderr_tail();
        self.health.tool_count = tools.len();
        self.health.resource_count = resources.len();
        self.registration = Some(McpServerRegistration::from_catalog(
            self.spec.server_name.clone(),
            "stdio".to_string(),
            client.server_info().cloned(),
            self.health.state.clone(),
            tools,
            resources,
            self.health.degraded_reason.clone(),
        ));

        if self.health.state == McpServerState::Starting {
            self.health.state = McpServerState::Ready;
        }
        Ok(())
    }

    pub fn recover(&mut self) -> Result<(), McpClientError> {
        self.health.restart_attempts += 1;
        if self.health.restart_attempts > self.policy.max_restart_attempts {
            self.health.state = McpServerState::Failed;
            self.health.last_error = Some("maximum restart attempts exceeded".to_string());
            return Err(McpClientError::Protocol {
                message: "maximum restart attempts exceeded".to_string(),
            });
        }

        self.client = None;
        self.start()
    }

    pub fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolCallResult, McpClientError> {
        self.ensure_available()?;
        self.client
            .as_mut()
            .expect("available client")
            .call_tool(tool_name, arguments, self.policy.request_timeout_ms)
    }

    pub fn read_resource(&mut self, uri: &str) -> Result<McpReadResourceResult, McpClientError> {
        self.ensure_available()?;
        self.client
            .as_mut()
            .expect("available client")
            .read_resource(uri, self.policy.request_timeout_ms)
    }

    pub fn health(&self) -> &McpServerHealth {
        &self.health
    }

    pub fn registration(&self) -> Option<&McpServerRegistration> {
        self.registration.as_ref()
    }

    pub fn server_name(&self) -> &str {
        &self.spec.server_name
    }

    fn ensure_available(&mut self) -> Result<(), McpClientError> {
        let running = match self.client.as_mut() {
            Some(client) => client.is_running(),
            None => false,
        };

        if running {
            return Ok(());
        }

        if self.policy.restart_on_disconnect {
            return self.recover();
        }

        self.health.state = McpServerState::Failed;
        Err(McpClientError::Protocol {
            message: "MCP server is not running".to_string(),
        })
    }

    fn handle_start_error(&mut self, err: McpClientError) -> Result<(), McpClientError> {
        let message = err.to_string();
        self.health.last_error = Some(message.clone());
        self.health.stderr_tail = self
            .client
            .as_ref()
            .map(|client| client.stderr_tail())
            .unwrap_or_default();

        if self.policy.allow_degraded_startup {
            self.health.state = McpServerState::Degraded;
            self.health.degraded_reason = Some(message.clone());
            self.registration = Some(McpServerRegistration::from_catalog(
                self.spec.server_name.clone(),
                "stdio".to_string(),
                None,
                McpServerState::Degraded,
                Vec::new(),
                Vec::new(),
                Some(message),
            ));
            Ok(())
        } else {
            self.health.state = McpServerState::Failed;
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_startup_is_reported_without_crashing() {
        let mut server = ManagedMcpServer::new(
            McpStdioServerSpec {
                server_name: "missing".to_string(),
                command: "/definitely/missing/mcp-server".to_string(),
                ..McpStdioServerSpec::default()
            },
            McpClientSpec::default(),
            McpLifecyclePolicy {
                allow_degraded_startup: true,
                ..McpLifecyclePolicy::default()
            },
        );

        server.start().expect("degraded start");

        assert_eq!(server.health().state, McpServerState::Degraded);
        assert!(server.health().last_error.is_some());
        assert!(server.registration().is_some());
    }
}
