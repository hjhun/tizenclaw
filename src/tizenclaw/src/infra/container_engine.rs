//! Container engine — sandbox for tool execution.
//!
//! Executes commands in an isolated environment via fork/exec or
//! Unix domain socket IPC to the tool-executor service.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::Command;

const TOOL_EXECUTOR_SOCKET: &str = "/run/tizenclaw-tool-executor.socket";

pub struct ContainerEngine {
    use_ipc: bool,
}

impl ContainerEngine {
    pub fn new() -> Self {
        // Use IPC if the tool executor socket exists
        let use_ipc = std::path::Path::new(TOOL_EXECUTOR_SOCKET).exists();
        ContainerEngine { use_ipc }
    }

    /// Execute a skill (binary) with arguments, returning stdout.
    pub fn execute_skill(&self, binary: &str, args: &[&str], timeout_secs: u64) -> Result<String, String> {
        if self.use_ipc {
            self.execute_via_ipc(binary, args, timeout_secs)
        } else {
            self.execute_direct(binary, args, timeout_secs)
        }
    }

    /// Execute Python code, returning stdout.
    pub fn execute_code(&self, code: &str) -> Result<String, String> {
        self.execute_direct("python3", &["-c", code], 30)
    }

    fn execute_direct(&self, binary: &str, args: &[&str], timeout_secs: u64) -> Result<String, String> {
        let result = Command::new(binary)
            .args(args)
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(format!("Exit code {}: {}", output.status.code().unwrap_or(-1), stderr))
                }
            }
            Err(e) => Err(format!("Failed to execute {}: {}", binary, e)),
        }
    }

    fn execute_via_ipc(&self, binary: &str, args: &[&str], timeout_secs: u64) -> Result<String, String> {
        let mut stream = UnixStream::connect(TOOL_EXECUTOR_SOCKET)
            .map_err(|e| format!("IPC connect failed: {}", e))?;
        
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(timeout_secs)))
            .ok();

        // Serialize request: binary\0arg1\0arg2\0...\n
        let mut request = binary.to_string();
        for arg in args {
            request.push('\0');
            request.push_str(arg);
        }
        request.push('\n');

        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("IPC write failed: {}", e))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| format!("IPC read failed: {}", e))?;

        Ok(response)
    }
}
