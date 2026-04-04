//! Container engine — IPC client for TizenClaw Tool Executor.
//!
//! Supports oneshot, streaming, and interactive modes via 
//! abstract namespace Unix domain socket and length-prefixed JSON protocol.

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

const SOCKET_NAME: &str = "tizenclaw-tool-executor.sock";
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
            unsafe { libc::close(fd); }
            // Let the caller fallback to direct execution or fail.
            return Err("Failed to connect to tizenclaw-tool-executor".into());
        }

        let opts = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        unsafe { libc::fcntl(fd, libc::F_SETFL, opts | libc::O_NONBLOCK); }

        use std::os::unix::io::FromRawFd;
        let std_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
        UnixStream::from_std(std_stream).map_err(|e| format!("Tokio wrap failed: {}", e))
    }

    async fn send_payload(stream: &mut UnixStream, val: &Value) -> Result<(), String> {
        let payload = val.to_string();
        let len = (payload.len() as u32).to_be_bytes();
        stream.write_all(&len).await.map_err(|e| format!("Write len failed: {}", e))?;
        stream.write_all(payload.as_bytes()).await.map_err(|e| format!("Write payload failed: {}", e))?;
        Ok(())
    }

    async fn recv_payload(stream: &mut UnixStream) -> Result<Value, String> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.map_err(|e| format!("Read len failed: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_PAYLOAD {
            return Err("Payload too large".into());
        }
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await.map_err(|e| format!("Read payload failed: {}", e))?;
        serde_json::from_slice(&buf).map_err(|e| format!("JSON parse failed: {}", e))
    }

    /// Backwards compatible execute endpoint for oneshot requests inside ToolDispatcher.
    pub async fn execute_oneshot(&self, binary: &str, args: &[&str]) -> Result<Value, String> {
        let mut stream = match Self::connect_ipc().await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("IPC fallback to direct spawned command due to: {}", e);
                return self.execute_direct(binary, args).await;
            }
        };

        let req = json!({
            "command": "execute",
            "tool_name": binary,
            "args": args,
            "mode": "oneshot"
        });

        Self::send_payload(&mut stream, &req).await?;

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = -1;

        loop {
            let resp = match Self::recv_payload(&mut stream).await {
                Ok(v) => v,
                Err(e) => return Err(e),
            };

            if let Some(status) = resp["status"].as_str() {
                if status == "error" {
                    return Err(resp["message"].as_str().unwrap_or("Unknown executor error").to_string());
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

    /// Stream executor output dynamically
    pub async fn execute_streaming(&self, binary: &str, args: &[&str]) -> Result<mpsc::Receiver<Value>, String> {
        let mut stream = Self::connect_ipc().await?;
        let req = json!({
            "command": "execute",
            "tool_name": binary,
            "args": args,
            "mode": "streaming"
        });
        Self::send_payload(&mut stream, &req).await?;

        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Ok(v) = Self::recv_payload(&mut stream).await {
                let is_exit = v["event"].as_str() == Some("exit");
                let _ = tx.send(v).await;
                if is_exit { break; }
            }
        });
        Ok(rx)
    }

    /// Fallback direct mode in case executor daemon isn't running or crashes
    async fn execute_direct(&self, binary: &str, args: &[&str]) -> Result<Value, String> {
        match tokio::process::Command::new(binary).args(args).output().await {
            Ok(output) => {
                Ok(json!({
                    "exit_code": output.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                    "success": output.status.success()
                }))
            }
            Err(e) => Err(format!("Direct execute failed: {}", e)),
        }
    }
}
