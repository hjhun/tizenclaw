//! tizenclaw-cli: CLI tool for interacting with TizenClaw daemon.
//!
//! Usage:
//!   tizenclaw-cli "What is the battery level?"
//!   tizenclaw-cli -s my_session "Run a skill"
//!   tizenclaw-cli --stream "Tell me about Tizen"
//!   tizenclaw-cli dashboard start
//!   tizenclaw-cli dashboard start --port 9091
//!   tizenclaw-cli dashboard stop
//!   tizenclaw-cli dashboard status
//!   tizenclaw-cli   (interactive mode)

use serde_json::{json, Map, Value};
use std::ffi::{c_char, c_int};
use std::fs;
use std::io::{self, BufRead, ErrorKind, Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static CLI_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);
static RPC_REQUEST_COUNTER: AtomicUsize = AtomicUsize::new(1);

const DEFAULT_SOCKET_NAME: &str = "tizenclaw.sock";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_PROMPT_TIMEOUT_MS: u64 = 600_000;
const LONG_PROMPT_STREAM_THRESHOLD_CHARS: usize = 6_000;
const MAX_IPC_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
const CONNECTION_ERROR_MESSAGE: &str = "Cannot connect to tizenclaw daemon. Is it running?";
const STREAM_CHUNK_METHOD: &str = "stream_chunk";
const AF_UNIX: c_int = 1;
const SOCK_STREAM: c_int = 1;
type SaFamilyT = u16;
type SockLenT = u32;

#[repr(C)]
struct RawSockAddr {
    sa_family: SaFamilyT,
    sa_data: [c_char; 14],
}

#[repr(C)]
struct RawSockAddrUn {
    sun_family: SaFamilyT,
    sun_path: [c_char; 108],
}

unsafe extern "C" {
    fn socket(domain: c_int, type_: c_int, protocol: c_int) -> c_int;
    fn connect(sockfd: c_int, addr: *const RawSockAddr, addrlen: SockLenT) -> c_int;
    fn close(fd: c_int) -> c_int;
}

#[derive(Clone, Debug)]
struct CliOptions {
    session_id: Option<String>,
    stream: bool,
    socket_path: Option<String>,
    socket_name: Option<String>,
    json_output: bool,
    timeout_ms: u64,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            session_id: None,
            stream: false,
            socket_path: None,
            socket_name: None,
            json_output: false,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

#[derive(Clone, Debug)]
enum CommandMode {
    Prompt(String),
    Interactive,
    Dashboard { action: String, port: Option<u16> },
    Register { kind: String, path: String },
    Unregister { kind: String, path: String },
    ListRegistrations,
    ListTasks,
    DevelStatus,
    ToolsStatus,
    SkillsStatus,
    Config(Vec<String>),
    ClearData(Vec<String>),
    Auth(Vec<String>),
    Setup,
    Usage { baseline: Option<Value> },
}

#[derive(Clone, Debug)]
struct ParsedCli {
    options: CliOptions,
    mode: CommandMode,
}

#[derive(Clone, Debug)]
struct IpcClient {
    socket_path: Option<String>,
    socket_name: Option<String>,
    timeout: Duration,
}

#[derive(Debug)]
struct RpcResponse {
    payload: Value,
    streamed_chunks: Vec<String>,
}

#[derive(Debug)]
struct PromptCall {
    payload: Value,
    text: Option<String>,
    stream_received: bool,
}

impl IpcClient {
    fn from_options(options: &CliOptions) -> Self {
        Self {
            socket_path: options.socket_path.clone(),
            socket_name: options.socket_name.clone(),
            timeout: Duration::from_millis(options.timeout_ms),
        }
    }

    fn connect(&self) -> Result<UnixStream, String> {
        if let Some(path) = self
            .socket_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let stream = UnixStream::connect(Path::new(path))
                .map_err(|_| CONNECTION_ERROR_MESSAGE.to_string())?;
            self.configure_stream(&stream)?;
            return Ok(stream);
        }

        let socket_name = self
            .socket_name
            .clone()
            .or_else(|| std::env::var("TIZENCLAW_SOCKET_PATH").ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SOCKET_NAME.to_string());

        if socket_name.starts_with('/') {
            let stream = UnixStream::connect(Path::new(&socket_name))
                .map_err(|_| CONNECTION_ERROR_MESSAGE.to_string())?;
            self.configure_stream(&stream)?;
            return Ok(stream);
        }

        let fd = unsafe { socket(AF_UNIX, SOCK_STREAM, 0) };
        if fd < 0 {
            return Err("Failed to create IPC socket".into());
        }

        let connect_result = unsafe {
            let mut addr: RawSockAddrUn = std::mem::zeroed();
            addr.sun_family = AF_UNIX as SaFamilyT;
            for (index, byte) in socket_name.as_bytes().iter().enumerate() {
                addr.sun_path[index + 1] = *byte as c_char;
            }
            let addr_len = (std::mem::size_of::<SaFamilyT>() + 1 + socket_name.len()) as SockLenT;
            connect(fd, &addr as *const _ as *const RawSockAddr, addr_len)
        };

        if connect_result < 0 {
            unsafe {
                close(fd);
            }
            return Err(CONNECTION_ERROR_MESSAGE.to_string());
        }

        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        self.configure_stream(&stream)?;
        Ok(stream)
    }

    fn configure_stream(&self, stream: &UnixStream) -> Result<(), String> {
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|err| format!("Failed to set read timeout: {}", err))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|err| format!("Failed to set write timeout: {}", err))
    }

    fn call_raw(&self, method: &str, params: Value) -> Result<Value, String> {
        let mut stream = self.connect()?;
        send_request(&mut stream, method, params)
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let payload = self.call_raw(method, params)?;
        extract_result(&payload)
    }

    fn call_raw_with_fallback(
        &self,
        primary_method: &str,
        primary_params: Value,
        fallback_method: &str,
        fallback_params: Value,
    ) -> Result<Value, String> {
        let mut stream = self.connect()?;
        let payload = send_request(&mut stream, primary_method, primary_params.clone())?;
        if method_not_found_response(&payload) {
            let mut fallback_stream = self.connect()?;
            send_request(&mut fallback_stream, fallback_method, fallback_params)
        } else {
            Ok(payload)
        }
    }

    fn call_with_fallback(
        &self,
        primary_method: &str,
        primary_params: Value,
        fallback_method: &str,
        fallback_params: Value,
    ) -> Result<Value, String> {
        let payload = self.call_raw_with_fallback(
            primary_method,
            primary_params,
            fallback_method,
            fallback_params,
        )?;
        extract_result(&payload)
    }

    fn process_prompt(
        &self,
        session_id: &str,
        prompt: &str,
        stream: bool,
    ) -> Result<PromptCall, String> {
        let prompt_client = self.with_timeout_floor(Duration::from_millis(DEFAULT_PROMPT_TIMEOUT_MS));
        if stream {
            prompt_client.process_prompt_stream(session_id, prompt, true)
        } else if prompt.len() >= LONG_PROMPT_STREAM_THRESHOLD_CHARS {
            prompt_client.process_prompt_stream(session_id, prompt, false)
        } else {
            let payload = prompt_client.call_raw_with_fallback(
                "process_prompt",
                json!({
                    "prompt": prompt,
                    "session_id": session_id,
                }),
                "prompt",
                json!({
                    "text": prompt,
                    "session_id": session_id,
                    "stream": false,
                }),
            )?;
            let result = extract_result(&payload)?;
            let text = extract_prompt_text(&result);
            Ok(PromptCall {
                payload,
                text,
                stream_received: false,
            })
        }
    }

    fn with_timeout_floor(&self, minimum_timeout: Duration) -> Self {
        let timeout = if self.timeout < minimum_timeout {
            minimum_timeout
        } else {
            self.timeout
        };

        Self {
            socket_path: self.socket_path.clone(),
            socket_name: self.socket_name.clone(),
            timeout,
        }
    }

    fn process_prompt_stream(
        &self,
        session_id: &str,
        prompt: &str,
        emit_chunks: bool,
    ) -> Result<PromptCall, String> {
        let primary_request = json!({
            "jsonrpc": "2.0",
            "id": next_request_id(),
            "method": "process_prompt_stream",
            "params": {
                "prompt": prompt,
                "session_id": session_id,
            },
        });

        match self.send_stream_request(primary_request, emit_chunks) {
            Ok(response) => Ok(self.prompt_call_from_stream_response(response, emit_chunks)),
            Err(error) if method_not_found(&error) => {
                let fallback_request = json!({
                    "jsonrpc": "2.0",
                    "id": next_request_id(),
                    "method": "prompt",
                    "params": {
                        "text": prompt,
                        "session_id": session_id,
                        "stream": true,
                    },
                });
                let response = self.send_stream_request(fallback_request, emit_chunks)?;
                Ok(self.prompt_call_from_stream_response(response, emit_chunks))
            }
            Err(error) => Err(error),
        }
    }

    fn prompt_call_from_stream_response(
        &self,
        response: RpcResponse,
        emit_chunks: bool,
    ) -> PromptCall {
        let result = extract_result(&response.payload).unwrap_or_else(|_| Value::Null);
        let text = extract_prompt_text(&result).or_else(|| {
            if response.streamed_chunks.is_empty() {
                None
            } else {
                Some(response.streamed_chunks.join(""))
            }
        });

        PromptCall {
            payload: response.payload,
            text,
            stream_received: emit_chunks && !response.streamed_chunks.is_empty(),
        }
    }

    fn send_stream_request(&self, request: Value, emit_chunks: bool) -> Result<RpcResponse, String> {
        let mut stream = self.connect()?;
        write_frame(&mut stream, &request.to_string())?;

        let mut streamed_chunks = Vec::new();
        loop {
            let frame = read_frame(&mut stream, self.timeout)?;
            let payload: Value = serde_json::from_str(&frame)
                .map_err(|err| format!("Invalid JSON-RPC frame: {}", err))?;

            if payload.get("method").and_then(Value::as_str) == Some(STREAM_CHUNK_METHOD) {
                if let Some(chunk) = payload
                    .get("params")
                    .and_then(|value| value.get("chunk"))
                    .and_then(Value::as_str)
                {
                    if emit_chunks {
                        print_stream_delta(chunk);
                    }
                    streamed_chunks.push(chunk.to_string());
                }
                continue;
            }

            if let Some(delta) = payload.get("delta").and_then(Value::as_str).or_else(|| {
                payload
                    .get("result")
                    .and_then(|value| value.get("delta"))
                    .and_then(Value::as_str)
            }) {
                if emit_chunks {
                    print_stream_delta(delta);
                }
                streamed_chunks.push(delta.to_string());
                continue;
            }

            if let Some(lines) = payload
                .get("result")
                .and_then(Value::as_str)
                .filter(|value| value.contains('\n'))
            {
                for delta in extract_ndjson_deltas(lines) {
                    if emit_chunks {
                        print_stream_delta(&delta);
                    }
                    streamed_chunks.push(delta);
                }
            }

            return Ok(RpcResponse {
                payload,
                streamed_chunks,
            });
        }
    }

    fn get_usage(
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

    fn start_dashboard(&self, port: Option<u16>) -> Result<Value, String> {
        let (dashboard_params, fallback_params) = dashboard_rpc_params(port);
        self.call_with_fallback(
            "dashboard.start",
            dashboard_params,
            "start_channel",
            fallback_params,
        )
    }

    fn stop_dashboard(&self) -> Result<Value, String> {
        self.call_with_fallback(
            "dashboard.stop",
            json!({}),
            "stop_channel",
            json!({ "name": "web_dashboard" }),
        )
    }

    fn dashboard_status(&self) -> Result<Value, String> {
        self.call_with_fallback(
            "dashboard.status",
            json!({}),
            "channel_status",
            json!({ "name": "web_dashboard" }),
        )
    }

    fn start_dashboard_raw(&self, port: Option<u16>) -> Result<Value, String> {
        let (dashboard_params, fallback_params) = dashboard_rpc_params(port);
        self.call_raw_with_fallback(
            "dashboard.start",
            dashboard_params,
            "start_channel",
            fallback_params,
        )
    }

    fn stop_dashboard_raw(&self) -> Result<Value, String> {
        self.call_raw_with_fallback(
            "dashboard.stop",
            json!({}),
            "stop_channel",
            json!({ "name": "web_dashboard" }),
        )
    }

    fn dashboard_status_raw(&self) -> Result<Value, String> {
        self.call_raw_with_fallback(
            "dashboard.status",
            json!({}),
            "channel_status",
            json!({ "name": "web_dashboard" }),
        )
    }

    fn clear_agent_data(
        &self,
        include_memory: bool,
        include_sessions: bool,
    ) -> Result<Value, String> {
        self.call(
            "clear_agent_data",
            json!({
                "include_memory": include_memory,
                "include_sessions": include_sessions,
            }),
        )
    }

    fn get_llm_config(&self, path: Option<&str>) -> Result<Value, String> {
        let params = match path {
            Some(path) => json!({ "path": path }),
            None => json!({}),
        };
        self.call("get_llm_config", params)
    }

    fn set_llm_config(&self, path: &str, value: Value) -> Result<Value, String> {
        self.call("set_llm_config", json!({ "path": path, "value": value }))
    }

    fn unset_llm_config(&self, path: &str) -> Result<Value, String> {
        self.call("unset_llm_config", json!({ "path": path }))
    }

    fn reload_llm_backends(&self) -> Result<Value, String> {
        self.call("reload_llm_backends", json!({}))
    }

    fn register_path(&self, kind: &str, path: &str) -> Result<Value, String> {
        self.call("register_path", json!({ "kind": kind, "path": path }))
    }

    fn unregister_path(&self, kind: &str, path: &str) -> Result<Value, String> {
        self.call("unregister_path", json!({ "kind": kind, "path": path }))
    }

    fn list_registered_paths(&self) -> Result<Value, String> {
        self.call("list_registered_paths", json!({}))
    }

    fn list_tasks(&self) -> Result<Value, String> {
        self.call("list_tasks", json!({}))
    }

    fn get_devel_status(&self) -> Result<Value, String> {
        self.call("get_devel_status", json!({}))
    }

    fn get_skill_capabilities(&self) -> Result<Value, String> {
        self.call("get_skill_capabilities", json!({}))
    }

    fn get_tool_audit(&self) -> Result<Value, String> {
        self.call("get_tool_audit", json!({}))
    }
}

fn next_request_id() -> usize {
    RPC_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn is_retryable_read_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    ) || matches!(error.raw_os_error(), Some(11))
}

fn read_exact_with_retry<R: Read>(
    reader: &mut R,
    buf: &mut [u8],
    deadline: Instant,
    context: &str,
) -> Result<(), String> {
    let mut offset = 0usize;
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
            Ok(read) => offset += read,
            Err(error) if is_retryable_read_error(&error) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("IPC {} failed: {}", context, error)),
        }
    }
    Ok(())
}

fn write_frame(stream: &mut UnixStream, payload: &str) -> Result<(), String> {
    let bytes = payload.as_bytes();
    if bytes.len() > MAX_IPC_MESSAGE_SIZE {
        return Err(format!(
            "Payload exceeds maximum IPC size: {} bytes",
            bytes.len()
        ));
    }

    let len = (bytes.len() as u32).to_be_bytes();
    stream
        .write_all(&len)
        .and_then(|_| stream.write_all(bytes))
        .map_err(|err| format!("Failed to write IPC frame: {}", err))
}

fn read_frame(stream: &mut UnixStream, timeout: Duration) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut len_buf = [0u8; 4];
    read_exact_with_retry(stream, &mut len_buf, deadline, "read len")?;
    let payload_len = u32::from_be_bytes(len_buf) as usize;
    if payload_len == 0 || payload_len > MAX_IPC_MESSAGE_SIZE {
        return Err(format!("Invalid IPC payload size: {}", payload_len));
    }

    let mut payload = vec![0u8; payload_len];
    read_exact_with_retry(stream, &mut payload, deadline, "read body")?;
    String::from_utf8(payload).map_err(|err| format!("Invalid UTF-8 IPC frame: {}", err))
}

fn send_request(stream: &mut UnixStream, method: &str, params: Value) -> Result<Value, String> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": next_request_id(),
        "method": method,
        "params": params,
    });
    write_frame(stream, &request.to_string())?;
    let timeout = stream
        .read_timeout()
        .ok()
        .flatten()
        .unwrap_or(Duration::from_millis(DEFAULT_TIMEOUT_MS));
    let payload = read_frame(stream, timeout)?;
    serde_json::from_str(&payload).map_err(|err| format!("Invalid JSON-RPC response: {}", err))
}

fn extract_result(payload: &Value) -> Result<Value, String> {
    if let Some(error) = payload.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Unknown JSON-RPC error");
        return Err(message.to_string());
    }

    payload
        .get("result")
        .cloned()
        .ok_or_else(|| "Missing JSON-RPC result".to_string())
}

fn extract_prompt_text(result: &Value) -> Option<String> {
    result
        .get("text")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| result.as_str().map(ToOwned::to_owned))
}

fn method_not_found(error: &str) -> bool {
    error.to_ascii_lowercase().contains("method not found")
}

fn method_not_found_response(payload: &Value) -> bool {
    payload
        .get("error")
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(method_not_found)
        .unwrap_or(false)
}

fn dashboard_rpc_params(port: Option<u16>) -> (Value, Value) {
    let mut dashboard_params = json!({});
    if let Some(port) = port {
        dashboard_params["port"] = json!(port);
    }

    let mut fallback_params = json!({ "name": "web_dashboard" });
    if let Some(port) = port {
        fallback_params["settings"] = json!({ "port": port });
    }

    (dashboard_params, fallback_params)
}

fn extract_ndjson_deltas(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|value| {
            value
                .get("delta")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    value
                        .get("result")
                        .and_then(|result| result.get("delta"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
        })
        .collect()
}

fn print_stream_delta(delta: &str) {
    print!("{}", delta);
    let _ = io::stdout().flush();
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn print_error_and_exit(error: &str) -> ! {
    if error == CONNECTION_ERROR_MESSAGE {
        eprintln!("{}", error);
    } else {
        eprintln!("Error: {}", error);
    }
    std::process::exit(1);
}

fn generate_session_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = CLI_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("cli_{}_{}", ts, seq)
}

fn parse_usage_baseline(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|err| format!("Invalid usage baseline JSON: {}", err))
}

fn is_tizen_runtime() -> bool {
    libtizenclaw_core::framework::paths::PlatformPaths::detect().is_tizen()
}

fn setup_data_dir() -> PathBuf {
    libtizenclaw_core::framework::paths::PlatformPaths::detect().runtime_root
}

fn setup_config_dir() -> PathBuf {
    setup_data_dir().join("config")
}

fn llm_config_path() -> PathBuf {
    setup_config_dir().join("llm_config.json")
}

fn codex_auth_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".codex").join("auth.json")
}

fn channel_config_path() -> PathBuf {
    setup_config_dir().join("channel_config.json")
}

fn default_dashboard_port() -> u16 {
    if is_tizen_runtime() {
        9090
    } else {
        9091
    }
}

fn dashboard_port_from_doc(doc: &Value) -> u16 {
    doc.get("channels")
        .and_then(Value::as_array)
        .and_then(|channels| {
            channels.iter().find_map(|channel| {
                if channel.get("name").and_then(Value::as_str) == Some("web_dashboard") {
                    channel
                        .get("settings")
                        .and_then(|settings| settings.get("port"))
                        .and_then(Value::as_u64)
                        .and_then(|port| u16::try_from(port).ok())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(default_dashboard_port)
}

fn dashboard_url() -> String {
    let port = fs::read_to_string(channel_config_path())
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .map(|doc| dashboard_port_from_doc(&doc))
        .unwrap_or_else(default_dashboard_port);
    format!("http://localhost:{}", port)
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("object just initialized")
}

fn set_path_value(doc: &mut Value, path: &[&str], new_value: Value) {
    let mut cursor = doc;
    for part in &path[..path.len().saturating_sub(1)] {
        let object = ensure_object(cursor);
        cursor = object
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    let object = ensure_object(cursor);
    object.insert(path[path.len() - 1].to_string(), new_value);
}

fn default_llm_config() -> Value {
    json!({
        "active_backend": "gemini",
        "fallback_backends": ["anthropic", "openai", "openai-codex", "ollama"],
        "benchmark": {
            "pinchbench": {
                "actual_tokens": {
                    "prompt": 0,
                    "completion": 0,
                    "total": 0
                },
                "target": {
                    "score": 0.8,
                    "summary": "Match the target PinchBench run.",
                    "suite": "all"
                }
            }
        },
        "backends": {
            "gemini": {
                "api_key": "",
                "model": "gemini-2.5-flash",
                "temperature": 0.7
            },
            "openai": {
                "api_key": "",
                "model": "gpt-4o",
                "endpoint": "https://api.openai.com/v1"
            },
            "openai-codex": {
                "auth_mode": "oauth",
                "model": "gpt-5.4",
                "endpoint": "https://chatgpt.com/backend-api",
                "transport": "responses",
                "api_path": "/codex/responses",
                "oauth": {
                    "access_token": "",
                    "refresh_token": "",
                    "id_token": "",
                    "expires_at": 0,
                    "account_id": "",
                    "auth_path": "",
                    "source": "codex_cli"
                }
            },
            "anthropic": {
                "api_key": "",
                "model": "claude-sonnet-4-20250514",
                "endpoint": "https://api.anthropic.com/v1",
                "temperature": 0.7
            },
            "xai": {
                "api_key": "",
                "model": "grok-3",
                "endpoint": "https://api.x.ai/v1"
            },
            "ollama": {
                "model": "llama3",
                "endpoint": "http://localhost:11434"
            }
        },
        "features": {
            "image_generation": {
                "provider": "openai",
                "api_key": "",
                "model": "gpt-image-1",
                "endpoint": "https://api.openai.com/v1",
                "size": "1024x1024",
                "background": "auto"
            }
        }
    })
}

fn default_telegram_config() -> Value {
    let default_workdir = std::env::current_dir()
        .unwrap_or_else(|_| setup_data_dir())
        .display()
        .to_string();
    json!({
        "bot_token": "",
        "allowed_chat_ids": [],
        "cli_workdir": default_workdir,
        "cli_backends": {
            "default_backend": "codex",
            "backends": {}
        }
    })
}

fn telegram_cli_default_backend(doc: &Value) -> String {
    doc.get("cli_backends")
        .and_then(|value| value.get("default_backend"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("codex")
        .to_string()
}

fn telegram_cli_backend_entries(doc: &Value) -> Map<String, Value> {
    doc.get("cli_backends")
        .and_then(|value| value.get("backends"))
        .and_then(Value::as_object)
        .cloned()
        .or_else(|| {
            doc.get("cli_backends")
                .and_then(Value::as_object)
                .map(|object| {
                    object
                        .iter()
                        .filter(|(key, _)| key.as_str() != "default_backend")
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect()
                })
        })
        .unwrap_or_default()
}

fn set_telegram_cli_backends(doc: &mut Value, default_backend: &str, backends: Map<String, Value>) {
    doc["cli_backends"] = json!({
        "default_backend": default_backend,
        "backends": backends
    });
}

fn load_json_or_default(path: &Path, default_value: Value) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .unwrap_or(default_value)
}

fn write_pretty_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create config directory '{}': {}",
                parent.display(),
                err
            )
        })?;
    }
    let serialized = serde_json::to_string_pretty(value)
        .map_err(|err| format!("Failed to serialize JSON for '{}': {}", path.display(), err))?;
    fs::write(path, serialized)
        .map_err(|err| format!("Failed to write '{}': {}", path.display(), err))
}

fn prompt_line(prompt: &str) -> Result<String, String> {
    print!("{}", prompt);
    io::stdout()
        .flush()
        .map_err(|err| format!("Failed to flush stdout: {}", err))?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|err| format!("Failed to read user input: {}", err))?;
    Ok(line.trim().to_string())
}

fn prompt_with_default(prompt: &str, default: Option<&str>) -> Result<String, String> {
    let prompt_text = match default {
        Some(value) if !value.is_empty() => format!("{} [{}]: ", prompt, value),
        _ => format!("{}: ", prompt),
    };
    let value = prompt_line(&prompt_text)?;
    if value.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(value)
    }
}

fn prompt_secret(prompt: &str, has_existing: bool) -> Result<Option<String>, String> {
    let suffix = if has_existing {
        " [press Enter to keep the saved value]"
    } else {
        " [press Enter to skip for now]"
    };
    let value = prompt_line(&format!("{}{}: ", prompt, suffix))?;
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn prompt_choice(prompt: &str, options: &[&str], default_index: usize) -> Result<usize, String> {
    println!("\n{}", prompt);
    for (index, option) in options.iter().enumerate() {
        println!("  {}. {}", index + 1, option);
    }

    loop {
        let default_value = (default_index + 1).to_string();
        let raw = prompt_with_default("Select an option", Some(&default_value))?;
        match raw.parse::<usize>() {
            Ok(value) if value >= 1 && value <= options.len() => return Ok(value - 1),
            _ => println!("Please enter a number between 1 and {}.", options.len()),
        }
    }
}

fn parse_chat_ids(raw: &str) -> Result<Vec<i64>, String> {
    let mut ids = Vec::new();
    for token in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let value = token
            .parse::<i64>()
            .map_err(|_| format!("Invalid chat id '{}'", token))?;
        ids.push(value);
    }
    Ok(ids)
}

fn find_in_path(candidates: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for candidate in candidates {
            let candidate_path = dir.join(candidate);
            if candidate_path.is_file() {
                return Some(candidate_path);
            }
        }
    }
    None
}

fn detect_backend_path(backend: &str) -> Option<String> {
    let candidate_lists: &[&[&str]] = match backend {
        "codex" => &[&["codex"]],
        "gemini" => &[&["gemini"], &["/snap/bin/gemini"]],
        "claude" => &[&["claude"], &["claude-code"]],
        _ => return None,
    };

    for candidates in candidate_lists {
        if candidates.len() == 1 && candidates[0].starts_with('/') {
            let path = Path::new(candidates[0]);
            if path.is_file() {
                return Some(path.display().to_string());
            }
            continue;
        }
        if let Some(path) = find_in_path(candidates) {
            return Some(path.display().to_string());
        }
    }
    None
}

fn detected_cli_backends() -> Map<String, Value> {
    let mut map = Map::new();
    for backend in ["codex", "gemini", "claude"] {
        if let Some(path) = detect_backend_path(backend) {
            map.insert(backend.to_string(), Value::String(path));
        }
    }
    map
}

fn configure_llm(doc: &mut Value) -> Result<(), String> {
    let current_backend = doc
        .get("active_backend")
        .and_then(Value::as_str)
        .unwrap_or("gemini");
    let backends = [
        "gemini",
        "openai",
        "openai-codex",
        "anthropic",
        "xai",
        "ollama",
    ];
    let default_index = backends
        .iter()
        .position(|backend| *backend == current_backend)
        .unwrap_or(0);
    let labels = [
        "Gemini",
        "OpenAI",
        "OpenAI Codex (ChatGPT session)",
        "Anthropic (Claude API)",
        "xAI",
        "Ollama",
    ];
    let choice = prompt_choice("Choose an LLM backend to configure", &labels, default_index)?;
    let backend = backends[choice];

    set_path_value(doc, &["active_backend"], Value::String(backend.to_string()));

    let backend_model_path = ["backends", backend, "model"];
    let current_model = doc
        .get("backends")
        .and_then(|value| value.get(backend))
        .and_then(|value| value.get("model"))
        .and_then(Value::as_str)
        .unwrap_or(match backend {
            "gemini" => "gemini-2.5-flash",
            "openai" => "gpt-4o",
            "openai-codex" => "gpt-5.4",
            "anthropic" => "claude-sonnet-4-20250514",
            "xai" => "grok-3",
            "ollama" => "llama3",
            _ => "",
        });
    let model = prompt_with_default("Model name", Some(current_model))?;
    set_path_value(doc, &backend_model_path, Value::String(model));

    match backend {
        "ollama" => {
            let current_endpoint = doc
                .get("backends")
                .and_then(|value| value.get("ollama"))
                .and_then(|value| value.get("endpoint"))
                .and_then(Value::as_str)
                .unwrap_or("http://localhost:11434");
            let endpoint = prompt_with_default("Ollama endpoint", Some(current_endpoint))?;
            set_path_value(
                doc,
                &["backends", "ollama", "endpoint"],
                Value::String(endpoint),
            );
        }
        "openai-codex" => {
            println!(
                "OpenAI Codex uses the ChatGPT/Codex CLI session. Run `tizenclaw-cli auth openai-codex login` to link it."
            );
        }
        _ => {
            let has_existing_key = doc
                .get("backends")
                .and_then(|value| value.get(backend))
                .and_then(|value| value.get("api_key"))
                .and_then(Value::as_str)
                .map(|value| !value.is_empty())
                .unwrap_or(false);
            if let Some(api_key) = prompt_secret("API key", has_existing_key)? {
                set_path_value(
                    doc,
                    &["backends", backend, "api_key"],
                    Value::String(api_key),
                );
            }

            if backend == "openai" || backend == "anthropic" || backend == "xai" {
                let current_endpoint = doc
                    .get("backends")
                    .and_then(|value| value.get(backend))
                    .and_then(|value| value.get("endpoint"))
                    .and_then(Value::as_str)
                    .unwrap_or(match backend {
                        "openai" => "https://api.openai.com/v1",
                        "anthropic" => "https://api.anthropic.com/v1",
                        "xai" => "https://api.x.ai/v1",
                        _ => "",
                    });
                let endpoint = prompt_with_default("API endpoint", Some(current_endpoint))?;
                set_path_value(
                    doc,
                    &["backends", backend, "endpoint"],
                    Value::String(endpoint),
                );
            }
        }
    }

    Ok(())
}

fn merge_missing(target: &mut Value, defaults: &Value) {
    if let Value::Object(default_map) = defaults {
        if let Value::Object(target_map) = target {
            for (key, default_value) in default_map {
                match target_map.get_mut(key) {
                    Some(existing) => merge_missing(existing, default_value),
                    None => {
                        target_map.insert(key.clone(), default_value.clone());
                    }
                }
            }
            return;
        }
    }

    if matches!(target, Value::Null) {
        *target = defaults.clone();
    }
}

fn codex_cli_path() -> Option<PathBuf> {
    detect_backend_path("codex").map(PathBuf::from)
}

fn parse_codex_login_state(output: &str) -> &'static str {
    let normalized = output.to_ascii_lowercase();
    if normalized.contains("logged in") {
        "logged_in"
    } else if normalized.contains("logged out") || normalized.contains("not logged in") {
        "logged_out"
    } else {
        "unknown"
    }
}

fn read_codex_auth_doc() -> Result<Value, String> {
    let path = codex_auth_path();
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read '{}': {}", path.display(), err))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("Failed to parse '{}': {}", path.display(), err))
}

fn codex_auth_string(auth_doc: &Value, key: &str) -> Option<String> {
    auth_doc
        .get("tokens")
        .and_then(Value::as_object)
        .and_then(|tokens| tokens.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            auth_doc
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
}

fn codex_auth_i64(auth_doc: &Value, key: &str) -> Option<i64> {
    auth_doc
        .get("tokens")
        .and_then(Value::as_object)
        .and_then(|tokens| tokens.get(key))
        .and_then(Value::as_i64)
        .or_else(|| auth_doc.get(key).and_then(Value::as_i64))
        .filter(|value| *value > 0)
}

fn decode_base64url_nopad(input: &str) -> Option<Vec<u8>> {
    fn decode_char(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }

    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Some(Vec::new());
    }

    let mut decoded = Vec::with_capacity((bytes.len() * 3) / 4 + 3);
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0usize;

    for &byte in bytes {
        chunk[chunk_len] = decode_char(byte)?;
        chunk_len += 1;
        if chunk_len == 4 {
            decoded.push((chunk[0] << 2) | (chunk[1] >> 4));
            decoded.push((chunk[1] << 4) | (chunk[2] >> 2));
            decoded.push((chunk[2] << 6) | chunk[3]);
            chunk_len = 0;
        }
    }

    match chunk_len {
        0 => {}
        2 => {
            decoded.push((chunk[0] << 2) | (chunk[1] >> 4));
        }
        3 => {
            decoded.push((chunk[0] << 2) | (chunk[1] >> 4));
            decoded.push((chunk[1] << 4) | (chunk[2] >> 2));
        }
        _ => return None,
    }

    Some(decoded)
}

fn decode_jwt_payload(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = decode_base64url_nopad(payload)?;
    serde_json::from_slice::<Value>(&decoded).ok()
}

fn jwt_exp(token: &str) -> Option<i64> {
    decode_jwt_payload(token)?
        .get("exp")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
}

fn codex_oauth_snapshot(auth_doc: &Value) -> Value {
    let mut oauth = Map::new();
    let access_token = codex_auth_string(auth_doc, "access_token");

    for key in ["access_token", "refresh_token", "id_token", "account_id"] {
        if let Some(value) = access_token
            .as_ref()
            .filter(|_| key == "access_token")
            .cloned()
            .or_else(|| codex_auth_string(auth_doc, key))
        {
            oauth.insert(key.to_string(), Value::String(value));
        }
    }

    // `llm_config.json` starts with `expires_at=0`. If connect/import
    // leaves that placeholder behind, later daemon restarts can treat a
    // still-valid Codex session as already expired and force an avoidable
    // OAuth refresh path.
    if let Some(expires_at) =
        codex_auth_i64(auth_doc, "expires_at").or_else(|| access_token.as_deref().and_then(jwt_exp))
    {
        oauth.insert("expires_at".to_string(), Value::from(expires_at));
    }

    oauth.insert(
        "auth_path".to_string(),
        Value::String(codex_auth_path().display().to_string()),
    );

    Value::Object(oauth)
}

fn codex_login_status() -> Value {
    let cli_path = codex_cli_path();
    let auth_path = codex_auth_path();
    let auth_doc = read_codex_auth_doc().ok();
    let mut state = json!({
        "status": "ok",
        "provider": "openai-codex",
        "codex_cli_available": cli_path.is_some(),
        "codex_cli_path": cli_path.as_ref().map(|path| path.display().to_string()),
        "codex_auth_file_exists": auth_path.exists(),
        "codex_auth_path": auth_path.display().to_string(),
        "codex_login_state": "unknown",
        "codex_login_output": "",
        "config_backend_present": false,
        "config_active_backend": "",
        "oauth_source": "",
        "account_id": "",
        "linked": false,
        "message": ""
    });

    if let Some(path) = cli_path {
        match Command::new(&path).args(["login", "status"]).output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let combined = if stdout.is_empty() {
                    stderr.clone()
                } else if stderr.is_empty() {
                    stdout.clone()
                } else {
                    format!("{}\n{}", stdout, stderr)
                };
                state["codex_login_state"] =
                    Value::String(parse_codex_login_state(&combined).to_string());
                state["codex_login_output"] = Value::String(combined);
            }
            Err(err) => {
                state["status"] = Value::String("error".to_string());
                state["message"] =
                    Value::String(format!("Failed to run Codex CLI login status: {}", err));
            }
        }
    } else {
        state["status"] = Value::String("error".to_string());
        state["message"] = Value::String("Codex CLI binary was not found in PATH".to_string());
    }

    if let Ok(llm_doc) = fs::read_to_string(llm_config_path())
        .map_err(|err| err.to_string())
        .and_then(|content| serde_json::from_str::<Value>(&content).map_err(|err| err.to_string()))
    {
        let active_backend = llm_doc
            .get("active_backend")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let backend = llm_doc
            .get("backends")
            .and_then(|backends| backends.get("openai-codex"));
        state["config_active_backend"] = Value::String(active_backend.clone());
        state["config_backend_present"] = Value::Bool(backend.is_some());
        state["oauth_source"] = Value::String(
            backend
                .and_then(|value| value.get("oauth"))
                .and_then(|value| value.get("source"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        );
        if active_backend == "openai-codex" && backend.is_some() {
            state["linked"] = Value::Bool(true);
        }
    }

    if let Some(doc) = auth_doc {
        let account_id = codex_auth_string(&doc, "account_id").unwrap_or_default();
        if !account_id.is_empty() {
            state["account_id"] = Value::String(account_id);
        }
    }

    if state["message"].as_str().unwrap_or("").is_empty() {
        let logged_in = state["codex_login_state"].as_str() == Some("logged_in");
        let linked = state["linked"].as_bool().unwrap_or(false);
        let auth_exists = state["codex_auth_file_exists"].as_bool().unwrap_or(false);
        state["message"] = Value::String(match (logged_in, auth_exists, linked) {
            (true, true, true) => {
                "Codex CLI session is available and TizenClaw is linked to openai-codex."
                    .to_string()
            }
            (true, true, false) => {
                "Codex CLI session is available. Run `tizenclaw-cli auth openai-codex connect` to link TizenClaw."
                    .to_string()
            }
            (true, false, _) => {
                "Codex CLI reports a login session, but the local auth store was not found yet."
                    .to_string()
            }
            (false, _, _) => {
                "No Codex CLI login session is active. Run `tizenclaw-cli auth openai-codex login` first."
                    .to_string()
            }
        });
    }

    state
}

fn should_retry_reload_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("connection refused") || normalized.contains("failed to connect to daemon")
}

fn reload_attempt() -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = IpcClient::from_options(&CliOptions::default())
            .reload_llm_backends()
            .map(|_| ());
        let _ = sender.send(result);
    });

    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or_else(|_| Err("Daemon backend reload timed out after 2 seconds".to_string()))
}

fn try_reload_llm_backends_with<F>(mut reload: F) -> (bool, Option<String>)
where
    F: FnMut() -> Result<(), String>,
{
    let mut last_error = None;
    for attempt in 0..5 {
        match reload() {
            Ok(()) => return (true, None),
            Err(err) => {
                let retryable = should_retry_reload_error(&err) && attempt < 4;
                last_error = Some(err);
                if retryable {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
                break;
            }
        }
    }

    (false, last_error)
}

fn try_reload_llm_backends() -> (bool, Option<String>) {
    try_reload_llm_backends_with(reload_attempt)
}

fn reload_message(reloaded: bool, reload_error: Option<&str>) -> String {
    if reloaded {
        "TizenClaw is now linked to the Codex CLI session and the daemon reloaded the backend."
            .to_string()
    } else if reload_error.map(should_retry_reload_error).unwrap_or(false) {
        "TizenClaw is now linked to the Codex CLI session. The daemon did not answer the first reload window, so restart or retry once if the new backend is not visible yet."
            .to_string()
    } else if reload_error
        .map(|error| error.contains("timed out"))
        .unwrap_or(false)
    {
        "TizenClaw is now linked to the Codex CLI session. The daemon reload did not finish in time, so verify the backend from the dashboard or retry once."
            .to_string()
    } else {
        "TizenClaw is now linked to the Codex CLI session. Start or reload the daemon to activate it."
            .to_string()
    }
}

fn connect_codex_session() -> Result<Value, String> {
    let status = codex_login_status();
    if status.get("codex_login_state").and_then(Value::as_str) != Some("logged_in") {
        return Err(status
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Codex CLI is not logged in")
            .to_string());
    }

    if !status
        .get("codex_auth_file_exists")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!(
            "Codex auth file '{}' was not found",
            codex_auth_path().display()
        ));
    }

    let config_path = llm_config_path();
    let mut doc = load_json_or_default(&config_path, default_llm_config());
    merge_missing(&mut doc, &default_llm_config());

    if doc.get("backends").and_then(Value::as_object).is_none() {
        doc["backends"] = Value::Object(Map::new());
    }
    if doc["backends"].get("openai-codex").is_none() {
        doc["backends"]["openai-codex"] = default_llm_config()["backends"]["openai-codex"].clone();
    } else {
        let defaults = default_llm_config()["backends"]["openai-codex"].clone();
        merge_missing(&mut doc["backends"]["openai-codex"], &defaults);
    }

    doc["active_backend"] = Value::String("openai-codex".to_string());

    let fallback_backends = doc
        .get("fallback_backends")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !fallback_backends
        .iter()
        .any(|value| value.as_str() == Some("openai-codex"))
    {
        let mut updated = fallback_backends;
        updated.push(Value::String("openai-codex".to_string()));
        doc["fallback_backends"] = Value::Array(updated);
    }

    doc["backends"]["openai-codex"]["oauth"]["source"] = Value::String("codex_cli".to_string());
    if let Ok(auth_doc) = read_codex_auth_doc() {
        if let Value::Object(snapshot) = codex_oauth_snapshot(&auth_doc) {
            for (key, value) in snapshot {
                doc["backends"]["openai-codex"]["oauth"][&key] = value;
            }
        }
        if let Some(account_id) = auth_doc
            .get("tokens")
            .and_then(|value| value.get("account_id"))
            .and_then(Value::as_str)
        {
            if !account_id.is_empty() {
                doc["backends"]["openai-codex"]["oauth"]["account_id"] =
                    Value::String(account_id.to_string());
            }
        }
    }

    write_pretty_json(&config_path, &doc)?;
    let (reloaded, reload_error) = try_reload_llm_backends();
    let message = reload_message(reloaded, reload_error.as_deref());

    Ok(json!({
        "status": "ok",
        "provider": "openai-codex",
        "linked": true,
        "config_path": config_path.display().to_string(),
        "active_backend": "openai-codex",
        "reloaded": reloaded,
        "reload_error": reload_error,
        "dashboard_url": dashboard_url(),
        "message": message
    }))
}

fn print_auth_result(result: &Value, as_json: bool) {
    if as_json {
        print_json(result);
        return;
    }

    let message = result
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Done.");
    println!("{}", message);

    if let Some(path) = result.get("config_path").and_then(Value::as_str) {
        println!("LLM config: {}", path);
    }
    if let Some(output) = result.get("codex_login_output").and_then(Value::as_str) {
        if !output.trim().is_empty() {
            println!("Codex CLI: {}", output.trim());
        }
    }
    if let Some(error) = result.get("reload_error").and_then(Value::as_str) {
        if !error.trim().is_empty() {
            println!("Daemon reload note: {}", error.trim());
        }
    }
}

fn cmd_auth(args: &[String]) {
    if args.len() < 2 || args[0] != "openai-codex" {
        eprintln!("Usage:");
        eprintln!("  tizenclaw-cli auth openai-codex status [--json]");
        eprintln!("  tizenclaw-cli auth openai-codex connect [--json]");
        eprintln!("  tizenclaw-cli auth openai-codex import [--json]");
        eprintln!("  tizenclaw-cli auth openai-codex login [--json]");
        std::process::exit(1);
    }

    let action = args[1].as_str();
    let as_json = args[2..]
        .iter()
        .any(|arg| arg == "--json" || arg == "--strict-json");

    match action {
        "status" => {
            let result = codex_login_status();
            print_auth_result(&result, as_json);
        }
        "connect" | "import" => match connect_codex_session() {
            Ok(result) => print_auth_result(&result, as_json),
            Err(error) => print_error_and_exit(&error),
        },
        "login" => {
            let cli_path = codex_cli_path()
                .ok_or_else(|| "Codex CLI binary was not found in PATH".to_string())
                .unwrap_or_else(|err| print_error_and_exit(&err));
            let status = Command::new(&cli_path)
                .args(["login"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .map_err(|err| format!("Failed to launch Codex CLI login: {}", err))
                .unwrap_or_else(|err| print_error_and_exit(&err));
            if !status.success() {
                print_error_and_exit("Codex CLI login did not complete successfully");
            }
            match connect_codex_session() {
                Ok(result) => print_auth_result(&result, as_json),
                Err(error) => print_error_and_exit(&error),
            }
        }
        _ => {
            eprintln!("Unknown auth action '{}'", action);
            std::process::exit(1);
        }
    }
}

fn print_botfather_guide() {
    println!("\nTelegram setup guide:");
    println!("  1. Open Telegram and search for @BotFather.");
    println!("  2. Run /newbot and follow the prompts.");
    println!("  3. Copy the bot token that BotFather gives you.");
    println!("  4. Send at least one message to your bot from the account you want to use.");
    println!("  5. Optionally restrict access with allowed_chat_ids after you know your chat id.");
}

fn configure_telegram(doc: &mut Value) -> Result<bool, String> {
    print_botfather_guide();

    let has_existing_token = doc
        .get("bot_token")
        .and_then(Value::as_str)
        .map(|value| !value.is_empty() && value != "YOUR_TELEGRAM_BOT_TOKEN_HERE")
        .unwrap_or(false);
    if let Some(token) = prompt_secret("Telegram bot token", has_existing_token)? {
        doc["bot_token"] = Value::String(token);
    }

    let token = doc
        .get("bot_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if token.is_empty() || token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
        println!("Telegram setup skipped because no bot token was provided.");
        return Ok(false);
    }

    let existing_ids = doc
        .get("allowed_chat_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let allowlist_default = if existing_ids.is_empty() { 0 } else { 1 };
    let allowlist_choice = prompt_choice(
        "How should Telegram access be handled?",
        &[
            "Keep it open for now (empty allowlist, easier for first-time testing)",
            "Enter allowed chat IDs now",
        ],
        allowlist_default,
    )?;
    if allowlist_choice == 0 {
        doc["allowed_chat_ids"] = Value::Array(vec![]);
        println!("Note: an empty allowlist means any chat that reaches the bot can talk to it.");
    } else {
        let existing = existing_ids
            .iter()
            .filter_map(Value::as_i64)
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let prompt_default = if existing.is_empty() {
            None
        } else {
            Some(existing.as_str())
        };
        loop {
            let raw = prompt_with_default("Comma-separated allowed chat IDs", prompt_default)?;
            match parse_chat_ids(&raw) {
                Ok(ids) => {
                    doc["allowed_chat_ids"] =
                        Value::Array(ids.into_iter().map(|id| Value::Number(id.into())).collect());
                    break;
                }
                Err(err) => println!("{}", err),
            }
        }
    }

    let current_workdir = doc
        .get("cli_workdir")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| setup_data_dir())
                .display()
                .to_string()
        });
    let cli_workdir = prompt_with_default(
        "Default project directory for Telegram coding mode",
        Some(&current_workdir),
    )?;
    doc["cli_workdir"] = Value::String(cli_workdir);

    let detected = detected_cli_backends();
    if !detected.is_empty() {
        println!("\nDetected coding-agent CLIs:");
        for (name, value) in &detected {
            if let Some(path) = value.as_str() {
                println!("  - {}: {}", name, path);
            }
        }
    } else {
        println!("\nNo coding-agent CLI binaries were auto-detected in PATH.");
    }

    let default_backend = telegram_cli_default_backend(doc);
    let existing_paths = telegram_cli_backend_entries(doc);
    let backend_path_choice = prompt_choice(
        "How should Telegram CLI backend paths be configured?",
        &[
            "Use detected paths where available",
            "Review and edit paths now",
            "Keep the existing file values",
        ],
        if existing_paths.is_empty() { 0 } else { 2 },
    )?;

    match backend_path_choice {
        0 => {
            let mut merged = existing_paths.clone();
            for (backend, value) in detected {
                merged.insert(backend, value);
            }
            set_telegram_cli_backends(doc, &default_backend, merged);
        }
        1 => {
            let mut manual = existing_paths.clone();
            for backend in ["codex", "gemini", "claude"] {
                let fallback = existing_paths
                    .get(backend)
                    .and_then(|value| {
                        value
                            .as_str()
                            .or_else(|| value.get("binary_path").and_then(Value::as_str))
                    })
                    .or_else(|| detected.get(backend).and_then(Value::as_str));
                let value =
                    prompt_with_default(&format!("Path for the {} CLI binary", backend), fallback)?;
                if !value.trim().is_empty() {
                    manual.insert(
                        backend.to_string(),
                        json!({
                            "binary_path": value
                        }),
                    );
                }
            }
            set_telegram_cli_backends(doc, &default_backend, manual);
        }
        _ => {
            if doc.get("cli_backends").is_none() {
                set_telegram_cli_backends(doc, &default_backend, existing_paths);
            }
        }
    }

    Ok(true)
}

fn print_setup_summary(config_dir: &Path, configured_now: bool) {
    println!("\nSetup summary:");
    println!("  Dashboard: {}", dashboard_url());
    println!("  Config directory: {}", config_dir.display());
    println!("  Open the dashboard in your browser with the URL above.");
    println!("  Start the dashboard manually: tizenclaw-cli dashboard start");
    println!("  Dashboard status command: tizenclaw-cli dashboard status");
    if configured_now {
        println!("  To rerun setup later: tizenclaw-cli setup");
        println!("  Telegram changes need a daemon restart to become active.");
    } else {
        println!("  Setup was postponed. You can continue with the dashboard now.");
        println!("  To configure later: tizenclaw-cli setup");
    }
}

fn cmd_setup() {
    let config_dir = setup_config_dir();
    let llm_path = config_dir.join("llm_config.json");
    let telegram_path = config_dir.join("telegram_config.json");

    println!("TizenClaw setup wizard");
    println!("This wizard prepares host-side LLM and Telegram settings.");

    let start_choice = prompt_choice(
        "How would you like to continue?",
        &[
            "Configure now",
            "Configure later and use the dashboard first",
        ],
        0,
    )
    .unwrap_or_else(|err| print_error_and_exit(&err));

    if start_choice == 1 {
        print_setup_summary(&config_dir, false);
        return;
    }

    let mut llm_doc = load_json_or_default(&llm_path, default_llm_config());
    configure_llm(&mut llm_doc).unwrap_or_else(|err| print_error_and_exit(&err));
    write_pretty_json(&llm_path, &llm_doc).unwrap_or_else(|err| print_error_and_exit(&err));

    let telegram_choice = prompt_choice(
        "Do you want to configure Telegram coding mode now?",
        &[
            "Yes, configure Telegram now",
            "No, I will set up Telegram later",
        ],
        1,
    )
    .unwrap_or_else(|err| print_error_and_exit(&err));

    if telegram_choice == 0 {
        let mut telegram_doc = load_json_or_default(&telegram_path, default_telegram_config());
        if configure_telegram(&mut telegram_doc).unwrap_or_else(|err| print_error_and_exit(&err)) {
            write_pretty_json(&telegram_path, &telegram_doc)
                .unwrap_or_else(|err| print_error_and_exit(&err));
        }
    }

    print_setup_summary(&config_dir, true);
}

fn usage_value(
    client: &IpcClient,
    session_id: Option<&str>,
    baseline: Option<&Value>,
) -> Result<Value, String> {
    client.get_usage(session_id, baseline)
}

fn usage_payload(
    client: &IpcClient,
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
    client.call_raw("get_usage", params)
}

fn show_usage(client: &IpcClient, session_id: Option<&str>, baseline: Option<&Value>) {
    match usage_value(client, session_id, baseline) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn send_prompt(
    client: &IpcClient,
    session_id: &str,
    prompt: &str,
    stream: bool,
) -> Result<PromptCall, String> {
    let response = client.process_prompt(session_id, prompt, stream)?;

    if !response.stream_received {
        if let Some(text) = response.text.as_deref() {
            println!("{}", text);
        }
    } else {
        println!();
    }

    Ok(response)
}

fn parse_dashboard_command(input: &str) -> (String, Option<u16>) {
    let mut parts = input.split_whitespace();
    let action = parts.next().unwrap_or("").to_string();
    let mut port = None;

    while let Some(part) = parts.next() {
        if part == "--port" {
            let value = parts.next().unwrap_or("");
            match value.parse::<u16>() {
                Ok(parsed) if parsed > 0 => port = Some(parsed),
                _ => {
                    eprintln!("Error: invalid dashboard port '{}'", value);
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!(
                "Unknown dashboard option '{}'. Use: start [--port N] | stop | status",
                part
            );
            std::process::exit(1);
        }
    }

    (action, port)
}

/// Handle `tizenclaw-cli dashboard <action> [--port N]`.
fn dashboard_value(client: &IpcClient, command: &str) -> Result<Value, String> {
    let (action, port) = parse_dashboard_command(command);

    match action.as_str() {
        "start" => client.start_dashboard(port),
        "stop" => client.stop_dashboard(),
        "status" => client.dashboard_status(),
        _ => Err(format!(
            "Unknown dashboard action '{}'. Use: start [--port N] | stop | status",
            action
        )),
    }
}

fn dashboard_payload(client: &IpcClient, command: &str) -> Result<Value, String> {
    let (action, port) = parse_dashboard_command(command);

    match action.as_str() {
        "start" => client.start_dashboard_raw(port),
        "stop" => client.stop_dashboard_raw(),
        "status" => client.dashboard_status_raw(),
        _ => Err(format!(
            "Unknown dashboard action '{}'. Use: start [--port N] | stop | status",
            action
        )),
    }
}

fn cmd_dashboard(client: &IpcClient, command: &str) {
    match dashboard_value(client, command) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_clear_data(client: &IpcClient, args: &[String]) {
    let mut include_memory = true;
    let mut include_sessions = true;
    let mut json_output = false;

    for arg in args {
        match arg.as_str() {
            "--memory-only" => {
                include_memory = true;
                include_sessions = false;
            }
            "--sessions-only" => {
                include_memory = false;
                include_sessions = true;
            }
            "--json" => {
                json_output = true;
            }
            _ => {
                eprintln!(
                    "Usage: tizenclaw-cli clear-data [--memory-only|--sessions-only] [--json]"
                );
                std::process::exit(1);
            }
        }
    }

    if !include_memory && !include_sessions {
        eprintln!("Error: at least one clear-data target must be enabled.");
        std::process::exit(1);
    }

    match client.clear_agent_data(include_memory, include_sessions) {
        Ok(result) => {
            if json_output {
                print_json(&result);
                return;
            }

            println!("Agent data cleared.");
            if include_memory {
                let cleared = result
                    .get("memory")
                    .and_then(|value| value.get("records_deleted"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                println!("  Memory records deleted: {}", cleared);
            }
            if include_sessions {
                let sessions_deleted = result
                    .get("sessions")
                    .and_then(|value| value.get("sessions_deleted"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let workdirs_deleted = result
                    .get("sessions")
                    .and_then(|value| value.get("workdirs_deleted"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                println!("  Session files deleted: {}", sessions_deleted);
                println!("  Workdirs deleted: {}", workdirs_deleted);
            }
        }
        Err(error) => print_error_and_exit(&error),
    }
}

/// Interactive REPL mode.
fn interactive_mode(client: &IpcClient, explicit_session_id: Option<&str>, stream: bool) {
    match explicit_session_id {
        Some(session_id) => println!("TizenClaw CLI interactive mode (session: {})", session_id),
        None => println!("TizenClaw CLI interactive mode"),
    }
    println!("Type 'quit' or 'exit' to leave. Type '/help' for commands.\n");

    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "quit" | "exit" => break,
            "/help" => {
                println!("Commands:");
                println!("  /usage            Show token usage");
                println!("  /clear-data       Clear memory and session data");
                println!("  /dashboard start [--port N] Start web dashboard");
                println!("  /dashboard stop   Stop web dashboard");
                println!("  /dashboard status Show dashboard status");
                println!("  -s <id>           Re-run CLI with a fixed session");
                println!("  quit, exit        Exit");
                println!("  <text>            Send prompt");
            }
            cmd if cmd.starts_with("/usage") => {
                show_usage(client, explicit_session_id, None);
            }
            cmd if cmd.starts_with("/clear-data") => {
                let args = cmd
                    .trim_start_matches("/clear-data")
                    .split_whitespace()
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                cmd_clear_data(client, &args);
            }
            cmd if cmd.starts_with("/dashboard ") => {
                let action = cmd.trim_start_matches("/dashboard ").trim();
                cmd_dashboard(client, action);
            }
            prompt => {
                let session_id = explicit_session_id
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(generate_session_id);
                if let Err(error) = send_prompt(client, &session_id, prompt, stream) {
                    eprintln!("{}", error);
                }
            }
        }
    }
}

fn cmd_config_get(client: &IpcClient, path: Option<&str>) {
    match client.get_llm_config(path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_set(client: &IpcClient, path: &str, raw_value: &str, strict_json: bool) {
    let value = if strict_json {
        match serde_json::from_str::<Value>(raw_value) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Error: invalid JSON value: {}", err);
                std::process::exit(1);
            }
        }
    } else {
        Value::String(raw_value.to_string())
    };

    match client.set_llm_config(path, value) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_unset(client: &IpcClient, path: &str) {
    match client.unset_llm_config(path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_reload(client: &IpcClient) {
    match client.reload_llm_backends() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config(client: &IpcClient, args: &[String]) {
    match args.first().map(String::as_str) {
        Some("get") => {
            cmd_config_get(client, args.get(1).map(String::as_str));
        }
        Some("set") => {
            if args.len() < 3 {
                eprintln!("Usage: tizenclaw-cli config set <path> <value> [--strict-json]");
                std::process::exit(1);
            }
            let strict_json = args[3..]
                .iter()
                .any(|arg| arg == "--strict-json" || arg == "--json");
            cmd_config_set(client, &args[1], &args[2], strict_json);
        }
        Some("unset") => {
            if args.len() < 2 {
                eprintln!("Usage: tizenclaw-cli config unset <path>");
                std::process::exit(1);
            }
            cmd_config_unset(client, &args[1]);
        }
        Some("reload") => {
            cmd_config_reload(client);
        }
        _ => {
            eprintln!("Usage:");
            eprintln!("  tizenclaw-cli config get [path]");
            eprintln!("  tizenclaw-cli config set <path> <value> [--strict-json]");
            eprintln!("  tizenclaw-cli config unset <path>");
            eprintln!("  tizenclaw-cli config reload");
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("tizenclaw-cli — TizenClaw CLI\n");
    eprintln!("Usage:");
    eprintln!("  tizenclaw-cli [options] [prompt]\n");
    eprintln!("Options:");
    eprintln!("  -s, --session <id>  Reuse a fixed session ID");
    eprintln!("  --stream            Stream prompt output");
    eprintln!("  --socket-path <p>   Override the Unix socket path");
    eprintln!("  --socket-name <n>   Override the abstract socket name");
    eprintln!("  --json              Emit raw JSON-RPC responses");
    eprintln!("  --timeout <ms>      IPC timeout in milliseconds");
    eprintln!("  --no-stream         Disable real-time streaming");
    eprintln!("  --usage           Show token usage");
    eprintln!("  --usage-baseline  JSON baseline for usage delta");
    eprintln!("  -h, --help        Show this help\n");
    eprintln!("Dashboard commands:");
    eprintln!("  tizenclaw-cli dashboard start [--port N]");
    eprintln!("                                   Start the web dashboard");
    eprintln!("  tizenclaw-cli dashboard stop    Stop the web dashboard");
    eprintln!("  tizenclaw-cli dashboard status  Show dashboard status\n");
    eprintln!("Agent data commands:");
    eprintln!("  tizenclaw-cli clear-data [--memory-only|--sessions-only] [--json]");
    eprintln!("                                   Clear persisted memory/session data\n");
    eprintln!("Registration commands:");
    eprintln!("  tizenclaw-cli register tool <path>");
    eprintln!("  tizenclaw-cli register skill <path>");
    eprintln!("  tizenclaw-cli unregister tool <path>");
    eprintln!("  tizenclaw-cli unregister skill <path>");
    eprintln!("  tizenclaw-cli list registrations\n");
    eprintln!("Task commands:");
    eprintln!("  tizenclaw-cli list tasks");
    eprintln!("  tizenclaw-cli devel status\n");
    eprintln!("LLM config commands:");
    eprintln!("  tizenclaw-cli config get [path]");
    eprintln!("  tizenclaw-cli config set <path> <value> [--strict-json]");
    eprintln!("  tizenclaw-cli config unset <path>");
    eprintln!("  tizenclaw-cli config reload\n");
    eprintln!("Setup commands:");
    eprintln!("  tizenclaw-cli setup         Interactive host setup wizard\n");
    eprintln!("Auth commands:");
    eprintln!("  tizenclaw-cli auth openai-codex status [--json]");
    eprintln!("  tizenclaw-cli auth openai-codex connect [--json]");
    eprintln!("  tizenclaw-cli auth openai-codex login [--json]\n");
    eprintln!("Inspection commands:");
    eprintln!("  tizenclaw-cli tools status");
    eprintln!("  tizenclaw-cli skills status\n");
    eprintln!("If no prompt given, starts interactive mode.");
}

fn parse_timeout(raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("Invalid timeout '{}'", raw))
}

fn parse_cli(args: &[String]) -> Result<ParsedCli, String> {
    let mut options = CliOptions::default();
    let mut usage_requested = false;
    let mut usage_baseline = None;
    let mut i = 0usize;

    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-s" | "--session" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("{} requires a value", args[i - 1]))?;
                options.session_id = Some(value.clone());
            }
            "--stream" => options.stream = true,
            "--no-stream" => options.stream = false,
            "--socket-path" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--socket-path requires a value".to_string())?;
                options.socket_path = Some(value.clone());
            }
            "--socket-name" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--socket-name requires a value".to_string())?;
                options.socket_name = Some(value.clone());
            }
            "--json" => options.json_output = true,
            "--timeout" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--timeout requires a value".to_string())?;
                options.timeout_ms = parse_timeout(value)?;
            }
            "--usage" => usage_requested = true,
            "--usage-baseline" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| "--usage-baseline requires a value".to_string())?;
                usage_baseline = Some(parse_usage_baseline(raw)?);
            }
            "dashboard" => {
                let action = args
                    .get(i + 1)
                    .ok_or_else(|| {
                        "Usage: tizenclaw-cli dashboard <start [--port N]|stop|status>".to_string()
                    })?
                    .clone();
                let mut port = None;
                let mut j = i + 2;
                while j < args.len() {
                    match args[j].as_str() {
                        "--port" => {
                            j += 1;
                            let value = args
                                .get(j)
                                .ok_or_else(|| "--port requires a value".to_string())?;
                            port =
                                Some(value.parse::<u16>().ok().filter(|v| *v > 0).ok_or_else(
                                    || format!("Invalid dashboard port '{}'", value),
                                )?);
                        }
                        other => return Err(format!("Unknown dashboard option '{}'", other)),
                    }
                    j += 1;
                }
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Dashboard { action, port },
                });
            }
            "register" => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Register {
                        kind: args
                            .get(i + 1)
                            .ok_or_else(|| {
                                "Usage: tizenclaw-cli register <tool|skill> <path>".to_string()
                            })?
                            .clone(),
                        path: args
                            .get(i + 2)
                            .ok_or_else(|| {
                                "Usage: tizenclaw-cli register <tool|skill> <path>".to_string()
                            })?
                            .clone(),
                    },
                });
            }
            "unregister" => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Unregister {
                        kind: args
                            .get(i + 1)
                            .ok_or_else(|| {
                                "Usage: tizenclaw-cli unregister <tool|skill> <path>".to_string()
                            })?
                            .clone(),
                        path: args
                            .get(i + 2)
                            .ok_or_else(|| {
                                "Usage: tizenclaw-cli unregister <tool|skill> <path>".to_string()
                            })?
                            .clone(),
                    },
                });
            }
            "list" => {
                let subject = args
                    .get(i + 1)
                    .ok_or_else(|| "Usage: tizenclaw-cli list <registrations|tasks>".to_string())?;
                let mode = match subject.as_str() {
                    "registrations" => CommandMode::ListRegistrations,
                    "tasks" => CommandMode::ListTasks,
                    _ => return Err("Usage: tizenclaw-cli list <registrations|tasks>".to_string()),
                };
                return Ok(ParsedCli { options, mode });
            }
            "devel" => {
                if args.get(i + 1).map(String::as_str) != Some("status") {
                    return Err("Usage: tizenclaw-cli devel status".to_string());
                }
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::DevelStatus,
                });
            }
            "tools" => {
                if args.get(i + 1).map(String::as_str) != Some("status") {
                    return Err("Usage: tizenclaw-cli tools status".to_string());
                }
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::ToolsStatus,
                });
            }
            "skills" => {
                if args.get(i + 1).map(String::as_str) != Some("status") {
                    return Err("Usage: tizenclaw-cli skills status".to_string());
                }
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::SkillsStatus,
                });
            }
            "config" => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Config(args[i + 1..].to_vec()),
                });
            }
            "clear-data" => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::ClearData(args[i + 1..].to_vec()),
                });
            }
            "auth" => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Auth(args[i + 1..].to_vec()),
                });
            }
            "setup" => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Setup,
                });
            }
            value if value.starts_with('-') => return Err(format!("Unknown option '{}'", value)),
            _ => {
                return Ok(ParsedCli {
                    options,
                    mode: CommandMode::Prompt(args[i..].join(" ")),
                });
            }
        }
        i += 1;
    }

    if usage_requested {
        Ok(ParsedCli {
            options,
            mode: CommandMode::Usage {
                baseline: usage_baseline,
            },
        })
    } else {
        Ok(ParsedCli {
            options,
            mode: CommandMode::Interactive,
        })
    }
}

fn cmd_register(client: &IpcClient, kind: &str, path: &str) {
    match client.register_path(kind, path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_unregister(client: &IpcClient, kind: &str, path: &str) {
    match client.unregister_path(kind, path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_list_registrations(client: &IpcClient) {
    match client.list_registered_paths() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_list_tasks(client: &IpcClient) {
    match client.list_tasks() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_devel_status(client: &IpcClient) {
    match client.get_devel_status() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_skill_status(client: &IpcClient) {
    match client.get_skill_capabilities() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_tool_status(client: &IpcClient) {
    match client.get_tool_audit() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse_cli(&args).unwrap_or_else(|err| print_error_and_exit(&err));

    match &parsed.mode {
        CommandMode::Auth(args) => {
            cmd_auth(args);
            return;
        }
        CommandMode::Setup => {
            cmd_setup();
            return;
        }
        _ => {}
    }

    let client = IpcClient::from_options(&parsed.options);

    match parsed.mode {
        CommandMode::Prompt(prompt) => {
            let session_id = parsed
                .options
                .session_id
                .clone()
                .unwrap_or_else(generate_session_id);
            let response = client
                .process_prompt(&session_id, &prompt, parsed.options.stream)
                .unwrap_or_else(|err| print_error_and_exit(&err));
            if parsed.options.json_output {
                println!(
                    "{}",
                    serde_json::to_string(&response.payload).unwrap_or_default()
                );
            } else if !response.stream_received {
                if let Some(text) = response.text.as_deref() {
                    println!("{}", text);
                }
            } else {
                println!();
            }
        }
        CommandMode::Interactive => {
            interactive_mode(
                &client,
                parsed.options.session_id.as_deref(),
                parsed.options.stream,
            );
        }
        CommandMode::Dashboard { action, port } => {
            let command = match port {
                Some(port) => format!("{} --port {}", action, port),
                None => action,
            };
            if parsed.options.json_output {
                let payload = dashboard_payload(&client, &command)
                    .unwrap_or_else(|err| print_error_and_exit(&err));
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            } else {
                let result = dashboard_value(&client, &command)
                    .unwrap_or_else(|err| print_error_and_exit(&err));
                print_json(&result);
            }
        }
        CommandMode::Register { kind, path } => cmd_register(&client, &kind, &path),
        CommandMode::Unregister { kind, path } => cmd_unregister(&client, &kind, &path),
        CommandMode::ListRegistrations => cmd_list_registrations(&client),
        CommandMode::ListTasks => cmd_list_tasks(&client),
        CommandMode::DevelStatus => cmd_devel_status(&client),
        CommandMode::ToolsStatus => cmd_tool_status(&client),
        CommandMode::SkillsStatus => cmd_skill_status(&client),
        CommandMode::Config(args) => cmd_config(&client, &args),
        CommandMode::ClearData(args) => cmd_clear_data(&client, &args),
        CommandMode::Usage { baseline } => {
            if parsed.options.json_output {
                let payload = usage_payload(
                    &client,
                    parsed.options.session_id.as_deref(),
                    baseline.as_ref(),
                )
                .unwrap_or_else(|err| print_error_and_exit(&err));
                println!("{}", serde_json::to_string(&payload).unwrap_or_default());
            } else {
                let result = usage_value(
                    &client,
                    parsed.options.session_id.as_deref(),
                    baseline.as_ref(),
                )
                .unwrap_or_else(|err| print_error_and_exit(&err));
                print_json(&result);
            }
        }
        CommandMode::Auth(_) | CommandMode::Setup => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        codex_auth_string, codex_oauth_snapshot, dashboard_port_from_doc, extract_ndjson_deltas,
        merge_missing, parse_chat_ids, parse_cli, parse_codex_login_state, reload_message,
        should_retry_reload_error, try_reload_llm_backends_with, CliOptions, CommandMode,
        IpcClient, RpcResponse, DEFAULT_PROMPT_TIMEOUT_MS, DEFAULT_TIMEOUT_MS,
        LONG_PROMPT_STREAM_THRESHOLD_CHARS,
    };
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn parse_chat_ids_accepts_comma_separated_ids() {
        assert_eq!(parse_chat_ids("123, 456,789").unwrap(), vec![123, 456, 789]);
    }

    #[test]
    fn parse_chat_ids_rejects_invalid_tokens() {
        assert!(parse_chat_ids("123, nope").is_err());
    }

    #[test]
    fn dashboard_port_from_doc_reads_web_dashboard_port() {
        let doc = json!({
            "channels": [
                {
                    "name": "web_dashboard",
                    "settings": {
                        "port": 9191
                    }
                }
            ]
        });
        assert_eq!(dashboard_port_from_doc(&doc), 9191);
    }

    #[test]
    fn parse_codex_login_state_detects_logged_in() {
        assert_eq!(
            parse_codex_login_state("Logged in using ChatGPT"),
            "logged_in"
        );
    }

    #[test]
    fn merge_missing_only_fills_absent_fields() {
        let mut doc = json!({
            "backends": {
                "openai-codex": {
                    "model": "custom-model"
                }
            }
        });
        let defaults = json!({
            "backends": {
                "openai-codex": {
                    "model": "gpt-5.4",
                    "transport": "responses"
                }
            }
        });
        merge_missing(&mut doc, &defaults);
        assert_eq!(
            doc["backends"]["openai-codex"]["model"],
            json!("custom-model")
        );
        assert_eq!(
            doc["backends"]["openai-codex"]["transport"],
            json!("responses")
        );
    }

    #[test]
    fn codex_oauth_snapshot_copies_tokens_and_auth_path() {
        let auth_doc = json!({
            "tokens": {
                "access_token": "access-token",
                "refresh_token": "refresh-token",
                "id_token": "id-token",
                "account_id": "acct-123"
            }
        });

        let snapshot = codex_oauth_snapshot(&auth_doc);

        assert_eq!(snapshot["access_token"], json!("access-token"));
        assert_eq!(snapshot["refresh_token"], json!("refresh-token"));
        assert_eq!(snapshot["id_token"], json!("id-token"));
        assert_eq!(snapshot["account_id"], json!("acct-123"));
        assert!(snapshot["auth_path"]
            .as_str()
            .map(|value| value.ends_with("/.codex/auth.json"))
            .unwrap_or(false));
    }

    #[test]
    fn codex_oauth_snapshot_accepts_flat_auth_shape() {
        let auth_doc = json!({
            "access_token": "flat-access-token",
            "refresh_token": "flat-refresh-token",
            "id_token": "flat-id-token",
            "account_id": "acct-flat"
        });

        let snapshot = codex_oauth_snapshot(&auth_doc);

        assert_eq!(snapshot["access_token"], json!("flat-access-token"));
        assert_eq!(snapshot["refresh_token"], json!("flat-refresh-token"));
        assert_eq!(snapshot["id_token"], json!("flat-id-token"));
        assert_eq!(snapshot["account_id"], json!("acct-flat"));
    }

    #[test]
    fn codex_oauth_snapshot_derives_expires_at_from_access_token() {
        let auth_doc = json!({
            "tokens": {
                "access_token": "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJleHAiOjQxMDI0NDQ4MDAsImh0dHBzOi8vYXBpLm9wZW5haS5jb20vYXV0aCI6eyJjaGF0Z3B0X2FjY291bnRfaWQiOiJhY2N0LWV4cCJ9fQ.sig",
                "refresh_token": "refresh-token"
            }
        });

        let snapshot = codex_oauth_snapshot(&auth_doc);

        assert_eq!(snapshot["expires_at"], json!(4102444800i64));
    }

    #[test]
    fn codex_auth_string_prefers_nested_tokens_but_falls_back_to_root() {
        let auth_doc = json!({
            "access_token": "root-access",
            "tokens": {
                "access_token": "nested-access"
            }
        });

        assert_eq!(
            codex_auth_string(&auth_doc, "access_token").as_deref(),
            Some("nested-access")
        );
        assert_eq!(
            codex_auth_string(&json!({"refresh_token": "root-refresh"}), "refresh_token")
                .as_deref(),
            Some("root-refresh")
        );
    }

    #[test]
    fn retry_reload_llm_backends_retries_connection_refused_errors() {
        let attempts = std::cell::Cell::new(0);
        let (reloaded, error) = try_reload_llm_backends_with(|| {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 3 {
                Err("Failed to connect to daemon. Is tizenclaw running? Connection refused (os error 111)".to_string())
            } else {
                Ok(())
            }
        });

        assert!(reloaded);
        assert!(error.is_none());
        assert_eq!(attempts.get(), 3);
    }

    #[test]
    fn reload_retry_detection_matches_connection_failures() {
        assert!(should_retry_reload_error(
            "Failed to connect to daemon. Is tizenclaw running? Connection refused (os error 111)"
        ));
        assert!(!should_retry_reload_error("Backend reload is not allowed"));
    }

    #[test]
    fn reload_message_explains_retryable_failures() {
        let message = reload_message(
            false,
            Some("Failed to connect to daemon. Is tizenclaw running? Connection refused (os error 111)"),
        );
        assert!(message.contains("restart or retry once"));
    }

    #[test]
    fn reload_message_explains_timeouts() {
        let message = reload_message(
            false,
            Some("Daemon backend reload timed out after 2 seconds"),
        );
        assert!(message.contains("did not finish in time"));
    }

    #[test]
    fn parse_cli_reads_prompt_mode_flags() {
        let parsed = parse_cli(&[
            "--session".to_string(),
            "my_session".to_string(),
            "--stream".to_string(),
            "--socket-path".to_string(),
            "/tmp/tizenclaw.sock".to_string(),
            "--socket-name".to_string(),
            "override.sock".to_string(),
            "--json".to_string(),
            "--timeout".to_string(),
            "1234".to_string(),
            "hello".to_string(),
            "world".to_string(),
        ])
        .unwrap();

        assert_eq!(parsed.options.session_id.as_deref(), Some("my_session"));
        assert!(parsed.options.stream);
        assert_eq!(
            parsed.options.socket_path.as_deref(),
            Some("/tmp/tizenclaw.sock")
        );
        assert_eq!(parsed.options.socket_name.as_deref(), Some("override.sock"));
        assert!(parsed.options.json_output);
        assert_eq!(parsed.options.timeout_ms, 1234);

        match parsed.mode {
            CommandMode::Prompt(prompt) => assert_eq!(prompt, "hello world"),
            mode => panic!("expected prompt mode, got {:?}", mode),
        }
    }

    #[test]
    fn parse_cli_reads_dashboard_subcommand() {
        let parsed = parse_cli(&[
            "--socket-name".to_string(),
            "cli.sock".to_string(),
            "dashboard".to_string(),
            "start".to_string(),
            "--port".to_string(),
            "9091".to_string(),
        ])
        .unwrap();

        assert_eq!(parsed.options.socket_name.as_deref(), Some("cli.sock"));

        match parsed.mode {
            CommandMode::Dashboard { action, port } => {
                assert_eq!(action, "start");
                assert_eq!(port, Some(9091));
            }
            mode => panic!("expected dashboard mode, got {:?}", mode),
        }
    }

    #[test]
    fn parse_cli_defaults_to_interactive_mode() {
        let parsed = parse_cli(&["--timeout".to_string(), "2500".to_string()]).unwrap();
        assert_eq!(parsed.options.timeout_ms, 2500);
        assert!(matches!(parsed.mode, CommandMode::Interactive));
    }

    #[test]
    fn process_prompt_uses_longer_timeout_floor() {
        let options = CliOptions::default();
        let client = IpcClient::from_options(&options);
        let prompt_client =
            client.with_timeout_floor(Duration::from_millis(DEFAULT_PROMPT_TIMEOUT_MS));

        assert_eq!(client.timeout, Duration::from_millis(DEFAULT_TIMEOUT_MS));
        assert_eq!(
            prompt_client.timeout,
            Duration::from_millis(DEFAULT_PROMPT_TIMEOUT_MS)
        );
    }

    #[test]
    fn process_prompt_preserves_explicitly_longer_timeout() {
        let options = CliOptions {
            timeout_ms: 900_000,
            ..CliOptions::default()
        };
        let client = IpcClient::from_options(&options);
        let prompt_client =
            client.with_timeout_floor(Duration::from_millis(DEFAULT_PROMPT_TIMEOUT_MS));

        assert_eq!(prompt_client.timeout, Duration::from_millis(900_000));
    }

    #[test]
    fn prompt_call_from_silent_stream_keeps_final_text_non_streaming() {
        let client = IpcClient::from_options(&CliOptions::default());
        let response = RpcResponse {
            payload: json!({
                "result": {
                    "text": "{\"total\":0.8}"
                }
            }),
            streamed_chunks: vec!["{\"tot".to_string(), "al\":0.8}".to_string()],
        };

        let prompt_call = client.prompt_call_from_stream_response(response, false);

        assert_eq!(prompt_call.text.as_deref(), Some("{\"total\":0.8}"));
        assert!(!prompt_call.stream_received);
    }

    #[test]
    fn long_prompt_threshold_is_large_enough_for_normal_cli_prompts() {
        assert!(LONG_PROMPT_STREAM_THRESHOLD_CHARS > 4000);
    }

    #[test]
    fn extract_ndjson_deltas_reads_delta_lines() {
        let raw = "{\"delta\":\"hel\"}\n{\"delta\":\"lo\"}\n";
        assert_eq!(extract_ndjson_deltas(raw), vec!["hel", "lo"]);
    }
}
