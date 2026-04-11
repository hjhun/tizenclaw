//! MCP client — Model Context Protocol client for external tool servers.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::llm::backend::LlmToolDecl;

pub type ToolDecl = LlmToolDecl;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpTransport {
    Stdio { command: String, args: Vec<String> },
    Http { url: String },
}

pub struct McpClient {
    pub server_name: String,
    pub server_url: String,
    pub transport: McpTransport,
    timeout_ms: u64,
    child: Option<Child>,
    reader: Option<Mutex<BufReader<std::process::ChildStdout>>>,
    writer: Option<Mutex<std::process::ChildStdin>>,
    connected: bool,
    tools: Vec<ToolDecl>,
    next_req_id: AtomicI32,
    last_used_ms: u64,
}

impl McpClient {
    pub fn new(server_name: &str, command: &str, args: &[String], timeout_ms: u64) -> Self {
        Self {
            server_name: server_name.into(),
            server_url: format!("stdio://{}", server_name),
            transport: McpTransport::Stdio {
                command: command.into(),
                args: args.to_vec(),
            },
            timeout_ms,
            child: None,
            reader: None,
            writer: None,
            connected: false,
            tools: Vec::new(),
            next_req_id: AtomicI32::new(1),
            last_used_ms: Self::now_ms(),
        }
    }

    pub fn connect(config: &Value) -> Result<Self, String> {
        let transport_name = config
            .get("transport")
            .and_then(Value::as_str)
            .unwrap_or("stdio")
            .trim()
            .to_ascii_lowercase();
        let server_name = config
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("mcp")
            .trim();
        let timeout_ms = config
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(30_000);

        let mut client = match transport_name.as_str() {
            "http" => {
                let url = config
                    .get("url")
                    .or_else(|| config.get("server_url"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "mcp http transport requires url".to_string())?;
                Self {
                    server_name: server_name.to_string(),
                    server_url: url.to_string(),
                    transport: McpTransport::Http {
                        url: url.to_string(),
                    },
                    timeout_ms,
                    child: None,
                    reader: None,
                    writer: None,
                    connected: false,
                    tools: Vec::new(),
                    next_req_id: AtomicI32::new(1),
                    last_used_ms: Self::now_ms(),
                }
            }
            _ => {
                let command = config
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "mcp stdio transport requires command".to_string())?;
                let args = config
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|value| value.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Self::new(server_name, command, &args, timeout_ms)
            }
        };

        if client.open() {
            Ok(client)
        } else {
            Err(format!("Failed to connect MCP client '{}'", server_name))
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn update_last_used(&mut self) {
        self.last_used_ms = Self::now_ms();
    }

    pub fn last_used_ms(&self) -> u64 {
        self.last_used_ms
    }

    pub fn open(&mut self) -> bool {
        if self.connected {
            return true;
        }
        self.update_last_used();

        match &self.transport {
            McpTransport::Stdio { command, args } => {
                let mut child = match Command::new(command)
                    .args(args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(child) => child,
                    Err(err) => {
                        log::error!(
                            "MCP client '{}' failed to spawn transport: {}",
                            self.server_name,
                            err
                        );
                        return false;
                    }
                };

                let pid = child.id();
                let Some(stdout) = child.stdout.take() else {
                    return false;
                };
                let Some(stdin) = child.stdin.take() else {
                    return false;
                };

                self.reader = Some(Mutex::new(BufReader::new(stdout)));
                self.writer = Some(Mutex::new(stdin));
                self.child = Some(child);
                self.connected = true;

                log::debug!("MCP client '{}' started (pid {})", self.server_name, pid);
            }
            McpTransport::Http { .. } => {
                self.connected = true;
            }
        }

        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "tizenclaw-mcp-client", "version": "1.0.0"}
        });

        match self.rpc_request("initialize", &init_params, 10_000) {
            Ok(resp) if resp.get("error").is_none() => {}
            Ok(_) | Err(_) => {
                self.disconnect();
                return false;
            }
        }

        let _ = self.send_notification("notifications/initialized", &json!({}));
        true
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.reader = None;
        self.writer = None;

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn discover_tools(&mut self) -> &[ToolDecl] {
        if !self.connected {
            return &self.tools;
        }

        match self.rpc_request("tools/list", &json!({}), 5_000) {
            Ok(resp) => {
                let tools = resp
                    .get("result")
                    .and_then(|value| value.get("tools"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                self.tools = tools
                    .iter()
                    .filter_map(|tool| {
                        let name = tool.get("name").and_then(Value::as_str)?;
                        Some(ToolDecl {
                            name: format!("mcp_{}_{}", self.server_name, name),
                            description: tool
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            parameters: tool
                                .get("inputSchema")
                                .cloned()
                                .unwrap_or_else(|| json!({"type":"object"})),
                        })
                    })
                    .collect();
            }
            Err(err) => {
                log::warn!("MCP client '{}' tools/list failed: {}", self.server_name, err);
            }
        }

        &self.tools
    }

    pub fn list_tools(&mut self) -> Vec<ToolDecl> {
        self.discover_tools();
        self.tools.clone()
    }

    pub fn get_tools(&self) -> &[ToolDecl] {
        &self.tools
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn call_tool(&mut self, tool_name: &str, arguments: &Value) -> Result<Value, String> {
        if !self.connected {
            return Err("Not connected".into());
        }
        self.update_last_used();

        let response = self.rpc_request(
            "tools/call",
            &json!({"name": tool_name, "arguments": arguments}),
            self.timeout_ms,
        )?;
        if let Some(result) = response.get("result") {
            Ok(result.clone())
        } else if let Some(error) = response.get("error") {
            Err(error.to_string())
        } else {
            Err("Invalid MCP response".to_string())
        }
    }

    fn send_notification(&self, method: &str, params: &Value) -> Result<(), String> {
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        match &self.transport {
            McpTransport::Stdio { .. } => self.send_stdio_message(&message),
            McpTransport::Http { url } => {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| err.to_string())?
                    .block_on(async {
                        reqwest::Client::builder()
                            .timeout(Duration::from_secs(10))
                            .build()
                            .map_err(|err| err.to_string())?
                            .post(url)
                            .header("Content-Type", "application/json")
                            .body(message.to_string())
                            .send()
                            .await
                            .map(|_| ())
                            .map_err(|err| err.to_string())
                    })
            }
        }
    }

    fn rpc_request(&self, method: &str, params: &Value, timeout_ms: u64) -> Result<Value, String> {
        let req_id = self.next_req_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params
        });

        match &self.transport {
            McpTransport::Stdio { .. } => {
                self.send_stdio_message(&request)?;
                self.read_stdio_response(req_id, timeout_ms)
            }
            McpTransport::Http { url } => {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| err.to_string())?
                    .block_on(async {
                        let response = reqwest::Client::builder()
                            .timeout(Duration::from_millis(timeout_ms))
                            .build()
                            .map_err(|err| err.to_string())?
                            .post(url)
                            .header("Content-Type", "application/json")
                            .body(request.to_string())
                            .send()
                            .await
                            .map_err(|err| err.to_string())?;
                        response.json::<Value>().await.map_err(|err| err.to_string())
                    })
            }
        }
    }

    fn send_stdio_message(&self, message: &Value) -> Result<(), String> {
        let writer = self.writer.as_ref().ok_or("No writer")?;
        let mut writer = writer.lock().map_err(|err| err.to_string())?;
        writer
            .write_all(format!("{}\n", message).as_bytes())
            .map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())
    }

    fn read_stdio_response(&self, request_id: i32, timeout_ms: u64) -> Result<Value, String> {
        let reader = self.reader.as_ref().ok_or("No reader")?;
        let mut reader = reader.lock().map_err(|err| err.to_string())?;
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let mut line = String::new();

        loop {
            if start.elapsed() >= timeout {
                return Err(format!("Timeout after {}ms", timeout_ms));
            }

            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return Err("EOF".into()),
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let response: Value =
                        serde_json::from_str(trimmed).map_err(|err| err.to_string())?;
                    if response.get("id").and_then(Value::as_i64) == Some(request_id as i64) {
                        return Ok(response);
                    }
                }
                Err(err) => return Err(err.to_string()),
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

pub struct McpClientManager {
    clients: Vec<McpClient>,
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

impl McpClientManager {
    pub fn new() -> Self {
        Self { clients: Vec::new() }
    }

    pub fn load_config_and_connect(&mut self, path: &str) -> bool {
        let Ok(content) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(config) = serde_json::from_str::<Value>(&content) else {
            return false;
        };

        if let Some(servers) = config.get("servers").and_then(Value::as_array) {
            for server in servers {
                let Ok(mut client) = McpClient::connect(server) else {
                    continue;
                };
                client.discover_tools();
                self.clients.push(client);
            }
        }

        !self.clients.is_empty()
    }

    pub fn get_all_tools(&self) -> Vec<ToolDecl> {
        self.clients
            .iter()
            .flat_map(|client| client.get_tools().to_vec())
            .collect()
    }

    pub fn call_tool(&mut self, full_name: &str, args: &Value) -> Option<Value> {
        for client in &mut self.clients {
            let prefix = format!("mcp_{}_", client.server_name);
            if let Some(tool_name) = full_name.strip_prefix(&prefix) {
                return Some(match client.call_tool(tool_name, args) {
                    Ok(value) => value,
                    Err(err) => json!({"isError": true, "error": err}),
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::McpClient;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn stdio_connect_and_list_tools() {
        let dir = tempdir().expect("tempdir");
        let script_path = dir.path().join("fake_mcp.py");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env python3
import json
import sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    method = req.get("method")
    if method == "initialize":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}}
        }), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "Echo a payload",
                    "inputSchema": {"type": "object"}
                }]
            }
        }), flush=True)
    elif method == "tools/call":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {"content": [{"type": "text", "text": "ok"}]}
        }), flush=True)
"#,
        )
        .expect("write fake server");

        let mut client = McpClient::connect(&json!({
            "name": "fake",
            "transport": "stdio",
            "command": "python3",
            "args": [script_path.to_string_lossy().to_string()]
        }))
        .expect("connect mcp");

        let tools = client.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "mcp_fake_echo");

        let result = client.call_tool("echo", &json!({"text": "hello"})).expect("call tool");
        assert_eq!(result["content"][0]["text"], "ok");
    }
}
