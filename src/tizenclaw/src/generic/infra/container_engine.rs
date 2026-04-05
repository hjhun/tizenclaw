//! Container engine — IPC client for TizenClaw Tool Executor.
//!
//! Execution priority:
//! 1. Connect to the executor socket, which lets systemd socket activation
//!    wake the daemon when that environment is available.
//! 2. If the socket path is unavailable, spawn `tizenclaw-tool-executor`
//!    as a subprocess and speak the same length-prefixed JSON protocol
//!    over stdio pipes.
//! 3. Only if both executor paths fail, execute directly inside
//!    `tizenclaw` as the final safety net.

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::mpsc;

const SOCKET_NAME: &str = "tizenclaw-tool-executor.sock";
const EXECUTOR_BINARY: &str = "tizenclaw-tool-executor";
const MAX_PAYLOAD: usize = 10 * 1024 * 1024;

pub struct ContainerEngine;

impl Default for ContainerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerEngine {
    pub fn new() -> Self {
        Self
    }

    async fn connect_ipc() -> Result<UnixStream, String> {
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err("Failed to create socket".into());
        }

        let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let bytes = SOCKET_NAME.as_bytes();
        addr.sun_path[1..1 + bytes.len()].copy_from_slice(unsafe {
            std::slice::from_raw_parts(bytes.as_ptr() as _, bytes.len())
        });

        let len = (std::mem::size_of::<libc::sa_family_t>() + 1 + bytes.len()) as libc::socklen_t;
        let ret = unsafe { libc::connect(fd, &addr as *const _ as _, len) };
        if ret < 0 {
            unsafe { libc::close(fd) };
            return Err("Failed to connect to tizenclaw-tool-executor".into());
        }

        let opts = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        unsafe { libc::fcntl(fd, libc::F_SETFL, opts | libc::O_NONBLOCK) };

        use std::os::unix::io::FromRawFd;
        let std_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
        UnixStream::from_std(std_stream).map_err(|e| format!("Tokio wrap failed: {}", e))
    }

    async fn send_payload<W: AsyncWrite + Unpin>(
        writer: &mut W,
        val: &Value,
    ) -> Result<(), String> {
        let payload = val.to_string();
        let len = (payload.len() as u32).to_be_bytes();
        writer
            .write_all(&len)
            .await
            .map_err(|e| format!("Write len failed: {}", e))?;
        writer
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| format!("Write payload failed: {}", e))?;
        writer
            .flush()
            .await
            .map_err(|e| format!("Flush failed: {}", e))?;
        Ok(())
    }

    async fn recv_payload<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Value, String> {
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| format!("Read len failed: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_PAYLOAD {
            return Err("Payload too large".into());
        }
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(|e| format!("Read payload failed: {}", e))?;
        serde_json::from_slice(&buf).map_err(|e| format!("JSON parse failed: {}", e))
    }

    fn executor_binary_candidates() -> Vec<std::path::PathBuf> {
        let mut candidates = vec![std::path::PathBuf::from(format!(
            "/usr/bin/{}",
            EXECUTOR_BINARY
        ))];

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                candidates.push(parent.join(EXECUTOR_BINARY));
            }
        }

        candidates
    }

    fn executor_binary_path() -> Result<std::path::PathBuf, String> {
        Self::executor_binary_candidates()
            .into_iter()
            .find(|path| path.exists())
            .ok_or_else(|| "Failed to locate tizenclaw-tool-executor binary".to_string())
    }

    async fn spawn_executor_stdio() -> Result<(Child, ChildStdin, ChildStdout), String> {
        let binary = Self::executor_binary_path()?;
        let mut child = tokio::process::Command::new(binary)
            .arg("--stdio")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn stdio executor: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture executor stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture executor stdout".to_string())?;

        Ok((child, stdin, stdout))
    }

    async fn collect_oneshot_from_transport<R: AsyncRead + Unpin>(
        reader: &mut R,
    ) -> Result<Value, String> {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = -1;

        loop {
            let resp = Self::recv_payload(reader).await?;

            if let Some(status) = resp["status"].as_str() {
                if status == "error" {
                    return Err(resp["message"]
                        .as_str()
                        .unwrap_or("Unknown executor error")
                        .to_string());
                }
            }

            match resp["event"].as_str() {
                Some("stdout") => stdout.push_str(resp["data"].as_str().unwrap_or("")),
                Some("stderr") => stderr.push_str(resp["data"].as_str().unwrap_or("")),
                Some("exit") => {
                    exit_code = resp["code"].as_i64().unwrap_or(-1) as i32;
                    break;
                }
                _ => {}
            }
        }

        Ok(json!({
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "success": exit_code == 0
        }))
    }

    async fn execute_oneshot_via_socket(&self, req: &Value) -> Result<Value, String> {
        let mut stream = Self::connect_ipc().await?;
        Self::send_payload(&mut stream, req).await?;
        Self::collect_oneshot_from_transport(&mut stream).await
    }

    async fn execute_oneshot_via_stdio_executor(&self, req: &Value) -> Result<Value, String> {
        let (mut child, mut stdin, mut stdout) = Self::spawn_executor_stdio().await?;
        Self::send_payload(&mut stdin, req).await?;
        drop(stdin);

        let result = Self::collect_oneshot_from_transport(&mut stdout).await;
        let _ = child.wait().await;
        result
    }

    async fn stream_from_transport<R: AsyncRead + Unpin + Send + 'static>(
        mut reader: R,
    ) -> Result<mpsc::Receiver<Value>, String> {
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Ok(v) = Self::recv_payload(&mut reader).await {
                let is_exit = v["event"].as_str() == Some("exit");
                let _ = tx.send(v).await;
                if is_exit {
                    break;
                }
            }
        });
        Ok(rx)
    }

    async fn execute_streaming_via_socket(
        &self,
        req: &Value,
    ) -> Result<mpsc::Receiver<Value>, String> {
        let mut stream = Self::connect_ipc().await?;
        Self::send_payload(&mut stream, req).await?;
        Self::stream_from_transport(stream).await
    }

    async fn execute_streaming_via_stdio_executor(
        &self,
        req: &Value,
    ) -> Result<mpsc::Receiver<Value>, String> {
        let (mut child, mut stdin, stdout) = Self::spawn_executor_stdio().await?;
        Self::send_payload(&mut stdin, req).await?;
        drop(stdin);
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Self::stream_from_transport(stdout).await
    }

    /// Backwards compatible execute endpoint for oneshot requests inside ToolDispatcher.
    pub async fn execute_oneshot(
        &self,
        binary: &str,
        args: &[&str],
        cwd: Option<&str>,
    ) -> Result<Value, String> {
        let req = json!({
            "command": "execute",
            "tool_name": binary,
            "args": args,
            "mode": "oneshot",
            "cwd": cwd
        });

        match self.execute_oneshot_via_socket(&req).await {
            Ok(result) => Ok(result),
            Err(socket_err) => {
                log::warn!(
                    "Socket executor unavailable, trying stdio executor: {}",
                    socket_err
                );
                match self.execute_oneshot_via_stdio_executor(&req).await {
                    Ok(result) => Ok(result),
                    Err(stdio_err) => {
                        log::warn!(
                            "Executor stdio fallback unavailable, using direct execution: {}",
                            stdio_err
                        );
                        self.execute_direct(binary, args, cwd).await
                    }
                }
            }
        }
    }

    /// Stream executor output dynamically.
    pub async fn execute_streaming(
        &self,
        binary: &str,
        args: &[&str],
        cwd: Option<&str>,
    ) -> Result<mpsc::Receiver<Value>, String> {
        let req = json!({
            "command": "execute",
            "tool_name": binary,
            "args": args,
            "mode": "streaming",
            "cwd": cwd
        });

        match self.execute_streaming_via_socket(&req).await {
            Ok(rx) => Ok(rx),
            Err(socket_err) => {
                log::warn!(
                    "Socket streaming unavailable, trying stdio executor: {}",
                    socket_err
                );
                match self.execute_streaming_via_stdio_executor(&req).await {
                    Ok(rx) => Ok(rx),
                    Err(stdio_err) => {
                        log::warn!(
                            "Executor streaming stdio fallback unavailable, using direct execution: {}",
                            stdio_err
                        );
                        let output = self.execute_direct(binary, args, cwd).await?;
                        let (tx, rx) = mpsc::channel(4);
                        let stdout = output["stdout"].as_str().unwrap_or("").to_string();
                        let stderr = output["stderr"].as_str().unwrap_or("").to_string();
                        let exit_code = output["exit_code"].as_i64().unwrap_or(-1);
                        tokio::spawn(async move {
                            if !stdout.is_empty() {
                                let _ = tx.send(json!({"event": "stdout", "data": stdout})).await;
                            }
                            if !stderr.is_empty() {
                                let _ = tx.send(json!({"event": "stderr", "data": stderr})).await;
                            }
                            let _ = tx.send(json!({"event": "exit", "code": exit_code})).await;
                        });
                        Ok(rx)
                    }
                }
            }
        }
    }

    /// Fallback direct mode only when executor delegation is unavailable.
    async fn execute_direct(
        &self,
        binary: &str,
        args: &[&str],
        cwd: Option<&str>,
    ) -> Result<Value, String> {
        let mut command = tokio::process::Command::new(binary);
        command.args(args);
        if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
            command.current_dir(cwd);
        }

        match command.output().await
        {
            Ok(output) => Ok(json!({
                "exit_code": output.status.code().unwrap_or(-1),
                "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                "success": output.status.success()
            })),
            Err(e) => Err(format!("Direct execute failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ContainerEngine;

    #[test]
    fn executor_candidates_include_system_binary() {
        let candidates = ContainerEngine::executor_binary_candidates();
        assert!(candidates
            .iter()
            .any(|path| path == std::path::Path::new("/usr/bin/tizenclaw-tool-executor")));
    }
}
