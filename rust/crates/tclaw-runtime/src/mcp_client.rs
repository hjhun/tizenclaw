use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

use crate::mcp::{
    JsonRpcError, JsonRpcNotification, JsonRpcRequest, McpClientMetadata, McpInitializeParams,
    McpInitializeResult, McpListResourcesResult, McpListToolsResult, McpPeerCapabilities,
    McpReadResourceParams, McpReadResourceResult, McpResourceCapabilities, McpResourceDefinition,
    McpServerInfo, McpToolCallParams, McpToolCallResult, McpToolCapabilities, McpToolDefinition,
};
use crate::mcp_stdio::{McpStdioServerSpec, McpTransport, McpTransportError, StdioMcpTransport};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpClientSpec {
    pub client_name: String,
    pub version: String,
    pub protocol_version: String,
}

impl Default for McpClientSpec {
    fn default() -> Self {
        Self {
            client_name: "tizenclaw-runtime".to_string(),
            version: "1.0.0".to_string(),
            protocol_version: crate::mcp::MCP_PROTOCOL_VERSION.to_string(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum McpClientError {
    #[error(transparent)]
    Transport(#[from] McpTransportError),
    #[error("MCP server returned remote error {code}: {message}")]
    Remote { code: i32, message: String },
    #[error("MCP protocol error: {message}")]
    Protocol { message: String },
}

pub struct McpClient {
    server_name: String,
    client_spec: McpClientSpec,
    transport: Box<dyn McpTransport>,
    next_request_id: i64,
    initialized: bool,
    server_info: Option<McpServerInfo>,
    capabilities: McpPeerCapabilities,
    tools: Vec<McpToolDefinition>,
    resources: Vec<McpResourceDefinition>,
}

impl McpClient {
    pub fn connect_stdio(
        spec: McpStdioServerSpec,
        client_spec: McpClientSpec,
    ) -> Result<Self, McpClientError> {
        let server_name = spec.server_name.clone();
        let transport = StdioMcpTransport::spawn(spec)?;
        Ok(Self::from_transport(server_name, client_spec, transport))
    }

    pub fn from_transport(
        server_name: impl Into<String>,
        client_spec: McpClientSpec,
        transport: impl McpTransport + 'static,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            client_spec,
            transport: Box::new(transport),
            next_request_id: 1,
            initialized: false,
            server_info: None,
            capabilities: McpPeerCapabilities::default(),
            tools: Vec::new(),
            resources: Vec::new(),
        }
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn server_info(&self) -> Option<&McpServerInfo> {
        self.server_info.as_ref()
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn cached_tools(&self) -> &[McpToolDefinition] {
        &self.tools
    }

    pub fn cached_resources(&self) -> &[McpResourceDefinition] {
        &self.resources
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        self.transport.stderr_tail()
    }

    pub fn initialize(&mut self, timeout_ms: u64) -> Result<McpInitializeResult, McpClientError> {
        let request = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            serde_json::to_value(McpInitializeParams::new(
                McpClientMetadata {
                    name: self.client_spec.client_name.clone(),
                    version: self.client_spec.version.clone(),
                },
                McpPeerCapabilities {
                    tools: Some(McpToolCapabilities::default()),
                    resources: Some(McpResourceCapabilities::default()),
                    experimental: Default::default(),
                },
            ))
            .map_err(|err| McpClientError::Protocol {
                message: err.to_string(),
            })?,
        );

        let response = self
            .transport
            .send_request(&request, Duration::from_millis(timeout_ms))?;
        let result = decode_response::<McpInitializeResult>(response)?;

        self.initialized = true;
        self.capabilities = result.capabilities.clone();
        self.server_info = Some(result.server_info.clone());
        Ok(result)
    }

    pub fn send_initialized_notification(&mut self) -> Result<(), McpClientError> {
        self.transport.send_notification(&JsonRpcNotification::new(
            "notifications/initialized",
            json!({}),
        ))?;
        Ok(())
    }

    pub fn list_tools(&mut self, timeout_ms: u64) -> Result<Vec<McpToolDefinition>, McpClientError> {
        let request = JsonRpcRequest::new(self.next_id(), "tools/list", json!({}));
        let response = self
            .transport
            .send_request(&request, Duration::from_millis(timeout_ms))?;
        let result = decode_response::<McpListToolsResult>(response)?;
        self.tools = result.tools.clone();
        Ok(result.tools)
    }

    pub fn list_resources(
        &mut self,
        timeout_ms: u64,
    ) -> Result<Vec<McpResourceDefinition>, McpClientError> {
        let request = JsonRpcRequest::new(self.next_id(), "resources/list", json!({}));
        let response = self
            .transport
            .send_request(&request, Duration::from_millis(timeout_ms))?;
        let result = decode_response::<McpListResourcesResult>(response)?;
        self.resources = result.resources.clone();
        Ok(result.resources)
    }

    pub fn read_resource(
        &mut self,
        uri: &str,
        timeout_ms: u64,
    ) -> Result<McpReadResourceResult, McpClientError> {
        let request = JsonRpcRequest::new(
            self.next_id(),
            "resources/read",
            serde_json::to_value(McpReadResourceParams {
                uri: uri.to_string(),
            })
            .map_err(|err| McpClientError::Protocol {
                message: err.to_string(),
            })?,
        );
        let response = self
            .transport
            .send_request(&request, Duration::from_millis(timeout_ms))?;
        decode_response::<McpReadResourceResult>(response)
    }

    pub fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
        timeout_ms: u64,
    ) -> Result<McpToolCallResult, McpClientError> {
        let request = JsonRpcRequest::new(
            self.next_id(),
            "tools/call",
            serde_json::to_value(McpToolCallParams {
                name: tool_name.to_string(),
                arguments,
            })
            .map_err(|err| McpClientError::Protocol {
                message: err.to_string(),
            })?,
        );
        let response = self
            .transport
            .send_request(&request, Duration::from_millis(timeout_ms))?;
        decode_response::<McpToolCallResult>(response)
    }

    pub fn is_running(&mut self) -> bool {
        self.transport.is_running()
    }

    pub fn close(&mut self) {
        self.transport.close();
    }

    fn next_id(&mut self) -> i64 {
        let current = self.next_request_id;
        self.next_request_id += 1;
        current
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.close();
    }
}

fn decode_response<T>(response: crate::mcp::JsonRpcResponse) -> Result<T, McpClientError>
where
    T: for<'de> Deserialize<'de>,
{
    if let Some(JsonRpcError { code, message, .. }) = response.error {
        return Err(McpClientError::Remote { code, message });
    }

    let Some(result) = response.result else {
        return Err(McpClientError::Protocol {
            message: "missing response result".to_string(),
        });
    };

    serde_json::from_value::<T>(result).map_err(|err| McpClientError::Protocol {
        message: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    fn fake_server_script() -> String {
        r#"#!/usr/bin/env python3
import json
import sys

for raw in sys.stdin:
    raw = raw.strip()
    if not raw:
        continue
    req = json.loads(raw)
    method = req.get("method")
    if method == "initialize":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}, "resources": {}},
                "serverInfo": {"name": "fake", "version": "1.2.3"}
            }
        }), flush=True)
    elif method == "tools/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "Echo back text",
                    "inputSchema": {"type": "object"}
                }]
            }
        }), flush=True)
    elif method == "resources/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "resources": [{
                    "uri": "file:///tmp/demo.txt",
                    "name": "demo",
                    "description": "Demo file",
                    "mimeType": "text/plain"
                }]
            }
        }), flush=True)
    elif method == "resources/read":
        uri = req.get("params", {}).get("uri")
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": "resource payload"
                }]
            }
        }), flush=True)
    elif method == "tools/call":
        args = req.get("params", {}).get("arguments", {})
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "content": [{
                    "type": "text",
                    "text": args.get("message", "missing")
                }]
            }
        }), flush=True)
"#
        .to_string()
    }

    #[test]
    fn initialize_and_discover_runtime_catalog() {
        let dir = tempdir().expect("tempdir");
        let script_path = dir.path().join("fake_mcp.py");
        fs::write(&script_path, fake_server_script()).expect("write fake server");

        let mut client = McpClient::connect_stdio(
            McpStdioServerSpec {
                server_name: "fake".to_string(),
                command: "python3".to_string(),
                args: vec![script_path.to_string_lossy().to_string()],
                ..McpStdioServerSpec::default()
            },
            McpClientSpec::default(),
        )
        .expect("connect");

        let init = client.initialize(2_000).expect("initialize");
        client
            .send_initialized_notification()
            .expect("initialized notification");
        let tools = client.list_tools(2_000).expect("list tools");
        let resources = client.list_resources(2_000).expect("list resources");
        let read = client
            .read_resource("file:///tmp/demo.txt", 2_000)
            .expect("read resource");
        let tool_call = client
            .call_tool("echo", json!({"message": "hello"}), 2_000)
            .expect("call tool");

        assert_eq!(init.server_info.name, "fake");
        assert_eq!(tools[0].name, "echo");
        assert_eq!(resources[0].uri, "file:///tmp/demo.txt");
        assert_eq!(read.contents[0].text.as_deref(), Some("resource payload"));
        assert_eq!(tool_call.content.len(), 1);
    }
}
