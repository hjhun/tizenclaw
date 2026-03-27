//! MCP client — Model Context Protocol client for external tool servers.
//!
//! Connects to an MCP server via stdio transport:
//! - Spawns child process → pipes stdin/stdout for JSON-RPC 2.0
//! - Performs `initialize` handshake
//! - Discovers remote tools via `tools/list`
//! - Calls remote tools via `tools/call`

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::llm::backend::LlmToolDecl;

/// A single MCP client connected to a server process.
pub struct McpClient {
    pub server_name: String,
    command: String,
    args: Vec<String>,
    timeout_ms: u64,
    child: Option<Child>,
    reader: Option<Mutex<BufReader<std::process::ChildStdout>>>,
    writer: Option<Mutex<std::process::ChildStdin>>,
    connected: bool,
    tools: Vec<LlmToolDecl>,
    next_req_id: AtomicI32,
    last_used_ms: u64,
}

impl McpClient {
    pub fn new(server_name: &str, command: &str, args: &[String], timeout_ms: u64) -> Self {
        McpClient {
            server_name: server_name.into(),
            command: command.into(),
            args: args.to_vec(),
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

    /// Spawn the server process and perform the MCP handshake.
    pub fn connect(&mut self) -> bool {
        if self.connected {
            return true;
        }
        self.update_last_used();

        let mut child = match Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::error!("MCP Client: Failed to spawn '{}': {}", self.command, e);
                return false;
            }
        };

        let pid = child.id();
        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();

        self.reader = Some(Mutex::new(BufReader::new(stdout)));
        self.writer = Some(Mutex::new(stdin));
        self.child = Some(child);
        self.connected = true;

        log::info!(
            "MCP Client: '{}' started (PID: {})",
            self.server_name, pid
        );

        // Perform initialize handshake
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "tizenclaw-mcp-client", "version": "1.0.0"}
        });

        match self.send_request_sync("initialize", &init_params, 10000) {
            Ok(resp) => {
                if resp.get("error").is_some() {
                    log::error!("MCP Client: Handshake failed for '{}'", self.server_name);
                    self.disconnect();
                    return false;
                }
            }
            Err(e) => {
                log::error!("MCP Client: Init error for '{}': {}", self.server_name, e);
                self.disconnect();
                return false;
            }
        }

        // Send notifications/initialized
        let notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let _ = self.send_rpc_message(&notif);

        log::info!("MCP Client: Handshake succeeded for '{}'", self.server_name);
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

    /// Discover tools from the remote server.
    pub fn discover_tools(&mut self) -> &[LlmToolDecl] {
        if !self.connected {
            return &self.tools;
        }

        match self.send_request_sync("tools/list", &json!({}), 5000) {
            Ok(resp) => {
                if let Some(tools_arr) = resp
                    .get("result")
                    .and_then(|r| r.get("tools"))
                    .and_then(|t| t.as_array())
                {
                    self.tools = tools_arr
                        .iter()
                        .filter_map(|t| {
                            let name = t["name"].as_str()?;
                            Some(LlmToolDecl {
                                name: format!("mcp_{}_{}", self.server_name, name),
                                description: t["description"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                                parameters: t
                                    .get("inputSchema")
                                    .cloned()
                                    .unwrap_or_else(|| json!({"type": "object"})),
                            })
                        })
                        .collect();
                }
            }
            Err(e) => {
                log::error!(
                    "MCP Client: tools/list error for '{}': {}",
                    self.server_name, e
                );
            }
        }
        &self.tools
    }

    pub fn get_tools(&self) -> &[LlmToolDecl] {
        &self.tools
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Call a tool on the remote server.
    pub fn call_tool(&mut self, tool_name: &str, arguments: &Value) -> Value {
        if !self.connected {
            return json!({"error": "Not connected"});
        }
        self.update_last_used();

        let params = json!({"name": tool_name, "arguments": arguments});
        match self.send_request_sync("tools/call", &params, self.timeout_ms as i64) {
            Ok(resp) => {
                if let Some(result) = resp.get("result") {
                    result.clone()
                } else if let Some(error) = resp.get("error") {
                    json!({"isError": true, "error": error})
                } else {
                    json!({"isError": true, "error": "Invalid response"})
                }
            }
            Err(e) => json!({"isError": true, "error": e.to_string()}),
        }
    }

    fn send_rpc_message(&self, message: &Value) -> Result<(), String> {
        let writer = self.writer.as_ref().ok_or("No writer")?;
        let mut writer = writer.lock().map_err(|e| e.to_string())?;
        let data = format!("{}\n", message);
        writer
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn read_rpc_message(&self, timeout_ms: i64) -> Result<Value, String> {
        let reader = self.reader.as_ref().ok_or("No reader")?;
        let mut reader = reader.lock().map_err(|e| e.to_string())?;

        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms as u64);
        let mut line = String::new();

        loop {
            if start.elapsed() >= timeout {
                return Err("Timeout".into());
            }

            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => return Err("EOF".into()),
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    return serde_json::from_str(trimmed).map_err(|e| e.to_string());
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    fn send_request_sync(
        &self,
        method: &str,
        params: &Value,
        timeout_ms: i64,
    ) -> Result<Value, String> {
        let req_id = self.next_req_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params
        });

        self.send_rpc_message(&request)?;

        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms as u64);

        loop {
            if start.elapsed() >= timeout {
                return Err(format!("Timeout after {}ms", timeout_ms));
            }

            let remaining = timeout
                .checked_sub(start.elapsed())
                .unwrap_or(Duration::from_millis(1));
            let resp = self.read_rpc_message(remaining.as_millis() as i64)?;

            // Check for matching ID
            if resp.get("id").and_then(|v| v.as_i64()) == Some(req_id as i64) {
                return Ok(resp);
            }

            // Handle notifications
            if let Some(m) = resp.get("method").and_then(|v| v.as_str()) {
                log::info!("MCP Client: notification from '{}': {}", self.server_name, m);
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Manages multiple MCP client connections.
pub struct McpClientManager {
    clients: Vec<McpClient>,
}

impl McpClientManager {
    pub fn new() -> Self {
        McpClientManager {
            clients: Vec::new(),
        }
    }

    /// Load MCP server configs from JSON and connect.
    ///
    /// Config format:
    /// ```json
    /// { "servers": [{"name": "x", "command": "/usr/bin/x", "args": ["--stdio"]}] }
    /// ```
    pub fn load_config_and_connect(&mut self, path: &str) -> bool {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let config: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return false,
        };

        if let Some(servers) = config["servers"].as_array() {
            for s in servers {
                let name = s["name"].as_str().unwrap_or("").to_string();
                let command = s["command"].as_str().unwrap_or("").to_string();
                let args: Vec<String> = s["args"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let timeout = s["timeout_ms"].as_u64().unwrap_or(30000);

                if name.is_empty() || command.is_empty() {
                    continue;
                }

                let mut client = McpClient::new(&name, &command, &args, timeout);
                if client.connect() {
                    client.discover_tools();
                    log::info!(
                        "MCP Client: '{}' connected ({} tools)",
                        name, client.get_tools().len()
                    );
                }
                self.clients.push(client);
            }
        }

        !self.clients.is_empty()
    }

    /// Get all tools from all connected clients.
    pub fn get_all_tools(&self) -> Vec<LlmToolDecl> {
        self.clients
            .iter()
            .flat_map(|c| c.get_tools().to_vec())
            .collect()
    }

    /// Route a tool call to the appropriate client.
    pub fn call_tool(&mut self, full_name: &str, args: &Value) -> Option<Value> {
        for client in &mut self.clients {
            let prefix = format!("mcp_{}_", client.server_name);
            if let Some(tool_name) = full_name.strip_prefix(&prefix) {
                return Some(client.call_tool(tool_name, args));
            }
        }
        None
    }
}
