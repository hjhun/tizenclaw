//! Safe Rust API for TizenClaw.
//!
//! This module provides a safe, ergonomic Rust interface wrapping the daemon's
//! IPC surface. It is used internally by the C FFI layer and is also available
//! to Rust consumers via `rlib`.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::io::FromRawFd;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const MAX_IPC_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
const IPC_TIMEOUT_SECS: u64 = 300;
const IPC_RETRY_SLEEP_MS: u64 = 25;
const ABSTRACT_SOCKET_NAME: &[u8] = b"tizenclaw.sock";

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
/// let response = agent.process_prompt("default", "Hello!").unwrap();
/// log::info!("{}", response);
/// ```
pub struct TizenClaw {
    initialized: bool,
    /// Tool names loaded during initialization.
    tools: Vec<String>,
}

impl TizenClaw {
    /// Create a new agent instance.
    pub fn new() -> Self {
        TizenClaw {
            initialized: false,
            tools: Vec::new(),
        }
    }

    /// Initialize the agent: discovers tools and prepares daemon IPC usage.
    pub fn initialize(&mut self) -> Result<(), String> {
        if self.initialized {
            return Err("Already initialized".into());
        }

        self.tools = Self::discover_tools();
        self.initialized = true;

        log::info!("TizenClaw API initialized ({} tools)", self.tools.len());
        Ok(())
    }

    /// Check if agent is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Process a prompt synchronously.
    pub fn process_prompt(&self, session_id: &str, prompt: &str) -> Result<String, String> {
        Ok(self
            .process_prompt_inner(session_id, prompt, false, None)?
            .text)
    }

    /// Process a prompt while streaming partial chunks through the callback.
    pub fn process_prompt_streaming<F>(
        &self,
        session_id: &str,
        prompt: &str,
        on_chunk: F,
    ) -> Result<PromptResponse, String>
    where
        F: FnMut(&str),
    {
        let mut on_chunk = on_chunk;
        self.process_prompt_inner(session_id, prompt, true, Some(&mut on_chunk))
    }

    /// Clear a session's conversation history.
    pub fn clear_session(&self, _session_id: &str) -> Result<(), String> {
        self.ensure_initialized()?;
        Err("clear_session is not exposed by the daemon IPC server".into())
    }

    /// Get agent status as JSON string.
    pub fn get_status(&self) -> Result<String, String> {
        Ok(json!({
            "status": if self.initialized { "running" } else { "not_initialized" },
            "version": "1.0.0",
            "tools_count": self.tools.len()
        })
        .to_string())
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
        self.ensure_initialized()?;

        let result = self.call_method("bridge_list_tools", json!({ "allowed_tools": [] }))?;
        Ok(result
            .get("tools")
            .cloned()
            .unwrap_or_else(|| Value::Array(vec![]))
            .to_string())
    }

    /// Execute a tool by name with JSON arguments.
    pub fn execute_tool(&self, tool_name: &str, args_json: &str) -> Result<String, String> {
        self.ensure_initialized()?;

        let args: Value =
            serde_json::from_str(args_json).map_err(|e| format!("Invalid JSON args: {}", e))?;

        let result = self.call_method(
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
        self.ensure_initialized()?;

        self.tools = Self::discover_tools();
        log::info!("Skills reloaded ({} tools)", self.tools.len());
        Ok(())
    }

    /// Retrieve usage information as structured JSON.
    pub fn get_usage(
        &self,
        session_id: Option<&str>,
        baseline: Option<&Value>,
    ) -> Result<Value, String> {
        self.ensure_initialized()?;

        let mut params = json!({});
        if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
            params["session_id"] = Value::String(session_id.to_string());
        }
        if let Some(baseline) = baseline {
            params["baseline"] = baseline.clone();
        }

        self.call_method("get_usage", params)
    }

    /// Start the dashboard channel.
    pub fn start_dashboard(&self, port: Option<u16>) -> Result<Value, String> {
        self.ensure_initialized()?;

        let mut params = json!({ "name": "web_dashboard" });
        if let Some(port) = port {
            params["settings"] = json!({ "port": port });
        }
        self.call_method("start_channel", params)
    }

    /// Stop the dashboard channel.
    pub fn stop_dashboard(&self) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("stop_channel", json!({ "name": "web_dashboard" }))
    }

    /// Query the dashboard channel status.
    pub fn dashboard_status(&self) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("channel_status", json!({ "name": "web_dashboard" }))
    }

    /// Read LLM config content or a nested path.
    pub fn get_llm_config(&self, path: Option<&str>) -> Result<Value, String> {
        self.ensure_initialized()?;
        let params = match path {
            Some(path) => json!({ "path": path }),
            None => json!({}),
        };
        self.call_method("get_llm_config", params)
    }

    /// Set a nested LLM config value.
    pub fn set_llm_config(&self, path: &str, value: Value) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("set_llm_config", json!({ "path": path, "value": value }))
    }

    /// Remove a nested LLM config value.
    pub fn unset_llm_config(&self, path: &str) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("unset_llm_config", json!({ "path": path }))
    }

    /// Reload backend configuration.
    pub fn reload_llm_backends(&self) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("reload_llm_backends", json!({}))
    }

    /// Register an external tool or skill path.
    pub fn register_path(&self, kind: &str, path: &str) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("register_path", json!({ "kind": kind, "path": path }))
    }

    /// Unregister an external tool or skill path.
    pub fn unregister_path(&self, kind: &str, path: &str) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("unregister_path", json!({ "kind": kind, "path": path }))
    }

    /// List registered tool and skill paths.
    pub fn list_registered_paths(&self) -> Result<Value, String> {
        self.ensure_initialized()?;
        self.call_method("list_registered_paths", json!({}))
    }

    /// Shutdown and release resources.
    pub fn shutdown(&mut self) {
        if self.initialized {
            self.initialized = false;
            self.tools.clear();
            log::info!("TizenClaw API shutdown");
        }
    }

    fn process_prompt_inner(
        &self,
        session_id: &str,
        prompt: &str,
        stream: bool,
        on_chunk: Option<&mut dyn FnMut(&str)>,
    ) -> Result<PromptResponse, String> {
        self.ensure_initialized()?;

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

    fn ensure_initialized(&self) -> Result<(), String> {
        if self.initialized {
            Ok(())
        } else {
            Err("Not initialized".into())
        }
    }

    fn call_method(&self, method: &str, params: Value) -> Result<Value, String> {
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

    fn connect_daemon() -> Result<UnixStream, String> {
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err("Failed to create daemon IPC socket".into());
        }

        let connect_result = unsafe {
            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
            for (index, byte) in ABSTRACT_SOCKET_NAME.iter().enumerate() {
                addr.sun_path[index + 1] = *byte as libc::c_char;
            }
            let addr_len = (std::mem::size_of::<libc::sa_family_t>()
                + 1
                + ABSTRACT_SOCKET_NAME.len()) as libc::socklen_t;
            libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len)
        };

        if connect_result < 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(format!(
                "Failed to connect to daemon. Is tizenclaw running? {}",
                error
            ));
        }

        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(IPC_TIMEOUT_SECS)))
            .map_err(|e| format!("Failed to set IPC read timeout: {}", e))?;
        Ok(stream)
    }

    fn send_jsonrpc(
        &self,
        method: &str,
        params: Value,
        mut on_chunk: Option<&mut dyn FnMut(&str)>,
    ) -> Result<RpcEnvelope, String> {
        let mut stream = Self::connect_daemon()?;
        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": 1,
            "params": params,
        });

        Self::write_frame(&mut stream, &request.to_string())?;

        let mut stream_received = false;
        loop {
            let raw = Self::read_frame(&mut stream)?;
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

    fn read_frame(stream: &mut UnixStream) -> Result<String, String> {
        let deadline = Instant::now() + Duration::from_secs(IPC_TIMEOUT_SECS);
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

    /// Discover tool names from both the shared tool tree and the
    /// TizenClaw-owned embedded descriptor directory.
    fn discover_tools() -> Vec<String> {
        let mut tools = Vec::new();
        let data_dir = std::env::var("TIZENCLAW_DATA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                if std::path::Path::new("/etc/tizen-release").exists()
                    || std::path::Path::new("/opt/usr/share/tizenclaw").exists()
                {
                    std::path::PathBuf::from("/opt/usr/share/tizenclaw")
                } else {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                    std::path::PathBuf::from(home).join(".tizenclaw")
                }
            });
        let tools_dir = std::env::var("TIZENCLAW_TOOLS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("tools"));
        collect_tools_from_tree(&tools_dir.to_string_lossy(), &mut tools);
        collect_embedded_tool_names(&data_dir.join("embedded").to_string_lossy(), &mut tools);
        tools.sort();
        tools.dedup();
        tools
    }
}

fn collect_tools_from_tree(root: &str, tools: &mut Vec<String>) {
    let root_entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for root_entry in root_entries.flatten() {
        let sub_dir = root_entry.path();
        if !sub_dir.is_dir() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&sub_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_string();
                if !name.starts_with('.') && !name.is_empty() {
                    tools.push(name);
                }
            }
        }
    }
}

fn collect_embedded_tool_names(root: &str, tools: &mut Vec<String>) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !file_name.ends_with(".md")
            || file_name == "index.md"
            || file_name == "tools.md"
            || file_name.starts_with('.')
        {
            continue;
        }

        tools.push(file_name.trim_end_matches(".md").to_string());
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
