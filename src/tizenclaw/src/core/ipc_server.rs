//! IPC server — Unix domain socket with JSON-RPC 2.0 protocol.

use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use crate::channel::ChannelRegistry;
use crate::core::agent_core::AgentCore;
use crate::core::llm_config_store;
use crate::core::registration_store::RegistrationKind;

const MAX_CONCURRENT_CLIENTS: usize = 8;
const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const DEFAULT_ABSTRACT_SOCKET_NAME: &str = "tizenclaw.sock";

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);
static DAEMON_STARTED_AT: LazyLock<Instant> = LazyLock::new(Instant::now);

#[derive(Clone, Debug)]
enum SocketEndpoint {
    Filesystem(PathBuf),
    Abstract(String),
}

impl SocketEndpoint {
    fn resolve(requested_path: &str) -> Self {
        if let Ok(path) = std::env::var("TIZENCLAW_SOCKET_PATH") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return Self::Filesystem(PathBuf::from(trimmed));
            }
        }

        if let Ok(name) = std::env::var("TIZENCLAW_SOCKET_NAME") {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Self::Abstract(trimmed.to_string());
            }
        }

        let trimmed = requested_path.trim();
        if !trimmed.is_empty() {
            return Self::Filesystem(PathBuf::from(trimmed));
        }

        Self::Abstract(DEFAULT_ABSTRACT_SOCKET_NAME.to_string())
    }

    fn describe(&self) -> String {
        match self {
            Self::Filesystem(path) => path.display().to_string(),
            Self::Abstract(name) => format!("\\0{}", name),
        }
    }

    fn cleanup(&self) {
        if let Self::Filesystem(path) = self {
            let _ = fs::remove_file(path);
        }
    }
}

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
        Self {
            running: Arc::new(AtomicBool::new(false)),
            active_clients: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn start(
        &self,
        socket_path: &str,
        agent: Arc<AgentCore>,
        channel_registry: Arc<Mutex<ChannelRegistry>>,
    ) -> std::thread::JoinHandle<()> {
        let running = self.running.clone();
        let active_clients = self.active_clients.clone();
        let endpoint = SocketEndpoint::resolve(socket_path);
        let rt_handle = tokio::runtime::Handle::current();
        running.store(true, Ordering::SeqCst);

        std::thread::spawn(move || {
            Self::server_loop(
                rt_handle,
                running,
                active_clients,
                endpoint,
                agent,
                channel_registry,
            );
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn configure_client_fd(fd: i32) {
        unsafe {
            let timeout = libc::timeval {
                tv_sec: 5,
                tv_usec: 0,
            };
            let timeout_ptr = &timeout as *const _ as *const libc::c_void;
            let timeout_len = std::mem::size_of::<libc::timeval>() as libc::socklen_t;

            let _ = libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                timeout_ptr,
                timeout_len,
            );
            let _ = libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDTIMEO,
                timeout_ptr,
                timeout_len,
            );
        }
    }

    fn bind_listener(endpoint: &SocketEndpoint) -> Result<i32, String> {
        endpoint.cleanup();

        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(format!(
                "Failed to create IPC socket: {}",
                std::io::Error::last_os_error()
            ));
        }

        let bind_result = unsafe {
            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;

            let addr_len = match endpoint {
                SocketEndpoint::Filesystem(path) => {
                    let bytes = path.as_os_str().as_bytes();
                    if bytes.len() >= addr.sun_path.len() {
                        libc::close(fd);
                        return Err(format!("IPC socket path too long: {}", path.display()));
                    }

                    for (index, byte) in bytes.iter().enumerate() {
                        addr.sun_path[index] = *byte as libc::c_char;
                    }

                    (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t
                }
                SocketEndpoint::Abstract(name) => {
                    let bytes = name.as_bytes();
                    if bytes.len() + 1 >= addr.sun_path.len() {
                        libc::close(fd);
                        return Err(format!("IPC socket name too long: {}", name));
                    }

                    for (index, byte) in bytes.iter().enumerate() {
                        addr.sun_path[index + 1] = *byte as libc::c_char;
                    }

                    (std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1) as libc::socklen_t
                }
            };

            if libc::bind(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
                let error = std::io::Error::last_os_error();
                libc::close(fd);
                return Err(format!(
                    "Failed to bind IPC socket '{}': {}",
                    endpoint.describe(),
                    error
                ));
            }

            let timeout = libc::timeval {
                tv_sec: 1,
                tv_usec: 0,
            };
            let timeout_ptr = &timeout as *const _ as *const libc::c_void;
            let timeout_len = std::mem::size_of::<libc::timeval>() as libc::socklen_t;
            let _ = libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                timeout_ptr,
                timeout_len,
            );

            libc::listen(fd, 64)
        };

        if bind_result < 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            endpoint.cleanup();
            return Err(format!(
                "Failed to listen on IPC socket '{}': {}",
                endpoint.describe(),
                error
            ));
        }

        Ok(fd)
    }

    fn server_loop(
        rt_handle: tokio::runtime::Handle,
        running: Arc<AtomicBool>,
        active_clients: Arc<AtomicUsize>,
        endpoint: SocketEndpoint,
        agent: Arc<AgentCore>,
        channel_registry: Arc<Mutex<ChannelRegistry>>,
    ) {
        let listener_fd = match Self::bind_listener(&endpoint) {
            Ok(fd) => fd,
            Err(err) => {
                log::error!("{}", err);
                return;
            }
        };

        log::info!("IPC server listening on {}", endpoint.describe());

        while running.load(Ordering::SeqCst) {
            let client_fd =
                unsafe { libc::accept(listener_fd, std::ptr::null_mut(), std::ptr::null_mut()) };
            if client_fd < 0 {
                let errno = std::io::Error::last_os_error()
                    .raw_os_error()
                    .unwrap_or_default();
                if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK && errno != libc::EINTR {
                    log::error!("IPC accept failed: errno={}", errno);
                }
                continue;
            }

            Self::configure_client_fd(client_fd);

            if active_clients
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                    (current < MAX_CONCURRENT_CLIENTS).then_some(current + 1)
                })
                .is_err()
            {
                let mut busy_stream = unsafe { UnixStream::from_raw_fd(client_fd) };
                let busy = Self::jsonrpc_error(Value::Null, -32000, "Server busy".to_string());
                let _ = Self::write_payload(&mut busy_stream, busy.as_bytes());
                continue;
            }

            let agent = agent.clone();
            let channel_registry = channel_registry.clone();
            let active_clients = active_clients.clone();
            let rt_handle = rt_handle.clone();

            std::thread::spawn(move || {
                let stream = unsafe { UnixStream::from_raw_fd(client_fd) };
                Self::handle_client(rt_handle, stream, agent, channel_registry);
                active_clients.fetch_sub(1, Ordering::SeqCst);
            });
        }

        unsafe {
            libc::close(listener_fd);
        }
        endpoint.cleanup();
        log::info!("IPC server stopped");
    }

    fn handle_client(
        rt_handle: tokio::runtime::Handle,
        mut stream: UnixStream,
        agent: Arc<AgentCore>,
        channel_registry: Arc<Mutex<ChannelRegistry>>,
    ) {
        let connection_id = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let writer = match stream.try_clone() {
            Ok(writer) => Arc::new(Mutex::new(writer)),
            Err(err) => {
                log::error!(
                    "IPC connection {} failed to clone writer: {}",
                    connection_id,
                    err
                );
                return;
            }
        };

        loop {
            let payload = match Self::read_payload(&mut stream) {
                Ok(payload) => payload,
                Err(err) => {
                    let eof = err.contains("UnexpectedEof")
                        || err.contains("Connection reset by peer")
                        || err.contains("connection closed");
                    if !eof {
                        log::warn!("IPC connection {} read failed: {}", connection_id, err);
                    }
                    break;
                }
            };

            let response =
                Self::dispatch_request(&rt_handle, &payload, &writer, &agent, &channel_registry);

            let write_result = writer
                .lock()
                .map_err(|_| "IPC writer lock poisoned".to_string())
                .and_then(|mut guard| Self::write_payload(&mut guard, response.as_bytes()));
            if let Err(err) = write_result {
                log::warn!("IPC connection {} write failed: {}", connection_id, err);
                break;
            }
        }

        log::debug!("IPC connection {} closed", connection_id);
    }

    fn read_payload(stream: &mut UnixStream) -> Result<Vec<u8>, String> {
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .map_err(|err| format!("Failed to read payload length: {}", err))?;

        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len > MAX_PAYLOAD_SIZE {
            return Err(format!(
                "Payload too large: {} bytes exceeds {} bytes",
                payload_len, MAX_PAYLOAD_SIZE
            ));
        }

        let mut payload = vec![0u8; payload_len];
        stream
            .read_exact(&mut payload)
            .map_err(|err| format!("Failed to read payload body: {}", err))?;
        Ok(payload)
    }

    fn write_payload(stream: &mut UnixStream, data: &[u8]) -> Result<(), String> {
        if data.len() > MAX_PAYLOAD_SIZE {
            return Err(format!(
                "Payload too large to send: {} bytes exceeds {} bytes",
                data.len(),
                MAX_PAYLOAD_SIZE
            ));
        }

        let len = (data.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .and_then(|_| stream.write_all(data))
            .and_then(|_| stream.flush())
            .map_err(|err| format!("Failed to write IPC payload: {}", err))
    }

    fn dispatch_request(
        rt_handle: &tokio::runtime::Handle,
        raw_payload: &[u8],
        writer: &Arc<Mutex<UnixStream>>,
        agent: &Arc<AgentCore>,
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
    ) -> String {
        let request: Value = match serde_json::from_slice(raw_payload) {
            Ok(value) => value,
            Err(err) => {
                return Self::jsonrpc_error(Value::Null, -32700, format!("Parse error: {}", err));
            }
        };

        if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Self::jsonrpc_error(Value::Null, -32600, "Invalid Request".to_string());
        }

        let method = match request.get("method").and_then(Value::as_str) {
            Some(method) if !method.trim().is_empty() => method,
            _ => {
                let req_id = request.get("id").cloned().unwrap_or(Value::Null);
                return Self::jsonrpc_error(req_id, -32600, "Invalid Request".to_string());
            }
        };

        let req_id = request.get("id").cloned().unwrap_or(Value::Null);
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        let result = match method {
            "process_prompt" | "process_prompt_stream" | "prompt" => {
                match Self::handle_process_prompt(
                    rt_handle, method, &params, &req_id, writer, agent,
                ) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "session.clear" => match Self::handle_session_clear(agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "session.list" => Self::handle_session_list(agent),
            "session.status" => match Self::required_str(&params, "session_id", &req_id) {
                Ok(session_id) => agent.session_runtime_status(session_id),
                Err(response) => return response,
            },
            "tool.list" | "bridge_list_tools" => Self::handle_tool_list(rt_handle, agent),
            "tool.reload" => match Self::handle_tool_reload(rt_handle, agent) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "backend.list" => match Self::handle_backend_list(agent) {
                Ok(result) => result,
                Err(response) => return Self::jsonrpc_error(req_id, -32000, response),
            },
            "backend.reload" | "reload_llm_backends" => {
                match Self::handle_backend_reload(rt_handle, agent, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "key.list" => match Self::handle_key_list(agent, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "key.set" => match Self::handle_key_set(rt_handle, agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "key.delete" => match Self::handle_key_delete(rt_handle, agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "key.test" => match Self::handle_key_test(rt_handle, agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "backend.config.get" | "get_llm_config" => {
                match Self::handle_backend_config_get(agent, &params, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "backend.config.set" | "set_llm_config" => {
                match Self::handle_backend_config_set(rt_handle, agent, &params, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "unset_llm_config" => {
                match Self::handle_backend_config_unset(rt_handle, agent, &params, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "dashboard.start" => {
                match Self::handle_dashboard_start(channel_registry, &params, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "dashboard.stop" => match Self::handle_dashboard_stop(channel_registry, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "dashboard.status" => match Self::handle_dashboard_status(channel_registry, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "register_path" => match Self::handle_register_path(rt_handle, agent, &params, &req_id)
            {
                Ok(result) => result,
                Err(response) => return response,
            },
            "unregister_path" => {
                match Self::handle_unregister_path(rt_handle, agent, &params, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
                }
            }
            "runtime_status" => Self::handle_runtime_status(agent, channel_registry),
            "ping" => json!({"pong": true}),
            "get_usage" => match Self::handle_get_usage(agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "list_tasks" => match Self::handle_list_tasks(&req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "get_devel_status" => Self::handle_devel_status(),
            "get_devel_result" => Self::handle_devel_result(),
            "clear_agent_data" => match Self::handle_clear_agent_data(agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "bridge_tool" => match Self::handle_bridge_tool(rt_handle, agent, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
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
            "start_channel" => match Self::handle_start_channel(channel_registry, &params, &req_id)
            {
                Ok(result) => result,
                Err(response) => return response,
            },
            "stop_channel" => match Self::handle_stop_channel(channel_registry, &params, &req_id) {
                Ok(result) => result,
                Err(response) => return response,
            },
            "channel_status" => {
                match Self::handle_channel_status(channel_registry, &params, &req_id) {
                    Ok(result) => result,
                    Err(response) => return response,
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
            "get_session_runtime" => match Self::required_str(&params, "session_id", &req_id) {
                Ok(session_id) => agent.session_runtime_status(session_id),
                Err(response) => return response,
            },
            "get_tool_audit" => agent.tool_audit_status(),
            "get_skill_capabilities" => agent.skill_capability_status(),
            _ => {
                return Self::jsonrpc_error(
                    req_id,
                    -32601,
                    format!("Method not found: {}", method),
                );
            }
        };

        Self::jsonrpc_result(req_id, result)
    }

    fn handle_process_prompt(
        rt_handle: &tokio::runtime::Handle,
        method: &str,
        params: &Value,
        req_id: &Value,
        writer: &Arc<Mutex<UnixStream>>,
        agent: &Arc<AgentCore>,
    ) -> Result<Value, String> {
        let prompt = match method {
            "prompt" => params.get("text").and_then(Value::as_str).unwrap_or(""),
            _ => params.get("prompt").and_then(Value::as_str).unwrap_or(""),
        }
        .trim();

        if prompt.is_empty() {
            return Err(Self::jsonrpc_error(
                req_id.clone(),
                -32602,
                "Missing 'prompt'".to_string(),
            ));
        }

        let session_id =
            Self::resolve_session_id(params.get("session_id").and_then(Value::as_str), "ipc");
        let stream = params
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || method == "process_prompt_stream";

        let response = if stream {
            let stream_writer = writer.clone();
            let stream_request_id = req_id.clone();
            let on_chunk = move |chunk: &str| {
                let frame = json!({
                    "jsonrpc": "2.0",
                    "method": "stream_chunk",
                    "params": {
                        "id": stream_request_id,
                        "chunk": chunk,
                    }
                })
                .to_string();

                if let Ok(mut guard) = stream_writer.lock() {
                    let _ = Self::write_payload(&mut guard, frame.as_bytes());
                }
            };

            tokio::task::block_in_place(|| {
                rt_handle.block_on(agent.process_prompt(&session_id, prompt, Some(&on_chunk)))
            })
        } else {
            tokio::task::block_in_place(|| {
                rt_handle.block_on(agent.process_prompt(&session_id, prompt, None))
            })
        };

        Ok(json!({
            "text": response,
            "session_id": session_id,
        }))
    }

    fn handle_session_clear(
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let session_id = Self::required_str(params, "session_id", req_id)?;
        let Some(session_store) = agent.get_session_store() else {
            return Err(Self::jsonrpc_error(
                req_id.clone(),
                -32000,
                "Session store unavailable".to_string(),
            ));
        };

        session_store.store().clear_session(session_id);
        Ok(json!({
            "status": "ok",
            "session_id": session_id,
            "cleared": true,
        }))
    }

    fn handle_session_list(agent: &Arc<AgentCore>) -> Value {
        let sessions_dir = agent
            .runtime_topology_summary()
            .get("sessions_dir")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::core::runtime_paths::default_data_dir().join("sessions"));

        let mut session_ids = fs::read_dir(sessions_dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string())
            })
            .collect::<Vec<_>>();
        session_ids.sort();

        json!({
            "status": "ok",
            "sessions": session_ids,
        })
    }

    fn handle_tool_list(rt_handle: &tokio::runtime::Handle, agent: &Arc<AgentCore>) -> Value {
        let tools = tokio::task::block_in_place(|| {
            rt_handle.block_on(agent.get_bridge_tool_declarations(&[]))
        });

        json!({
            "status": "ok",
            "count": tools.len(),
            "tools": tools.into_iter().map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
            })).collect::<Vec<_>>(),
        })
    }

    fn handle_tool_reload(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
    ) -> Result<Value, String> {
        tokio::task::block_in_place(|| rt_handle.block_on(agent.reload_tools()));
        Ok(Self::handle_tool_list(rt_handle, agent))
    }

    fn handle_backend_list(agent: &Arc<AgentCore>) -> Result<Value, String> {
        let config = agent.get_llm_config(None)?;
        let runtime = agent.get_llm_runtime();
        let active_backend = runtime
            .get("configured_active_backend")
            .and_then(Value::as_str)
            .unwrap_or("");
        let fallback_backends = runtime
            .get("configured_fallback_backends")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let backends = config
            .get("backends")
            .and_then(Value::as_object)
            .map(|items| {
                items
                    .iter()
                    .map(|(name, value)| {
                        let is_fallback = fallback_backends.iter().any(|entry| entry == name);
                        json!({
                            "name": name,
                            "configured": value,
                            "is_active": name == active_backend,
                            "is_fallback": is_fallback,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(json!({
            "status": "ok",
            "backends": backends,
            "runtime": runtime,
        }))
    }

    fn handle_backend_reload(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        req_id: &Value,
    ) -> Result<Value, String> {
        let config =
            tokio::task::block_in_place(|| rt_handle.block_on(agent.reload_llm_backends()))
                .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "config": llm_config_store::redact_secrets(&config),
            "runtime": agent.get_llm_runtime(),
        }))
    }

    fn handle_key_list(agent: &Arc<AgentCore>, req_id: &Value) -> Result<Value, String> {
        agent
            .list_keys()
            .map(|result| {
                json!({
                    "status": "ok",
                    "stored": result["stored"].clone(),
                    "from_env": result["from_env"].clone(),
                })
            })
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))
    }

    fn handle_key_set(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let key = Self::required_str(params, "key", req_id)?;
        let value = Self::required_str(params, "value", req_id)?;
        if value.trim().is_empty() {
            return Err(Self::jsonrpc_error(
                req_id.clone(),
                -32602,
                "Key value must not be empty".to_string(),
            ));
        }

        let result = tokio::task::block_in_place(|| rt_handle.block_on(agent.set_key(key, value)))
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "key": key,
            "stored": result["stored"].clone(),
        }))
    }

    fn handle_key_delete(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let key = Self::required_str(params, "key", req_id)?;
        let result = tokio::task::block_in_place(|| rt_handle.block_on(agent.delete_key(key)))
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "key": key,
            "deleted": result["deleted"].clone(),
        }))
    }

    fn handle_key_test(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let key = Self::required_str(params, "key", req_id)?;
        let result = tokio::task::block_in_place(|| rt_handle.block_on(agent.test_key(key)))
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "key": key,
            "reachable": result["reachable"].clone(),
            "status_code": result["status_code"].clone(),
        }))
    }

    fn handle_backend_config_get(
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let path = params.get("path").and_then(Value::as_str);
        agent
            .get_llm_config(path)
            .map(|value| {
                json!({
                    "status": "ok",
                    "path": path,
                    "value": llm_config_store::redact_secrets(&value),
                })
            })
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))
    }

    fn handle_backend_config_set(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let path = Self::required_str(params, "path", req_id)?;
        let Some(value) = params.get("value").cloned() else {
            return Err(Self::jsonrpc_error(
                req_id.clone(),
                -32602,
                "Missing 'value'".to_string(),
            ));
        };

        let saved =
            tokio::task::block_in_place(|| rt_handle.block_on(agent.set_llm_config(path, value)))
                .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "path": path,
            "value": llm_config_store::redact_secrets(&saved),
        }))
    }

    fn handle_backend_config_unset(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let path = Self::required_str(params, "path", req_id)?;
        let removed =
            tokio::task::block_in_place(|| rt_handle.block_on(agent.unset_llm_config(path)))
                .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "path": path,
            "removed": llm_config_store::redact_secrets(&removed),
        }))
    }

    fn handle_dashboard_start(
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let mut dashboard_params = json!({
            "name": "web_dashboard",
        });
        if let Some(port) = params.get("port").cloned() {
            dashboard_params["settings"] = json!({ "port": port });
        }
        Self::handle_start_channel(channel_registry, &dashboard_params, req_id)
    }

    fn handle_dashboard_stop(
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
        req_id: &Value,
    ) -> Result<Value, String> {
        Self::handle_stop_channel(channel_registry, &json!({"name": "web_dashboard"}), req_id)
    }

    fn handle_dashboard_status(
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
        req_id: &Value,
    ) -> Result<Value, String> {
        Self::handle_channel_status(channel_registry, &json!({"name": "web_dashboard"}), req_id)
    }

    fn handle_register_path(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let kind = Self::registration_kind(params, req_id)?;
        let path = Self::required_str(params, "path", req_id)?;
        let registrations = tokio::task::block_in_place(|| {
            rt_handle.block_on(agent.register_external_path(kind, path))
        })
        .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "kind": kind.as_str(),
            "registrations": registrations,
        }))
    }

    fn handle_unregister_path(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let kind = Self::registration_kind(params, req_id)?;
        let path = Self::required_str(params, "path", req_id)?;
        let (registrations, removed) = tokio::task::block_in_place(|| {
            rt_handle.block_on(agent.unregister_external_path(kind, path))
        })
        .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;
        Ok(json!({
            "status": "ok",
            "kind": kind.as_str(),
            "removed": removed,
            "registrations": registrations,
        }))
    }

    fn handle_runtime_status(
        agent: &Arc<AgentCore>,
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
    ) -> Value {
        let registrations = agent.list_registered_paths();
        let dashboard = match channel_registry.lock() {
            Ok(registry) => registry.channel_status("web_dashboard").map(|running| {
                json!({
                    "name": "web_dashboard",
                    "running": running,
                })
            }),
            Err(_) => None,
        }
        .unwrap_or_else(|| {
            json!({
                "name": "web_dashboard",
                "running": false,
            })
        });

        json!({
            "status": "ok",
            "uptime_secs": DAEMON_STARTED_AT.elapsed().as_secs(),
            "runtime_topology": agent.runtime_topology_summary(),
            "registrations": registrations,
            "llm_runtime": agent.get_llm_runtime(),
            "safety": agent.safety_guard_status(),
            "tool_policy": agent.tool_policy_status(),
            "tool_audit": agent.tool_audit_status(),
            "skills": agent.skill_capability_status(),
            "dashboard": dashboard,
        })
    }

    fn handle_get_usage(
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let Some(session_store) = agent.get_session_store() else {
            return Err(Self::jsonrpc_error(
                req_id.clone(),
                -32000,
                "No session store".to_string(),
            ));
        };

        let session_id = params
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let date = params
            .get("date")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let usage = if session_id.is_empty() {
            session_store.store().load_daily_usage(date)
        } else {
            session_store.store().load_token_usage(session_id)
        };
        let baseline = crate::storage::session_store::TokenUsage::from_json(params.get("baseline"));
        let delta = usage.diff_from(&baseline);
        Ok(json!({
            "scope": if session_id.is_empty() { "daily" } else { "session" },
            "session_id": if session_id.is_empty() { Value::Null } else { Value::String(session_id.to_string()) },
            "date": if session_id.is_empty() {
                Value::String(if date.is_empty() { "today".to_string() } else { date.to_string() })
            } else {
                Value::Null
            },
            "usage": usage.to_json(),
            "delta": delta.to_json(),
            "prompt_tokens": usage.total_prompt_tokens,
            "completion_tokens": usage.total_completion_tokens,
            "cache_creation_input_tokens": usage.total_cache_creation_input_tokens,
            "cache_read_input_tokens": usage.total_cache_read_input_tokens,
            "total_requests": usage.total_requests
        }))
    }

    fn handle_list_tasks(req_id: &Value) -> Result<Value, String> {
        let task_dir = crate::core::runtime_paths::default_data_dir().join("tasks");
        crate::core::task_scheduler::TaskScheduler::list_tasks_from_dir(&task_dir)
            .map(|tasks| {
                json!({
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
                })
            })
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))
    }

    fn handle_devel_status() -> Value {
        let task_dir = crate::core::runtime_paths::default_data_dir().join("tasks");
        let repo_root = std::env::current_dir()
            .ok()
            .and_then(|cwd| crate::core::devel_mode::detect_repo_root(&cwd))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        crate::core::devel_mode::devel_status_json(&task_dir, &repo_root)
    }

    fn handle_devel_result() -> Value {
        let task_dir = crate::core::runtime_paths::default_data_dir().join("tasks");
        let repo_root = std::env::current_dir()
            .ok()
            .and_then(|cwd| crate::core::devel_mode::detect_repo_root(&cwd))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        crate::core::devel_mode::devel_result_json(&task_dir, &repo_root)
    }

    fn handle_clear_agent_data(
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let include_memory = params
            .get("include_memory")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let include_sessions = params
            .get("include_sessions")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        agent
            .clear_agent_data(include_memory, include_sessions)
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))
    }

    fn handle_bridge_tool(
        rt_handle: &tokio::runtime::Handle,
        agent: &Arc<AgentCore>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let tool_name = Self::required_str(params, "tool_name", req_id)?;
        let args = params.get("args").cloned().unwrap_or_else(|| json!({}));
        let allowed_tools = Self::string_array(params.get("allowed_tools"));
        Ok(tokio::task::block_in_place(|| {
            rt_handle.block_on(agent.execute_bridge_tool(tool_name, &args, &allowed_tools))
        }))
    }

    fn handle_start_channel(
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let name = Self::required_str(params, "name", req_id)?;
        let settings = params.get("settings");
        let mut registry = channel_registry.lock().map_err(|_| {
            Self::jsonrpc_error(req_id.clone(), -32000, "Registry lock failed".to_string())
        })?;

        registry
            .start_channel(name, settings)
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;

        let mut status = registry
            .channel_snapshot(name)
            .unwrap_or_else(|| json!({"name": name, "running": true}));
        if let Some(object) = status.as_object_mut() {
            object.insert("status".to_string(), json!("ok"));
        }
        Ok(status)
    }

    fn handle_stop_channel(
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let name = Self::required_str(params, "name", req_id)?;
        let mut registry = channel_registry.lock().map_err(|_| {
            Self::jsonrpc_error(req_id.clone(), -32000, "Registry lock failed".to_string())
        })?;

        registry
            .stop_channel(name)
            .map_err(|err| Self::jsonrpc_error(req_id.clone(), -32000, err))?;

        let mut status = registry
            .channel_snapshot(name)
            .unwrap_or_else(|| json!({"name": name, "running": false}));
        if let Some(object) = status.as_object_mut() {
            object.insert("status".to_string(), json!("ok"));
        }
        Ok(status)
    }

    fn handle_channel_status(
        channel_registry: &Arc<Mutex<ChannelRegistry>>,
        params: &Value,
        req_id: &Value,
    ) -> Result<Value, String> {
        let name = Self::required_str(params, "name", req_id)?;
        let registry = channel_registry.lock().map_err(|_| {
            Self::jsonrpc_error(req_id.clone(), -32000, "Registry lock failed".to_string())
        })?;

        registry.channel_snapshot(name).ok_or_else(|| {
            Self::jsonrpc_error(req_id.clone(), -32000, "Channel not registered".to_string())
        })
    }

    fn required_str<'a>(params: &'a Value, key: &str, req_id: &Value) -> Result<&'a str, String> {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                Self::jsonrpc_error(req_id.clone(), -32602, format!("Missing '{}'", key))
            })
    }

    fn registration_kind(params: &Value, req_id: &Value) -> Result<RegistrationKind, String> {
        match params.get("kind").and_then(Value::as_str).unwrap_or("") {
            "tool" => Ok(RegistrationKind::Tool),
            "skill" => Ok(RegistrationKind::Skill),
            _ => Err(Self::jsonrpc_error(
                req_id.clone(),
                -32602,
                "Invalid 'kind'. Expected 'tool' or 'skill'".to_string(),
            )),
        }
    }

    fn string_array(value: Option<&Value>) -> Vec<String> {
        value
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|value| value.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
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

    fn jsonrpc_result(id: Value, result: Value) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        })
        .to_string()
    }

    fn jsonrpc_error(id: Value, code: i64, message: String) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": code,
                "message": message,
            }
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::IpcServer;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    #[test]
    fn read_payload_roundtrip_uses_big_endian_length_prefix() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        IpcServer::write_payload(&mut client, br#"{"jsonrpc":"2.0"}"#).unwrap();
        let payload = IpcServer::read_payload(&mut server).unwrap();
        assert_eq!(payload, br#"{"jsonrpc":"2.0"}"#);
    }

    #[test]
    fn read_payload_rejects_oversized_frames() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let oversized = ((super::MAX_PAYLOAD_SIZE + 1) as u32).to_be_bytes();
        client.write_all(&oversized).unwrap();
        client.write_all(&[0u8; 4]).unwrap();

        let err = IpcServer::read_payload(&mut server).unwrap_err();
        assert!(err.contains("Payload too large"));
    }

    #[test]
    fn write_payload_prefixes_body_length() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        IpcServer::write_payload(&mut client, b"pong").unwrap();

        let mut len_buf = [0u8; 4];
        server.read_exact(&mut len_buf).unwrap();
        assert_eq!(u32::from_be_bytes(len_buf), 4);

        let mut body = [0u8; 4];
        server.read_exact(&mut body).unwrap();
        assert_eq!(&body, b"pong");
    }
}
