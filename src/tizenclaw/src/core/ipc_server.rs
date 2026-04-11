//! IPC server — Unix domain socket with JSON-RPC 2.0 protocol.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::channel::ChannelRegistry;
use crate::core::agent_core::AgentCore;
use crate::core::registration_store::RegistrationKind;

const MAX_CONCURRENT_CLIENTS: usize = 8;
const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10MB
static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

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
    fn configure_client_fd(fd: i32) {
        unsafe {
            let timeout = libc::timeval {
                tv_sec: 5,
                tv_usec: 0,
            };
            let _ = libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &timeout as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
            let _ = libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDTIMEO,
                &timeout as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
        }
    }

    pub fn new() -> Self {
        IpcServer {
            running: Arc::new(AtomicBool::new(false)),
            active_clients: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Start the IPC server in a new thread.
    pub fn start(
        &self,
        agent: Arc<AgentCore>,
        registry: Arc<Mutex<ChannelRegistry>>,
    ) -> std::thread::JoinHandle<()> {
        let running = self.running.clone();
        let active_clients = self.active_clients.clone();
        running.store(true, Ordering::SeqCst);
        let rt_handle = tokio::runtime::Handle::current();

        std::thread::spawn(move || {
            Self::server_loop(rt_handle, running, active_clients, agent, registry);
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn resolve_session_id(requested: Option<&str>, prefix: &str) -> String {
        let requested = requested.unwrap_or("").trim();
        if !requested.is_empty() {
            return requested.to_string();
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let seq = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{}_{}_{}", prefix, ts, seq)
    }

    fn server_loop(
        rt_handle: tokio::runtime::Handle,
        running: Arc<AtomicBool>,
        active_clients: Arc<AtomicUsize>,
        agent: Arc<AgentCore>,
        registry: Arc<Mutex<ChannelRegistry>>,
    ) {
        let sock = unsafe {
            let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
            if fd < 0 {
                log::error!("Failed to create IPC socket");
                return;
            }

            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as u16;
            let name = b"tizenclaw.sock";
            for (i, b) in name.iter().enumerate() {
                addr.sun_path[1 + i] = *b as libc::c_char;
            }
            let addr_len =
                (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

            if libc::bind(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
                log::error!("Failed to bind IPC socket");
                libc::close(fd);
                return;
            }
            if libc::listen(fd, 64) < 0 {
                log::error!("Failed to listen IPC socket");
                libc::close(fd);
                return;
            }

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
            let client_fd =
                unsafe { libc::accept(sock, std::ptr::null_mut(), std::ptr::null_mut()) };
            if client_fd < 0 {
                let errno = std::io::Error::last_os_error()
                    .raw_os_error()
                    .unwrap_or_default();
                if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK {
                    log::error!("IPC accept failed: errno={}", errno);
                }
                continue;
            }

            Self::configure_client_fd(client_fd);

            if active_clients.load(Ordering::SeqCst) >= MAX_CONCURRENT_CLIENTS {
                log::warn!("Max concurrent clients reached");
                let busy = json!({"jsonrpc":"2.0","error":{"code":-32000,"message":"Server busy"}})
                    .to_string();
                Self::send_response(client_fd, &busy);
                unsafe {
                    libc::close(client_fd);
                }
                continue;
            }

            let agent = agent.clone();
            let registry = registry.clone();
            let active = active_clients.clone();
            let rt_handle_clone = rt_handle.clone();
            active.fetch_add(1, Ordering::SeqCst);

            std::thread::spawn(move || {
                Self::handle_client(rt_handle_clone, client_fd, agent, registry);
                active.fetch_sub(1, Ordering::SeqCst);
                unsafe {
                    libc::close(client_fd);
                }
            });
        }

        unsafe {
            libc::close(sock);
        }
        log::info!("IPC Server stopped")
    }

    fn handle_client(
        rt_handle: tokio::runtime::Handle,
        fd: i32,
        agent: Arc<AgentCore>,
        registry: Arc<Mutex<ChannelRegistry>>,
    ) {
        loop {
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
            let n = unsafe {
                libc::recv(
                    fd,
                    buf.as_mut_ptr() as *mut _,
                    payload_len,
                    libc::MSG_WAITALL,
                )
            };
            if n as usize != payload_len {
                break;
            }

            let raw_msg = String::from_utf8_lossy(&buf).to_string();
            if raw_msg.is_empty() {
                break;
            }

            let response = Self::dispatch_request(&rt_handle, &raw_msg, &agent, &registry, fd);
            Self::send_response(fd, &response);
        }
    }

    fn dispatch_request(
        rt_handle: &tokio::runtime::Handle,
        raw: &str,
        agent: &Arc<AgentCore>,
        registry: &Arc<Mutex<ChannelRegistry>>,
        client_fd: i32,
    ) -> String {
        let req: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => {
                let fut = agent.process_prompt("default", raw, None);
                let result = tokio::task::block_in_place(|| rt_handle.block_on(fut));
                return json!({"jsonrpc":"2.0","id":null,"result":{"text":result}}).to_string();
            }
        };

        if req.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") || req.get("method").is_none()
        {
            return json!({"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid Request"},"id":null})
                .to_string();
        }

        let method = req["method"].as_str().unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));
        let req_id = req.get("id").cloned().unwrap_or(Value::Null);

        let result: Value = match method {
            "prompt" => {
                let session_id = Self::resolve_session_id(params["session_id"].as_str(), "ipc");
                let text = params["text"].as_str().unwrap_or("");
                let stream = params
                    .get("stream")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if text.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Empty prompt"},"id":req_id})
                        .to_string();
                }

                let result = if stream {
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
                    let req_id_clone = req_id.clone();
                    let fd_clone = client_fd;
                    rt_handle.spawn(async move {
                        while let Some(chunk) = rx.recv().await {
                            let stream_resp = json!({
                                "jsonrpc": "2.0",
                                "method": "stream_chunk",
                                "params": {"id": &req_id_clone, "chunk": chunk}
                            })
                            .to_string();
                            let _ = tokio::task::spawn_blocking(move || {
                                IpcServer::send_response(fd_clone, &stream_resp);
                            })
                            .await;
                        }
                    });
                    let on_chunk = move |chunk: &str| {
                        let _ = tx.send(chunk.to_string());
                    };
                    let fut = agent.process_prompt(&session_id, text, Some(&on_chunk));
                    tokio::task::block_in_place(|| rt_handle.block_on(fut))
                } else {
                    let fut = agent.process_prompt(&session_id, text, None);
                    tokio::task::block_in_place(|| rt_handle.block_on(fut))
                };

                json!({"text": result, "session_id": session_id})
            }

            "get_usage" => {
                if let Some(ss_ref) = agent.get_session_store() {
                    let session_id = params
                        .get("session_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim();
                    let date = params
                        .get("date")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim();
                    let usage = if session_id.is_empty() {
                        ss_ref.store().load_daily_usage(date)
                    } else {
                        ss_ref.store().load_token_usage(session_id)
                    };
                    let baseline = crate::storage::session_store::TokenUsage::from_json(
                        params.get("baseline"),
                    );
                    let delta = usage.diff_from(&baseline);
                    json!({
                        "scope": if session_id.is_empty() { "daily" } else { "session" },
                        "session_id": if session_id.is_empty() { Value::Null } else { Value::String(session_id.to_string()) },
                        "date": if session_id.is_empty() { Value::String(if date.is_empty() { "today".to_string() } else { date.to_string() }) } else { Value::Null },
                        "usage": usage.to_json(),
                        "delta": delta.to_json(),
                        "prompt_tokens": usage.total_prompt_tokens,
                        "completion_tokens": usage.total_completion_tokens,
                        "cache_creation_input_tokens": usage.total_cache_creation_input_tokens,
                        "cache_read_input_tokens": usage.total_cache_read_input_tokens,
                        "total_requests": usage.total_requests
                    })
                } else {
                    json!({"error": "No session store"})
                }
            }

            "list_tasks" => {
                let task_dir = crate::core::runtime_paths::default_data_dir().join("tasks");
                match crate::core::task_scheduler::TaskScheduler::list_tasks_from_dir(&task_dir) {
                    Ok(tasks) => json!({
                        "status": "success",
                        "count": tasks.len(),
                        "tasks": tasks.into_iter().map(|task| json!({
                            "id": task.id,
                            "name": task.name,
                            "session_id": task.session_id,
                            "interval_secs": task.interval_secs,
                            "schedule": task.schedule_expr,
                            "one_shot": task.one_shot,
                            "enabled": task.enabled,
                            "project_dir": task.project_dir,
                            "coding_backend": task.coding_backend,
                            "coding_model": task.coding_model,
                            "execution_mode": task.execution_mode,
                            "auto_approve": task.auto_approve,
                            "prompt": task.prompt,
                        })).collect::<Vec<_>>(),
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "get_devel_status" => {
                let task_dir = crate::core::runtime_paths::default_data_dir().join("tasks");
                let repo_root = std::env::current_dir()
                    .ok()
                    .and_then(|cwd| crate::core::devel_mode::detect_repo_root(&cwd))
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
                crate::core::devel_mode::devel_status_json(&task_dir, &repo_root)
            }

            "get_devel_result" => {
                let task_dir = crate::core::runtime_paths::default_data_dir().join("tasks");
                let repo_root = std::env::current_dir()
                    .ok()
                    .and_then(|cwd| crate::core::devel_mode::detect_repo_root(&cwd))
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                crate::core::devel_mode::devel_result_json(&task_dir, &repo_root)
            }

            "clear_agent_data" => {
                let include_memory = params
                    .get("include_memory")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                let include_sessions = params
                    .get("include_sessions")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                match agent.clear_agent_data(include_memory, include_sessions) {
                    Ok(result) => result,
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "bridge_tool" => {
                let tool_name = params["tool_name"].as_str().unwrap_or("").trim();
                if tool_name.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'tool_name'"},"id":req_id})
                        .to_string();
                }
                let args = params.get("args").cloned().unwrap_or_else(|| json!({}));
                let allowed_tools = params
                    .get("allowed_tools")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(|item| item.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let fut = agent.execute_bridge_tool(tool_name, &args, &allowed_tools);
                tokio::task::block_in_place(|| rt_handle.block_on(fut))
            }

            "bridge_list_tools" => {
                let allowed_tools = params
                    .get("allowed_tools")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(|item| item.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let fut = agent.get_bridge_tool_declarations(&allowed_tools);
                let tools = tokio::task::block_in_place(|| rt_handle.block_on(fut));
                json!({
                    "tools": tools.into_iter().map(|tool| json!({
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    })).collect::<Vec<_>>()
                })
            }

            "get_llm_config" => {
                let path = params["path"].as_str();
                match agent.get_llm_config(path) {
                    Ok(value) => json!({
                        "status": "ok",
                        "path": path,
                        "value": value
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "set_llm_config" => {
                let path = params["path"].as_str().unwrap_or("");
                if path.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'path'"},"id":req_id})
                        .to_string();
                }
                if params.get("value").is_none() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'value'"},"id":req_id})
                        .to_string();
                }

                let value = params["value"].clone();
                let fut = agent.set_llm_config(path, value);
                match tokio::task::block_in_place(|| rt_handle.block_on(fut)) {
                    Ok(saved) => json!({
                        "status": "ok",
                        "path": path,
                        "value": saved
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "unset_llm_config" => {
                let path = params["path"].as_str().unwrap_or("");
                if path.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'path'"},"id":req_id})
                        .to_string();
                }

                let fut = agent.unset_llm_config(path);
                match tokio::task::block_in_place(|| rt_handle.block_on(fut)) {
                    Ok(removed) => json!({
                        "status": "ok",
                        "path": path,
                        "removed": removed
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "reload_llm_backends" => {
                let fut = agent.reload_llm_backends();
                match tokio::task::block_in_place(|| rt_handle.block_on(fut)) {
                    Ok(config) => json!({
                        "status": "ok",
                        "config": config
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "get_llm_runtime" => {
                let runtime = agent.get_llm_runtime();
                json!({
                    "status": "ok",
                    "configured_active_backend": runtime["configured_active_backend"].clone(),
                    "configured_fallback_backends": runtime["configured_fallback_backends"].clone(),
                    "runtime_primary_backend": runtime["runtime_primary_backend"].clone(),
                    "runtime_has_primary_backend": runtime["runtime_has_primary_backend"].clone()
                })
            }

            "start_channel" => {
                let name = params["name"].as_str().unwrap_or("");
                if name.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'name'"},"id":req_id})
                        .to_string();
                }
                let settings = params.get("settings");
                match registry.lock() {
                    Ok(mut reg) => match reg.start_channel(name, settings) {
                        Ok(()) => json!({"status": "ok", "name": name}),
                        Err(e) => {
                            return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":e},"id":req_id})
                                .to_string();
                        }
                    },
                    Err(_) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":"Registry lock failed"},"id":req_id})
                            .to_string();
                    }
                }
            }

            "stop_channel" => {
                let name = params["name"].as_str().unwrap_or("");
                if name.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'name'"},"id":req_id})
                        .to_string();
                }
                match registry.lock() {
                    Ok(mut reg) => match reg.stop_channel(name) {
                        Ok(()) => json!({"status": "ok", "name": name}),
                        Err(e) => {
                            return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":e},"id":req_id})
                                .to_string();
                        }
                    },
                    Err(_) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":"Registry lock failed"},"id":req_id})
                            .to_string();
                    }
                }
            }

            "channel_status" => {
                let name = params["name"].as_str().unwrap_or("");
                if name.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'name'"},"id":req_id})
                        .to_string();
                }
                match registry.lock() {
                    Ok(reg) => match reg.channel_status(name) {
                        Some(running) => json!({"name": name, "running": running}),
                        None => {
                            return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":"Channel not registered"},"id":req_id})
                                .to_string();
                        }
                    },
                    Err(_) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":"Registry lock failed"},"id":req_id})
                            .to_string();
                    }
                }
            }

            "register_path" => {
                let kind = match params["kind"].as_str().unwrap_or("") {
                    "tool" => RegistrationKind::Tool,
                    "skill" => RegistrationKind::Skill,
                    _ => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid 'kind'. Expected 'tool' or 'skill'"},"id":req_id})
                            .to_string();
                    }
                };
                let path = params["path"].as_str().unwrap_or("");
                if path.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'path'"},"id":req_id})
                        .to_string();
                }
                let fut = agent.register_external_path(kind, path);
                match tokio::task::block_in_place(|| rt_handle.block_on(fut)) {
                    Ok(registrations) => json!({
                        "status": "ok",
                        "kind": kind.as_str(),
                        "registrations": registrations,
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "unregister_path" => {
                let kind = match params["kind"].as_str().unwrap_or("") {
                    "tool" => RegistrationKind::Tool,
                    "skill" => RegistrationKind::Skill,
                    _ => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Invalid 'kind'. Expected 'tool' or 'skill'"},"id":req_id})
                            .to_string();
                    }
                };
                let path = params["path"].as_str().unwrap_or("");
                if path.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'path'"},"id":req_id})
                        .to_string();
                }
                let fut = agent.unregister_external_path(kind, path);
                match tokio::task::block_in_place(|| rt_handle.block_on(fut)) {
                    Ok((registrations, removed)) => json!({
                        "status": "ok",
                        "kind": kind.as_str(),
                        "removed": removed,
                        "registrations": registrations,
                    }),
                    Err(err) => {
                        return json!({"jsonrpc":"2.0","error":{"code":-32000,"message":err},"id":req_id})
                            .to_string();
                    }
                }
            }

            "list_registered_paths" => {
                let registrations = agent.list_registered_paths();
                json!({
                    "status": "ok",
                    "registrations": registrations,
                    "runtime_topology": agent.runtime_topology_summary(),
                })
            }

            "get_session_runtime" => {
                let session_id = params["session_id"].as_str().unwrap_or("").trim();
                if session_id.is_empty() {
                    return json!({"jsonrpc":"2.0","error":{"code":-32602,"message":"Missing 'session_id'"},"id":req_id})
                        .to_string();
                }
                agent.session_runtime_status(session_id)
            }

            "get_tool_audit" => agent.tool_audit_status(),

            "get_skill_capabilities" => agent.skill_capability_status(),

            _ => {
                return json!({"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":req_id})
                    .to_string();
            }
        };

        json!({"jsonrpc":"2.0","id":req_id,"result":result}).to_string()
    }

    fn send_response(fd: i32, response: &str) {
        let mut msg_buf = Vec::with_capacity(4 + response.len());
        msg_buf.extend_from_slice(&(response.len() as u32).to_be_bytes());
        msg_buf.extend_from_slice(response.as_bytes());

        unsafe {
            let mut sent: usize = 0;
            while sent < msg_buf.len() {
                let n = libc::write(
                    fd,
                    msg_buf.as_ptr().add(sent) as *const _,
                    msg_buf.len() - sent,
                );
                if n <= 0 {
                    break;
                }
                sent += n as usize;
            }
        }
    }
}
