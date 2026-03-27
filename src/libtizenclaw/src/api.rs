//! Safe Rust API for TizenClaw.
//!
//! This module provides a safe, ergonomic Rust interface
//! wrapping the agent's core functionality. Used internally
//! by the C FFI layer and available to Rust consumers via `rlib`.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

/// Process uptime and system metrics.
fn parse_proc_status() -> (i64, i64, i32) {
    let mut rss_kb: i64 = 0;
    let mut vm_kb: i64 = 0;
    let mut threads: i32 = 0;

    if let Ok(content) = std::fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("VmRSS:") {
                rss_kb = val.trim().split_whitespace().next()
                    .and_then(|s| s.parse().ok()).unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("VmSize:") {
                vm_kb = val.trim().split_whitespace().next()
                    .and_then(|s| s.parse().ok()).unwrap_or(0);
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
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
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
    if sys_uptime > start_secs { sys_uptime - start_secs } else { 0.0 }
}

/// TizenClaw agent — safe Rust API.
///
/// # Example (Rust)
/// ```rust,no_run
/// use libtizenclaw::api::TizenClaw;
///
/// let mut agent = TizenClaw::new();
/// agent.initialize().unwrap();
/// let response = agent.process_prompt("default", "Hello!").unwrap();
/// println!("{}", response);
/// ```
pub struct TizenClaw {
    initialized: bool,
    /// Tool names loaded during initialization.
    tools: Vec<String>,
    /// IPC socket path for the running daemon.
    ipc_path: String,
}

impl TizenClaw {
    /// Create a new agent instance.
    pub fn new() -> Self {
        TizenClaw {
            initialized: false,
            tools: Vec::new(),
            ipc_path: "/run/tizenclaw.sock".into(),
        }
    }

    /// Initialize the agent: discovers tools, connects to daemon IPC.
    pub fn initialize(&mut self) -> Result<(), String> {
        if self.initialized {
            return Err("Already initialized".into());
        }

        // Discover tools from the tools directory
        self.tools = Self::discover_tools();
        self.initialized = true;

        log::info!("TizenClaw API initialized ({} tools)", self.tools.len());
        Ok(())
    }

    /// Check if agent is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Process a prompt via IPC to the running daemon.
    pub fn process_prompt(&self, session_id: &str, prompt: &str) -> Result<String, String> {
        if !self.initialized {
            return Err("Not initialized".into());
        }

        let request = json!({
            "method": "process",
            "session_id": session_id,
            "prompt": prompt
        });

        self.ipc_call(&request)
    }

    /// Clear a session's conversation history.
    pub fn clear_session(&self, session_id: &str) -> Result<(), String> {
        if !self.initialized {
            return Err("Not initialized".into());
        }

        let request = json!({
            "method": "clear_session",
            "session_id": session_id
        });

        self.ipc_call(&request)?;
        Ok(())
    }

    /// Get agent status as JSON string.
    pub fn get_status(&self) -> Result<String, String> {
        Ok(json!({
            "status": if self.initialized { "running" } else { "not_initialized" },
            "version": "1.0.0",
            "tools_count": self.tools.len()
        }).to_string())
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
        }).to_string())
    }

    /// Get available tools as JSON string.
    pub fn get_tools(&self) -> Result<String, String> {
        if !self.initialized {
            return Err("Not initialized".into());
        }

        let tools_json: Vec<Value> = self.tools.iter()
            .map(|name| json!({"name": name}))
            .collect();
        Ok(Value::Array(tools_json).to_string())
    }

    /// Execute a tool by name with JSON arguments.
    pub fn execute_tool(&self, tool_name: &str, args_json: &str) -> Result<String, String> {
        if !self.initialized {
            return Err("Not initialized".into());
        }

        let args: Value = serde_json::from_str(args_json)
            .map_err(|e| format!("Invalid JSON args: {}", e))?;

        let request = json!({
            "method": "execute_tool",
            "tool_name": tool_name,
            "args": args
        });

        self.ipc_call(&request)
    }

    /// Reload skill manifests.
    pub fn reload_skills(&mut self) -> Result<(), String> {
        if !self.initialized {
            return Err("Not initialized".into());
        }

        self.tools = Self::discover_tools();
        log::info!("Skills reloaded ({} tools)", self.tools.len());
        Ok(())
    }

    /// Shutdown and release resources.
    pub fn shutdown(&mut self) {
        if self.initialized {
            self.initialized = false;
            self.tools.clear();
            log::info!("TizenClaw API shutdown");
        }
    }

    // ── Internal helpers ───────────────────────

    /// Send an IPC request to the TizenClaw daemon.
    fn ipc_call(&self, request: &Value) -> Result<String, String> {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let mut stream = UnixStream::connect(&self.ipc_path)
            .map_err(|e| format!("IPC connect failed: {}", e))?;

        stream.set_read_timeout(Some(std::time::Duration::from_secs(60)))
            .map_err(|e| format!("Set timeout failed: {}", e))?;

        let payload = request.to_string();
        let len_bytes = (payload.len() as u32).to_le_bytes();
        stream.write_all(&len_bytes)
            .map_err(|e| format!("IPC write len failed: {}", e))?;
        stream.write_all(payload.as_bytes())
            .map_err(|e| format!("IPC write body failed: {}", e))?;

        // Read response length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)
            .map_err(|e| format!("IPC read len failed: {}", e))?;
        let resp_len = u32::from_le_bytes(len_buf) as usize;

        if resp_len > 10 * 1024 * 1024 {
            return Err("Response too large".into());
        }

        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf)
            .map_err(|e| format!("IPC read body failed: {}", e))?;

        String::from_utf8(resp_buf)
            .map_err(|e| format!("Invalid UTF-8 response: {}", e))
    }

    /// Discover tool names from all subdirectories under /opt/usr/share/tizen-tools.
    fn discover_tools() -> Vec<String> {
        let mut tools = Vec::new();
        let root = "/opt/usr/share/tizen-tools";

        let root_entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => return tools,
        };

        for root_entry in root_entries.flatten() {
            let sub_dir = root_entry.path();
            if !sub_dir.is_dir() { continue; }

            if let Ok(entries) = std::fs::read_dir(&sub_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() { continue; }
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    if !name.starts_with('.') && !name.is_empty() {
                        tools.push(name);
                    }
                }
            }
        }

        tools.sort();
        tools.dedup();
        tools
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
