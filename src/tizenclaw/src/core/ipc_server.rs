//! IPC server — Unix domain socket with JSON-RPC 2.0 protocol.

use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::core::agent_core::AgentCore;

const MAX_CONCURRENT_CLIENTS: usize = 8;
const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10MB

pub struct IpcServer {
    running: Arc<AtomicBool>,
    active_clients: Arc<AtomicUsize>,
}

impl Default for IpcServer {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcServer {
    pub fn new() -> Self {
        IpcServer {
            running: Arc::new(AtomicBool::new(false)),
            active_clients: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Start the IPC server in a new thread.
    pub fn start(&self, agent: Arc<AgentCore>) -> std::thread::JoinHandle<()> {
        let running = self.running.clone();
        let active_clients = self.active_clients.clone();
        running.store(true, Ordering::SeqCst);

        std::thread::spawn(move || {
            Self::server_loop(running, active_clients, agent);
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn server_loop(
        running: Arc<AtomicBool>,
        active_clients: Arc<AtomicUsize>,
        agent: Arc<AgentCore>,
    ) {
        // Abstract namespace socket via raw libc API
        let sock = unsafe {
            let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
            if fd < 0 {
                log::error!("Failed to create IPC socket");
                return;
            }

            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as u16;
            let name = b"tizenclaw.sock";
            // Abstract namespace: sun_path[0] = 0, then name
            for (i, b) in name.iter().enumerate() {
                addr.sun_path[1 + i] = *b as i8;
            }
            let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

            if libc::bind(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
                log::error!("Failed to bind IPC socket");
                libc::close(fd);
                return;
            }

            if libc::listen(fd, 5) < 0 {
                log::error!("Failed to listen IPC socket");
                libc::close(fd);
                return;
            }

            // Set accept timeout so the thread wakes up to check stop flag.
            // Without this, accept() blocks forever and SIGTERM causes hang.
            let timeout = libc::timeval {
                tv_sec: 1,
                tv_usec: 0,
            };
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &timeout as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );

            fd
        };

        log::info!("IPC Server listening on \\0tizenclaw.sock");

        while running.load(Ordering::SeqCst) {
            let client_fd = unsafe { libc::accept(sock, std::ptr::null_mut(), std::ptr::null_mut()) };
            if client_fd < 0 {
                // Timeout or error — loop back and re-check running flag
                continue;
            }

            if active_clients.load(Ordering::SeqCst) >= MAX_CONCURRENT_CLIENTS {
                log::warn!("Max concurrent clients reached");
                let busy = json!({"jsonrpc":"2.0","error":{"code":-32000,"message":"Server busy"}}).to_string();
                Self::send_response(client_fd, &busy);
                unsafe { libc::close(client_fd); }
                continue;
            }

            let agent = agent.clone();
            let active = active_clients.clone();
            active.fetch_add(1, Ordering::SeqCst);

            std::thread::spawn(move || {
                Self::handle_client(client_fd, agent);
                active.fetch_sub(1, Ordering::SeqCst);
                unsafe { libc::close(client_fd); }
            });
        }

        unsafe { libc::close(sock); }
        log::info!("IPC Server stopped")
    }

    fn handle_client(fd: i32, agent: Arc<AgentCore>) {
        loop {
            // Read 4-byte length prefix
            let mut len_buf = [0u8; 4];
            let n = unsafe { libc::recv(fd, len_buf.as_mut_ptr() as *mut _, 4, libc::MSG_WAITALL) };
            if n != 4 {
                break;
            }

            let payload_len = u32::from_be_bytes(len_buf) as usize;
            if payload_len > MAX_PAYLOAD_SIZE {
                log::error!("Payload too large: {}", payload_len);
                break;
            }

            let mut buf = vec![0u8; payload_len];
            let n = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut _, payload_len, libc::MSG_WAITALL) };
            if n as usize != payload_len {
                break;
            }

            let raw_msg = String::from_utf8_lossy(&buf).to_string();
            if raw_msg.is_empty() {
                break;
            }

            log::info!("IPC msg received ({} bytes)", raw_msg.len());

            let response = Self::dispatch_request(&raw_msg, &agent, fd);
            Self::send_response(fd, &response);
        }
    }

    fn dispatch_request(raw: &str, agent: &Arc<AgentCore>, _client_fd: i32) -> String {
        let req: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => {
                // Plain text prompt — no lock needed, AgentCore handles its own locking
                let fut = agent.process_prompt("default", raw, None);
                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| handle.block_on(fut))
                } else {
                    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(fut)
                };
                return json!({"jsonrpc":"2.0","id":null,"result":{"text":result}}).to_string();
            }
        };

        if req.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") || req.get("method").is_none() {
            return json!({"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid Request"},"id":null}).to_string();
        }

        let method = req["method"].as_str().unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));
        let req_id = req.get("id").cloned().unwrap_or(Value::Null);

        let result: Value = match method {
            "prompt" => {
                let session_id = params["session_id"].as_str().unwrap_or("default");
                let text = params["text"].as_str().unwrap_or("");
                if text.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Empty prompt"},"id":req_id}).to_string();
                }
                let fut = agent.process_prompt(session_id, text, None);
                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| handle.block_on(fut))
                } else {
                    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(fut)
                };
                json!({"text": result})
            }
            "get_usage" => {
                if let Some(ss_ref) = agent.get_session_store() {
                    let usage = ss_ref.store().load_daily_usage("");
                    json!({
                        "prompt_tokens": usage.total_prompt_tokens,
                        "completion_tokens": usage.total_completion_tokens,
                        "total_requests": usage.total_requests
                    })
                } else {
                    json!({"error": "No session store"})
                }
            }
            _ => {
                return json!({"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":req_id}).to_string();
            }
        };

        json!({"jsonrpc":"2.0","id":req_id,"result":result}).to_string()
    }

    fn send_response(fd: i32, response: &str) {
        let len = (response.len() as u32).to_be_bytes();
        unsafe {
            libc::write(fd, len.as_ptr() as *const _, 4);
            let mut sent: usize = 0;
            while sent < response.len() {
                let n = libc::write(
                    fd,
                    response.as_ptr().add(sent) as *const _,
                    response.len() - sent,
                );
                if n <= 0 { break; }
                sent += n as usize;
            }
        }
    }
}
