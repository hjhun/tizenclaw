use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::mcp::{JsonRpcId, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum McpTransportError {
    #[error("unsupported transport mode: {mode}")]
    UnsupportedTransport { mode: String },
    #[error("failed to spawn MCP server '{server}': {message}")]
    Spawn { server: String, message: String },
    #[error("transport I/O failed: {message}")]
    Io { message: String },
    #[error("transport timed out waiting for response to {request_id:?} after {timeout_ms}ms")]
    Timeout { request_id: JsonRpcId, timeout_ms: u64 },
    #[error("transport protocol error: {message}")]
    Protocol { message: String },
    #[error("transport process exited: {message}")]
    ProcessExited { message: String },
}

pub trait McpTransport: Send {
    fn send_request(
        &mut self,
        request: &JsonRpcRequest,
        timeout: Duration,
    ) -> Result<JsonRpcResponse, McpTransportError>;

    fn send_notification(
        &mut self,
        notification: &JsonRpcNotification,
    ) -> Result<(), McpTransportError>;

    fn is_running(&mut self) -> bool;

    fn close(&mut self);

    fn description(&self) -> String;

    fn stderr_tail(&self) -> Vec<String> {
        Vec::new()
    }
}

enum TransportEvent {
    Response(JsonRpcResponse),
    ReaderError(String),
    Eof,
}

pub struct StdioMcpTransport {
    spec: McpStdioServerSpec,
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<TransportEvent>,
    buffered_responses: BTreeMap<JsonRpcId, JsonRpcResponse>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    stdout_join: Option<JoinHandle<()>>,
    stderr_join: Option<JoinHandle<()>>,
}

impl StdioMcpTransport {
    pub fn spawn(spec: McpStdioServerSpec) -> Result<Self, McpTransportError> {
        match spec.transport {
            StdioTransportMode::Stdio => {}
            StdioTransportMode::Pty => {
                return Err(McpTransportError::UnsupportedTransport {
                    mode: "pty".to_string(),
                });
            }
        }

        let mut command = Command::new(&spec.command);
        command
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|err| McpTransportError::Spawn {
            server: spec.server_name.clone(),
            message: err.to_string(),
        })?;

        let stdin = child.stdin.take().ok_or_else(|| McpTransportError::Spawn {
            server: spec.server_name.clone(),
            message: "child stdin unavailable".to_string(),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| McpTransportError::Spawn {
            server: spec.server_name.clone(),
            message: "child stdout unavailable".to_string(),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| McpTransportError::Spawn {
            server: spec.server_name.clone(),
            message: "child stderr unavailable".to_string(),
        })?;

        let (sender, receiver) = mpsc::channel();
        let stdout_join = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let parsed: Value = match serde_json::from_str(trimmed) {
                            Ok(value) => value,
                            Err(err) => {
                                let _ = sender.send(TransportEvent::ReaderError(err.to_string()));
                                continue;
                            }
                        };

                        if parsed.get("id").is_some()
                            && (parsed.get("result").is_some() || parsed.get("error").is_some())
                        {
                            match serde_json::from_value::<JsonRpcResponse>(parsed) {
                                Ok(response) => {
                                    let _ = sender.send(TransportEvent::Response(response));
                                }
                                Err(err) => {
                                    let _ =
                                        sender.send(TransportEvent::ReaderError(err.to_string()));
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let _ = sender.send(TransportEvent::ReaderError(err.to_string()));
                        break;
                    }
                }
            }

            let _ = sender.send(TransportEvent::Eof);
        });

        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(16)));
        let stderr_tail_clone = Arc::clone(&stderr_tail);
        let stderr_join = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(mut guard) = stderr_tail_clone.lock() {
                    if guard.len() == 16 {
                        guard.pop_front();
                    }
                    guard.push_back(trimmed.to_string());
                }
            }
        });

        Ok(Self {
            spec,
            child,
            stdin,
            receiver,
            buffered_responses: BTreeMap::new(),
            stderr_tail,
            stdout_join: Some(stdout_join),
            stderr_join: Some(stderr_join),
        })
    }

    fn send_serialized<T: Serialize>(&mut self, message: &T) -> Result<(), McpTransportError> {
        let encoded =
            serde_json::to_string(message).map_err(|err| McpTransportError::Protocol {
                message: err.to_string(),
            })?;
        self.stdin
            .write_all(encoded.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|err| McpTransportError::Io {
                message: err.to_string(),
            })
    }

    fn receive_response(
        &mut self,
        request_id: &JsonRpcId,
        timeout: Duration,
    ) -> Result<JsonRpcResponse, McpTransportError> {
        if let Some(response) = self.buffered_responses.remove(request_id) {
            return Ok(response);
        }

        loop {
            match self.receiver.recv_timeout(timeout) {
                Ok(TransportEvent::Response(response)) => {
                    if &response.id == request_id {
                        return Ok(response);
                    }
                    self.buffered_responses.insert(response.id.clone(), response);
                }
                Ok(TransportEvent::ReaderError(message)) => {
                    return Err(McpTransportError::Protocol { message });
                }
                Ok(TransportEvent::Eof) => {
                    return Err(McpTransportError::ProcessExited {
                        message: self.process_exit_message(),
                    });
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(McpTransportError::Timeout {
                        request_id: request_id.clone(),
                        timeout_ms: timeout.as_millis() as u64,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(McpTransportError::ProcessExited {
                        message: self.process_exit_message(),
                    });
                }
            }
        }
    }

    fn process_exit_message(&mut self) -> String {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let stderr = self.stderr_tail().join(" | ");
                if stderr.is_empty() {
                    format!("{} exited with {}", self.spec.server_name, status)
                } else {
                    format!("{} exited with {} ({})", self.spec.server_name, status, stderr)
                }
            }
            Ok(None) => format!("{} closed its stdio pipe", self.spec.server_name),
            Err(err) => err.to_string(),
        }
    }
}

impl McpTransport for StdioMcpTransport {
    fn send_request(
        &mut self,
        request: &JsonRpcRequest,
        timeout: Duration,
    ) -> Result<JsonRpcResponse, McpTransportError> {
        self.send_serialized(request)?;
        self.receive_response(&request.id, timeout)
    }

    fn send_notification(
        &mut self,
        notification: &JsonRpcNotification,
    ) -> Result<(), McpTransportError> {
        self.send_serialized(notification)
    }

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn close(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.stdout_join.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_join.take() {
            let _ = handle.join();
        }
    }

    fn description(&self) -> String {
        format!("stdio:{}", self.spec.server_name)
    }

    fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .map(|guard| guard.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl Drop for StdioMcpTransport {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::*;
    use crate::mcp::{JsonRpcRequest, McpReadResourceParams};

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
                "serverInfo": {"name": "fake", "version": "1.0.0"}
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
                    "text": "hello from resource"
                }]
            }
        }), flush=True)
    else:
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": req.get("id"),
            "result": {}
        }), flush=True)
"#
        .to_string()
    }

    #[test]
    fn stdio_transport_round_trips_requests() {
        let dir = tempdir().expect("tempdir");
        let script_path = dir.path().join("fake_mcp.py");
        fs::write(&script_path, fake_server_script()).expect("write fake server");

        let mut transport = StdioMcpTransport::spawn(McpStdioServerSpec {
            server_name: "fake".to_string(),
            command: "python3".to_string(),
            args: vec![script_path.to_string_lossy().to_string()],
            ..McpStdioServerSpec::default()
        })
        .expect("spawn transport");

        let response = transport
            .send_request(
                &JsonRpcRequest::new(1, "initialize", serde_json::json!({})),
                Duration::from_secs(2),
            )
            .expect("initialize");
        assert_eq!(response.id, JsonRpcId::Number(1));

        let response = transport
            .send_request(
                &JsonRpcRequest::new(
                    2,
                    "resources/read",
                    serde_json::to_value(McpReadResourceParams {
                        uri: "file:///tmp/demo.txt".to_string(),
                    })
                    .expect("resource params"),
                ),
                Duration::from_secs(2),
            )
            .expect("read resource");
        assert_eq!(response.id, JsonRpcId::Number(2));
        assert!(response.result.is_some());
    }

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
