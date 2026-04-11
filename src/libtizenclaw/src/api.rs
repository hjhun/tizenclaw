//! Safe Rust API for TizenClaw.
//!
//! This module provides a safe, ergonomic Rust interface wrapping the daemon's
//! IPC surface. It is used internally by the C FFI layer and is also available
//! to Rust consumers via `rlib`.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::io::FromRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const DEFAULT_SOCKET_NAME: &str = "tizenclaw.sock";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_IPC_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
const IPC_RETRY_SLEEP_MS: u64 = 25;

/// Process uptime and system metrics.
fn parse_proc_status() -> (i64, i64, i32) {
    let mut rss_kb: i64 = 0;
    let mut vm_kb: i64 = 0;
    let mut threads: i32 = 0;

    if let Ok(content) = std::fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("VmRSS:") {
                rss_kb = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("VmSize:") {
                vm_kb = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("Threads:") {
                threads = val.trim().parse().unwrap_or(0);
            }
        }
    }
    (rss_kb, vm_kb, threads)
}

fn parse_loadavg() -> (f64, f64, f64) {
    if let Ok(content) = std::fs::read_to_string("/proc/loadavg") {
        let parts: Vec<&str> = content.split_whitespace().collect();
        let l1 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let l5 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let l15 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        (l1, l5, l15)
    } else {
        (0.0, 0.0, 0.0)
    }
}

fn get_process_uptime() -> f64 {
    let sys_uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    let proc_start = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            let after_comm = s.rfind(')')?;
            let rest = &s[after_comm + 2..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            fields.get(19).and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    let clk_tck: f64 = 100.0;
    let start_secs = proc_start / clk_tck;
    if sys_uptime > start_secs {
        sys_uptime - start_secs
    } else {
        0.0
    }
}

fn bool_to_json(flag: bool) -> Value {
    Value::Bool(flag)
}

fn is_retryable_read_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

fn read_exact_with_retry<R: Read>(
    reader: &mut R,
    buf: &mut [u8],
    deadline: Instant,
    context: &str,
) -> Result<(), String> {
    let mut offset = 0;
    while offset < buf.len() {
        match reader.read(&mut buf[offset..]) {
            Ok(0) => {
                return Err(format!(
                    "IPC {} failed: unexpected EOF after {} of {} bytes",
                    context,
                    offset,
                    buf.len()
                ));
            }
            Ok(read) => {
                offset += read;
            }
            Err(error) if is_retryable_read_error(&error) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(IPC_RETRY_SLEEP_MS));
            }
            Err(error) => {
                return Err(format!("IPC {} failed: {}", context, error));
            }
        }
    }

    Ok(())
}

struct RpcEnvelope {
    payload: Value,
    stream_received: bool,
}

/// Result of prompt execution through the daemon IPC layer.
pub struct PromptResponse {
    pub session_id: String,
    pub text: String,
    pub stream_received: bool,
}

/// TizenClaw agent — safe Rust API.
///
/// # Example (Rust)
/// ```rust,no_run
/// use tizenclaw::api::TizenClaw;
///
/// let mut agent = TizenClaw::new();
/// agent.initialize().unwrap();
/// let response = agent.process_prompt("Hello!", "default").unwrap();
/// log::info!("{}", response);
/// ```
pub struct TizenClaw {
    socket_path: Option<String>,
    timeout_ms: u64,
}

impl TizenClaw {
    /// Create a new agent instance.
    pub fn new() -> Self {
        TizenClaw {
            socket_path: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    /// Initialize the agent by verifying the daemon is reachable.
    pub fn initialize(&mut self) -> Result<(), String> {
        let result = self.call("ping", json!({}))?;
        if result.get("pong").and_then(Value::as_bool) == Some(true) {
            log::info!("TizenClaw API initialized");
            Ok(())
        } else {
            Err("Daemon ping returned an unexpected payload".into())
        }
    }

    /// Check if agent is initialized.
    pub fn is_initialized(&self) -> bool {
        self.call("ping", json!({})).is_ok()
    }

    /// Process a prompt synchronously.
    pub fn process_prompt(&self, prompt: &str, session_id: &str) -> Result<String, String> {
        Ok(self
            .process_prompt_inner(prompt, session_id, false, None)?
            .text)
    }

    /// Process a prompt while streaming partial chunks through the callback.
    pub fn process_prompt_streaming<F>(
        &self,
        prompt: &str,
        session_id: &str,
        on_chunk: F,
    ) -> Result<PromptResponse, String>
    where
        F: FnMut(&str),
    {
        let mut on_chunk = on_chunk;
        self.process_prompt_inner(prompt, session_id, true, Some(&mut on_chunk))
    }

    /// Clear a session's conversation history.
    pub fn clear_session(&self, _session_id: &str) -> Result<(), String> {
        Err("clear_session is not exposed by the daemon IPC server".into())
    }

    /// Clear daemon-managed memory and/or session data.
    pub fn clear_agent_data(
        &self,
        include_memory: bool,
        include_sessions: bool,
    ) -> Result<Value, String> {
        self.call(
            "clear_agent_data",
            json!({
                "include_memory": include_memory,
                "include_sessions": include_sessions
            }),
        )
    }

    /// Get agent status as JSON string.
    pub fn get_status(&self) -> Result<String, String> {
        self.runtime_status().map(|value| value.to_string())
    }

    /// Get system metrics as JSON string.
    pub fn get_metrics(&self) -> Result<String, String> {
        let (rss_kb, vm_kb, threads) = parse_proc_status();
        let (load_1m, load_5m, load_15m) = parse_loadavg();
        let uptime = get_process_uptime();
        let hours = uptime as u64 / 3600;
        let minutes = (uptime as u64 % 3600) / 60;
        let seconds = uptime as u64 % 60;

        Ok(json!({
            "version": "1.0.0",
            "status": "running",
            "uptime": {
                "seconds": uptime,
                "formatted": format!("{}h {}m {}s", hours, minutes, seconds)
            },
            "memory": { "vm_rss_kb": rss_kb, "vm_size_kb": vm_kb },
            "cpu": { "load_1m": load_1m, "load_5m": load_5m, "load_15m": load_15m },
            "threads": threads,
            "pid": std::process::id()
        })
        .to_string())
    }

    /// Get available tools as JSON string.
    pub fn get_tools(&self) -> Result<String, String> {
        let result = self.list_tools()?;
        Ok(result
            .get("tools")
            .cloned()
            .unwrap_or_else(|| Value::Array(vec![]))
            .to_string())
    }

    /// Execute a tool by name with JSON arguments.
    pub fn execute_tool(&self, tool_name: &str, args_json: &str) -> Result<String, String> {
        let args: Value =
            serde_json::from_str(args_json).map_err(|e| format!("Invalid JSON args: {}", e))?;

        let result = self.call(
            "bridge_tool",
            json!({
                "tool_name": tool_name,
                "args": args,
                "allowed_tools": []
            }),
        )?;

        Ok(result.to_string())
    }

    /// Reload skill manifests.
    pub fn reload_skills(&mut self) -> Result<(), String> {
        let _ = self.list_tools()?;
        log::info!("Skills reloaded");
        Ok(())
    }

    /// Retrieve usage information as structured JSON.
    pub fn get_usage(
        &self,
        session_id: Option<&str>,
        baseline: Option<&Value>,
    ) -> Result<Value, String> {
        let mut params = json!({});
        if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
            params["session_id"] = Value::String(session_id.to_string());
        }
        if let Some(baseline) = baseline {
            params["baseline"] = baseline.clone();
        }

        self.call("get_usage", params)
    }

    /// Start the dashboard channel.
    pub fn start_dashboard(&self, port: Option<u16>) -> Result<Value, String> {
        let mut params = json!({ "name": "web_dashboard" });
        if let Some(port) = port {
            params["settings"] = json!({ "port": port });
        }
        self.call("start_channel", params)
    }

    /// Stop the dashboard channel.
    pub fn stop_dashboard(&self) -> Result<Value, String> {
        self.call("stop_channel", json!({ "name": "web_dashboard" }))
    }

    /// Query the dashboard channel status.
    pub fn dashboard_status(&self) -> Result<Value, String> {
        self.call("channel_status", json!({ "name": "web_dashboard" }))
    }

    /// Read LLM config content or a nested path.
    pub fn get_llm_config(&self, path: Option<&str>) -> Result<Value, String> {
        let params = match path {
            Some(path) => json!({ "path": path }),
            None => json!({}),
        };
        self.call("get_llm_config", params)
    }

    /// Set a nested LLM config value.
    pub fn set_llm_config(&self, path: &str, value: Value) -> Result<Value, String> {
        self.call("set_llm_config", json!({ "path": path, "value": value }))
    }

    /// Remove a nested LLM config value.
    pub fn unset_llm_config(&self, path: &str) -> Result<Value, String> {
        self.call("unset_llm_config", json!({ "path": path }))
    }

    /// Reload backend configuration.
    pub fn reload_llm_backends(&self) -> Result<Value, String> {
        self.call("reload_llm_backends", json!({}))
    }

    /// Register an external tool or skill path.
    pub fn register_path(&self, kind: &str, path: &str) -> Result<Value, String> {
        self.call("register_path", json!({ "kind": kind, "path": path }))
    }

    /// Unregister an external tool or skill path.
    pub fn unregister_path(&self, kind: &str, path: &str) -> Result<Value, String> {
        self.call("unregister_path", json!({ "kind": kind, "path": path }))
    }

    /// List registered tool and skill paths.
    pub fn list_registered_paths(&self) -> Result<Value, String> {
        self.call("list_registered_paths", json!({}))
    }

    /// Read daemon-reported skill capability state.
    pub fn get_skill_capabilities(&self) -> Result<Value, String> {
        self.call("get_skill_capabilities", json!({}))
    }

    /// Read daemon-reported tool execution audit state.
    pub fn get_tool_audit(&self) -> Result<Value, String> {
        self.call("get_tool_audit", json!({}))
    }

    /// List scheduler tasks visible through the daemon IPC channel.
    pub fn list_tasks(&self) -> Result<Value, String> {
        self.call("list_tasks", json!({}))
    }

    /// Read the current devel-mode state exposed through the daemon IPC channel.
    pub fn get_devel_status(&self) -> Result<Value, String> {
        self.call("get_devel_status", json!({}))
    }

    pub fn list_tools(&self) -> Result<Value, String> {
        self.call("bridge_list_tools", json!({ "allowed_tools": [] }))
    }

    pub fn runtime_status(&self) -> Result<Value, String> {
        self.call("runtime_status", json!({}))
    }

    /// Shutdown and release resources.
    pub fn shutdown(&mut self) {
        log::info!("TizenClaw API shutdown");
    }

    fn process_prompt_inner(
        &self,
        prompt: &str,
        session_id: &str,
        stream: bool,
        on_chunk: Option<&mut dyn FnMut(&str)>,
    ) -> Result<PromptResponse, String> {
        let envelope = self.send_jsonrpc(
            "prompt",
            json!({
                "session_id": session_id,
                "text": prompt,
                "stream": bool_to_json(stream)
            }),
            on_chunk,
        )?;
        let result = Self::extract_result(&envelope.payload)?;

        let resolved_session_id = result
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or(session_id)
            .to_string();
        let text = result
            .get("text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| result.to_string());

        Ok(PromptResponse {
            session_id: resolved_session_id,
            text,
            stream_received: envelope.stream_received,
        })
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let envelope = self.send_jsonrpc(method, params, None)?;
        Self::extract_result(&envelope.payload)
    }

    fn extract_result(payload: &Value) -> Result<Value, String> {
        if let Some(error) = payload.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Unknown error");
            return Err(message.to_string());
        }

        payload
            .get("result")
            .cloned()
            .ok_or_else(|| "Missing JSON-RPC result".to_string())
    }

    fn resolved_socket_name(socket_path: Option<&str>) -> String {
        socket_path
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| std::env::var("TIZENCLAW_SOCKET_PATH").ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SOCKET_NAME.to_string())
    }

    fn connect_socket(socket_name: &str) -> Result<UnixStream, String> {
        if socket_name.starts_with('/') {
            let stream = UnixStream::connect(Path::new(socket_name)).map_err(|error| {
                format!("Failed to connect to daemon socket '{}': {}", socket_name, error)
            })?;
            return Ok(stream);
        }

        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err("Failed to create daemon IPC socket".into());
        }

        let connect_result = unsafe {
            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
            for (index, byte) in socket_name.as_bytes().iter().enumerate() {
                addr.sun_path[index + 1] = *byte as libc::c_char;
            }
            let addr_len = (std::mem::size_of::<libc::sa_family_t>()
                + 1
                + socket_name.len()) as libc::socklen_t;
            libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len)
        };

        if connect_result < 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(format!(
                "Failed to connect to daemon socket '{}': {}",
                socket_name, error
            ));
        }

        Ok(unsafe { UnixStream::from_raw_fd(fd) })
    }

    fn send_jsonrpc(
        &self,
        method: &str,
        params: Value,
        mut on_chunk: Option<&mut dyn FnMut(&str)>,
    ) -> Result<RpcEnvelope, String> {
        let mut stream = Self::connect_socket(&Self::resolved_socket_name(self.socket_path.as_deref()))?;
        let timeout = Duration::from_millis(self.timeout_ms);
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set IPC read timeout: {}", e))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set IPC write timeout: {}", e))?;
        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": 1,
            "params": params,
        });

        Self::write_frame(&mut stream, &request.to_string())?;

        let mut stream_received = false;
        loop {
            let raw = Self::read_frame(&mut stream, timeout)?;
            let payload: Value =
                serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON response: {}", e))?;

            if payload.get("method").and_then(Value::as_str) == Some("stream_chunk") {
                stream_received = true;
                if let Some(chunk) = payload
                    .get("params")
                    .and_then(|params| params.get("chunk"))
                    .and_then(Value::as_str)
                {
                    if let Some(callback) = on_chunk.as_mut() {
                        (*callback)(chunk);
                    }
                }
                continue;
            }

            return Ok(RpcEnvelope {
                payload,
                stream_received,
            });
        }
    }

    fn write_frame(stream: &mut UnixStream, payload: &str) -> Result<(), String> {
        let len_bytes = (payload.len() as u32).to_be_bytes();
        stream
            .write_all(&len_bytes)
            .map_err(|e| format!("IPC write len failed: {}", e))?;
        stream
            .write_all(payload.as_bytes())
            .map_err(|e| format!("IPC write body failed: {}", e))?;
        Ok(())
    }

    fn read_frame(stream: &mut UnixStream, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        let mut len_buf = [0u8; 4];
        read_exact_with_retry(stream, &mut len_buf, deadline, "read len")?;
        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len == 0 || payload_len > MAX_IPC_MESSAGE_SIZE {
            return Err(format!("Invalid IPC payload size: {}", payload_len));
        }

        let mut buffer = vec![0u8; payload_len];
        read_exact_with_retry(stream, &mut buffer, deadline, "read body")?;
        String::from_utf8(buffer).map_err(|e| format!("Invalid UTF-8 response: {}", e))
    }
}

impl Default for TizenClaw {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TizenClaw {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Thread-safe shared handle for use in C FFI.
pub type TizenClawHandle = Arc<Mutex<TizenClaw>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    enum ScriptedRead {
        Bytes(Vec<u8>),
        Error(ErrorKind),
        Eof,
    }

    struct ScriptedReader {
        steps: VecDeque<ScriptedRead>,
    }

    impl ScriptedReader {
        fn new(steps: Vec<ScriptedRead>) -> Self {
            Self {
                steps: VecDeque::from(steps),
            }
        }
    }

    impl Read for ScriptedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match self.steps.pop_front() {
                Some(ScriptedRead::Bytes(bytes)) => {
                    let len = bytes.len().min(buf.len());
                    buf[..len].copy_from_slice(&bytes[..len]);
                    Ok(len)
                }
                Some(ScriptedRead::Error(kind)) => Err(std::io::Error::from(kind)),
                Some(ScriptedRead::Eof) | None => Ok(0),
            }
        }
    }

    #[test]
    fn read_exact_with_retry_recovers_from_would_block() {
        let mut reader = ScriptedReader::new(vec![
            ScriptedRead::Error(ErrorKind::WouldBlock),
            ScriptedRead::Bytes(vec![0xAA, 0xBB]),
            ScriptedRead::Error(ErrorKind::Interrupted),
            ScriptedRead::Bytes(vec![0xCC, 0xDD]),
        ]);
        let mut buf = [0u8; 4];

        let result = read_exact_with_retry(
            &mut reader,
            &mut buf,
            Instant::now() + Duration::from_millis(500),
            "read body",
        );

        assert!(result.is_ok(), "expected retryable reads to succeed");
        assert_eq!(buf, [0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn read_exact_with_retry_fails_after_deadline() {
        let mut reader = ScriptedReader::new(vec![ScriptedRead::Error(ErrorKind::WouldBlock)]);
        let mut buf = [0u8; 4];

        let result = read_exact_with_retry(
            &mut reader,
            &mut buf,
            Instant::now(),
            "read len",
        );

        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("IPC read len failed"),
            "expected read-len context in error"
        );
    }

    #[test]
    fn read_exact_with_retry_reports_unexpected_eof() {
        let mut reader = ScriptedReader::new(vec![
            ScriptedRead::Bytes(vec![0xAA, 0xBB]),
            ScriptedRead::Eof,
        ]);
        let mut buf = [0u8; 4];

        let result = read_exact_with_retry(
            &mut reader,
            &mut buf,
            Instant::now() + Duration::from_millis(500),
            "read body",
        );

        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("unexpected EOF"),
            "expected EOF detail in error"
        );
    }
}
