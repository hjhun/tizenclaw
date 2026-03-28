//! Container engine — sandbox for tool execution.
//!
//! Executes commands in an isolated environment via fork/exec or
//! Unix domain socket IPC to the tool-executor service.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;

const TOOL_EXECUTOR_SOCKET: &str = "/run/tizenclaw-tool-executor.socket";

pub struct ContainerEngine {
    use_ipc: bool,
}

impl Default for ContainerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerEngine {
    pub fn new() -> Self {
        // Use IPC if the tool executor socket exists
        let use_ipc = std::path::Path::new(TOOL_EXECUTOR_SOCKET).exists();
        ContainerEngine { use_ipc }
    }

    /// Execute a skill (binary) with arguments, returning stdout.
    pub async fn execute_skill(&self, binary: &str, args: &[&str], timeout_secs: u64) -> Result<String, String> {
        if self.use_ipc {
            self.execute_via_ipc(binary, args, timeout_secs).await
        } else {
            self.execute_direct(binary, args, timeout_secs).await
        }
    }

    /// Execute Shell code, returning stdout.
    pub async fn execute_code(&self, code: &str) -> Result<String, String> {
        self.execute_direct("sh", &["-c", code], 30).await
    }

    async fn execute_direct(&self, binary: &str, args: &[&str], _timeout_secs: u64) -> Result<String, String> {
        match Command::new(binary).args(args).output().await {
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

    async fn execute_via_ipc(&self, binary: &str, args: &[&str], _timeout_secs: u64) -> Result<String, String> {
        let mut stream = UnixStream::connect(TOOL_EXECUTOR_SOCKET).await
            .map_err(|e| format!("IPC connect failed: {}", e))?;
        
        let mut request = binary.to_string();
        for arg in args {
            request.push('\0');
            request.push_str(arg);
        }
        request.push('\n');

        stream.write_all(request.as_bytes()).await
            .map_err(|e| format!("IPC write failed: {}", e))?;

        let mut response = String::new();
        stream.read_to_string(&mut response).await
            .map_err(|e| format!("IPC read failed: {}", e))?;

        Ok(response)
    }
}
