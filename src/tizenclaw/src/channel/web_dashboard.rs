//! Web Dashboard — pure-Rust HTTP server channel.
//!
//! Replaces C++ libsoup-based web dashboard with a TCP-based HTTP server.
//! Serves the web UI and provides REST API endpoints for:
//! - Status, metrics, chat
//! - Sessions, tasks, logs (file-based storage)
//! - Auth (login, password change)
//! - Config management
//! - Apps listing, A2A agent card
//! - Static file serving

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const APP_DATA_DIR: &str = "/opt/usr/data/tizenclaw";
const ALLOWED_CONFIGS: &[&str] = &[
    "llm_config.json",
    "telegram_config.json",
    "slack_config.json",
    "discord_config.json",
    "webhook_config.json",
    "tool_policy.json",
    "agent_roles.json",
    "tunnel_config.json",
    "web_search_config.json",
];

/// Simple HTTP request parsed from raw TCP.
struct HttpRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: String,
}

/// Simple HTTP response builder.
struct HttpResponse {
    status: u16,
    status_text: String,
    content_type: String,
    body: Vec<u8>,
    extra_headers: Vec<(String, String)>,
}

impl HttpResponse {
    fn json_ok(body: &Value) -> Self {
        let b = body.to_string();
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            content_type: "application/json".into(),
            body: b.into_bytes(),
            extra_headers: vec![],
        }
    }

    fn json_error(status: u16, msg: &str) -> Self {
        let b = json!({"error": msg}).to_string();
        let status_text = match status {
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            _ => "Error",
        };
        HttpResponse {
            status,
            status_text: status_text.into(),
            content_type: "application/json".into(),
            body: b.into_bytes(),
            extra_headers: vec![],
        }
    }

    fn static_file(content: Vec<u8>, content_type: &str) -> Self {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            content_type: content_type.into(),
            body: content,
            extra_headers: vec![
                ("Cache-Control".into(), "no-cache, no-store, must-revalidate".into()),
            ],
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut resp = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
             Access-Control-Allow-Headers: Content-Type, Authorization\r\n",
            self.status, self.status_text, self.content_type, self.body.len()
        );
        for (k, v) in &self.extra_headers {
            resp.push_str(&format!("{}: {}\r\n", k, v));
        }
        resp.push_str("\r\n");
        let mut bytes = resp.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

pub struct WebDashboard {
    name: String,
    port: u16,
    localhost_only: bool,
    web_root: PathBuf,
    config_dir: PathBuf,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    admin_pw_hash: Arc<Mutex<String>>,
    active_tokens: Arc<Mutex<HashSet<String>>>,
}

impl WebDashboard {
    pub fn new(config: &ChannelConfig) -> Self {
        let port = config.settings.get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(9090) as u16;
        let localhost_only = config.settings.get("localhost_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let web_root = config.settings.get("web_root")
            .and_then(|v| v.as_str())
            .unwrap_or("/opt/usr/share/tizenclaw/web")
            .to_string();

        let config_dir = format!("{}/config", APP_DATA_DIR);
        // Load admin password hash (default: sha256("admin"))
        let default_hash = sha256_hex("admin");
        let pw_hash = load_admin_password(&format!("{}/admin_password.json", config_dir))
            .unwrap_or(default_hash);

        WebDashboard {
            name: config.name.clone(),
            port,
            localhost_only,
            web_root: PathBuf::from(web_root),
            config_dir: PathBuf::from(config_dir),
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
            admin_pw_hash: Arc::new(Mutex::new(pw_hash)),
            active_tokens: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl Channel for WebDashboard {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }

        // Check web root
        if !self.web_root.is_dir() {
            log::warn!("WebDashboard: web root not found: {:?}", self.web_root);
        }

        let bind_addr = if self.localhost_only {
            format!("127.0.0.1:{}", self.port)
        } else {
            format!("0.0.0.0:{}", self.port)
        };

        let listener = match TcpListener::bind(&bind_addr) {
            Ok(l) => l,
            Err(e) => {
                log::error!("WebDashboard: failed to bind {}: {}", bind_addr, e);
                return false;
            }
        };

        if let Err(e) = listener.set_nonblocking(true) {
            log::error!("WebDashboard: set_nonblocking failed: {}", e);
            return false;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let web_root = self.web_root.clone();
        let config_dir = self.config_dir.clone();
        let admin_pw_hash = self.admin_pw_hash.clone();
        let active_tokens = self.active_tokens.clone();
        let port = self.port;

        self.thread = Some(std::thread::spawn(move || {
            log::info!("WebDashboard running on {}", bind_addr);
            while running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let web_root = web_root.clone();
                        let config_dir = config_dir.clone();
                        let admin_pw = admin_pw_hash.clone();
                        let tokens = active_tokens.clone();
                        let running_c = running.clone();
                        std::thread::spawn(move || {
                            handle_connection(
                                stream, &web_root, &config_dir,
                                &admin_pw, &tokens, &running_c,
                            );
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(e) => {
                        log::error!("WebDashboard accept error: {}", e);
                        break;
                    }
                }
            }
            log::info!("WebDashboard stopped (port {})", port);
        }));

        log::info!("WebDashboard started on port {}", self.port);
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }

    fn send_message(&self, _msg: &str) -> Result<(), String> {
        Ok(()) // Dashboard is pull-based
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// ─── Connection Handler ─────────────────────────────────────

fn handle_connection(
    mut stream: std::net::TcpStream,
    web_root: &Path,
    config_dir: &Path,
    admin_pw: &Mutex<String>,
    tokens: &Mutex<HashSet<String>>,
    _running: &AtomicBool,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));

    let request = match parse_request(&mut stream) {
        Some(r) => r,
        None => return,
    };

    // OPTIONS (CORS preflight)
    if request.method == "OPTIONS" {
        let resp = HttpResponse {
            status: 200, status_text: "OK".into(),
            content_type: "text/plain".into(), body: vec![],
            extra_headers: vec![],
        };
        let _ = stream.write_all(&resp.to_bytes());
        return;
    }

    let response = route_request(&request, web_root, config_dir, admin_pw, tokens);
    let _ = stream.write_all(&response.to_bytes());
}

fn parse_request(stream: &mut std::net::TcpStream) -> Option<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;

    let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
    if parts.len() < 2 { return None; }

    let method = parts[0].to_string();
    let full_path = parts[1].to_string();

    // Parse path and query
    let (path, query) = if let Some(idx) = full_path.find('?') {
        let p = full_path[..idx].to_string();
        let q = parse_query(&full_path[idx + 1..]);
        (p, q)
    } else {
        (full_path, HashMap::new())
    };

    // Parse headers
    let mut headers = HashMap::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 { break; }
        let trimmed = line.trim();
        if trimmed.is_empty() { break; }
        if let Some((key, val)) = trimmed.split_once(':') {
            let k = key.trim().to_lowercase();
            let v = val.trim().to_string();
            if k == "content-length" {
                content_length = v.parse().unwrap_or(0);
            }
            headers.insert(k, v);
        }
    }

    // Read body
    let mut body = vec![0u8; content_length.min(1_048_576)]; // 1MB max
    if content_length > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let body_str = String::from_utf8_lossy(&body).to_string();

    Some(HttpRequest { method, path, query, headers, body: body_str })
}

fn parse_query(qs: &str) -> HashMap<String, String> {
    qs.split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

// ─── Router ────────────────────────────────────────────────

fn route_request(
    req: &HttpRequest,
    web_root: &Path,
    config_dir: &Path,
    admin_pw: &Mutex<String>,
    tokens: &Mutex<HashSet<String>>,
) -> HttpResponse {
    let path = req.path.as_str();

    // A2A agent card
    if path == "/.well-known/agent.json" {
        return api_agent_card();
    }

    // API routes
    if let Some(api_path) = path.strip_prefix("/api/") {
        return handle_api(req, api_path, config_dir, admin_pw, tokens);
    }

    // Apps
    if path.starts_with("/apps/") {
        return serve_app_file(req, web_root);
    }

    // Static files
    serve_static(path, web_root)
}

fn handle_api(
    req: &HttpRequest,
    api_path: &str,
    config_dir: &Path,
    admin_pw: &Mutex<String>,
    tokens: &Mutex<HashSet<String>>,
) -> HttpResponse {
    match api_path {
        "status" => api_status(),
        "metrics" => api_metrics(),
        "chat" => api_chat(req),
        "sessions/dates" => api_session_dates(),
        "sessions" => api_sessions(),
        "tasks/dates" => api_task_dates(),
        "tasks" => api_tasks(),
        "logs/dates" => api_log_dates(),
        "logs" => api_logs(req),
        "auth/login" => api_auth_login(req, admin_pw, tokens),
        "auth/change_password" => api_auth_change_password(req, admin_pw, tokens, config_dir),
        "config/list" => api_config_list(req, tokens, config_dir),
        "apps" => api_apps_list(),
        "a2a" => api_a2a(req),
        _ => {
            // Dynamic sub-paths
            if let Some(id) = api_path.strip_prefix("sessions/") {
                return api_session_detail(id);
            }
            if let Some(file) = api_path.strip_prefix("tasks/") {
                return api_task_detail(file);
            }
            if let Some(name) = api_path.strip_prefix("config/") {
                if req.method == "POST" {
                    return api_config_set(req, name, tokens, config_dir);
                }
                return api_config_get(name, tokens, req, config_dir);
            }
            if let Some(app_id) = api_path.strip_prefix("apps/") {
                return api_app_detail(app_id);
            }
            HttpResponse::json_error(404, "Not found")
        }
    }
}

// ─── API endpoint implementations ────────

fn api_status() -> HttpResponse {
    HttpResponse::json_ok(&json!({
        "status": "running",
        "version": "1.0.0",
        "channels": "active"
    }))
}

fn api_metrics() -> HttpResponse {
    // Read memory info from /proc/self/status
    let (rss_kb, vm_kb, threads) = parse_proc_status();

    // Read CPU load from /proc/loadavg
    let (load_1m, load_5m, load_15m) = parse_loadavg();

    // Process uptime from /proc/self/stat
    let uptime_secs = get_process_uptime();
    let hours = uptime_secs as u64 / 3600;
    let minutes = (uptime_secs as u64 % 3600) / 60;
    let seconds = uptime_secs as u64 % 60;
    let formatted = format!("{}h {}m {}s", hours, minutes, seconds);

    // PID
    let pid = std::process::id();

    HttpResponse::json_ok(&json!({
        "version": "1.0.0",
        "status": "running",
        "uptime": {
            "seconds": uptime_secs,
            "formatted": formatted
        },
        "counters": {
            "requests": 0,
            "errors": 0,
            "llm_calls": 0,
            "tool_calls": 0
        },
        "memory": {
            "vm_rss_kb": rss_kb,
            "vm_size_kb": vm_kb
        },
        "cpu": {
            "load_1m": load_1m,
            "load_5m": load_5m,
            "load_15m": load_15m
        },
        "threads": threads,
        "pid": pid
    }))
}

fn api_chat(req: &HttpRequest) -> HttpResponse {
    if req.method != "POST" {
        return HttpResponse::json_error(405, "Method not allowed");
    }
    let payload: Value = serde_json::from_str(&req.body).unwrap_or(json!({}));
    let prompt = payload["prompt"].as_str().unwrap_or("");
    let session_id = payload["session_id"].as_str().unwrap_or("web_dashboard");

    if prompt.is_empty() {
        return HttpResponse::json_error(400, "Empty prompt");
    }

    // Forward to agent via IPC (abstract Unix domain socket)
    match ipc_send_prompt(session_id, prompt) {
        Ok(response_text) => HttpResponse::json_ok(&json!({
            "status": "ok",
            "session_id": session_id,
            "response": response_text
        })),
        Err(e) => {
            log::error!("Dashboard chat IPC error: {}", e);
            HttpResponse::json_error(502, &format!("Agent error: {}", e))
        }
    }
}

/// Connect to the daemon's abstract Unix domain socket and send a prompt.
fn ipc_send_prompt(session_id: &str, prompt: &str) -> Result<String, String> {
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err("Failed to create IPC socket".into());
        }

        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as u16;
        let name = b"tizenclaw.sock";
        for (i, b) in name.iter().enumerate() {
            addr.sun_path[1 + i] = *b as i8;
        }
        let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

        if libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
            libc::close(fd);
            return Err("Failed to connect to agent (is tizenclaw running?)".into());
        }

        // Set receive timeout (LLM can take a while)
        let timeout = libc::timeval { tv_sec: 30, tv_usec: 0 };
        libc::setsockopt(
            fd, libc::SOL_SOCKET, libc::SO_RCVTIMEO,
            &timeout as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );

        // Build JSON-RPC request
        let req = json!({
            "jsonrpc": "2.0",
            "method": "prompt",
            "id": 1,
            "params": {
                "session_id": session_id,
                "text": prompt
            }
        });
        let data = req.to_string();

        // Send 4-byte length prefix + payload
        let len_bytes = (data.len() as u32).to_be_bytes();
        if libc::write(fd, len_bytes.as_ptr() as *const _, 4) != 4 {
            libc::close(fd);
            return Err("Failed to send request length".into());
        }
        let mut sent: usize = 0;
        while sent < data.len() {
            let n = libc::write(fd, data.as_ptr().add(sent) as *const _, data.len() - sent);
            if n <= 0 { libc::close(fd); return Err("Failed to send request".into()); }
            sent += n as usize;
        }

        // Receive 4-byte length prefix
        let mut len_buf = [0u8; 4];
        if libc::recv(fd, len_buf.as_mut_ptr() as *mut _, 4, libc::MSG_WAITALL) != 4 {
            libc::close(fd);
            return Err("Failed to receive response (timeout?)".into());
        }
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        if resp_len == 0 || resp_len > 10 * 1024 * 1024 {
            libc::close(fd);
            return Err("Invalid response length".into());
        }

        // Receive payload
        let mut buf = vec![0u8; resp_len];
        let mut got: usize = 0;
        while got < resp_len {
            let n = libc::recv(fd, buf.as_mut_ptr().add(got) as *mut _, resp_len - got, 0);
            if n <= 0 { break; }
            got += n as usize;
        }
        libc::close(fd);

        let raw = String::from_utf8_lossy(&buf[..got]).to_string();
        let resp: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("Invalid JSON response: {}", e))?;

        if let Some(result) = resp.get("result") {
            if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
                return Ok(text.to_string());
            }
            return Ok(serde_json::to_string_pretty(result).unwrap_or_default());
        }
        if let Some(err) = resp.get("error") {
            let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
            return Err(msg.to_string());
        }
        Err("Unexpected response format".into())
    }
}

fn api_sessions() -> HttpResponse {
    let sessions_dir = format!("{}/sessions", APP_DATA_DIR);
    let entries = list_md_files(&sessions_dir);
    HttpResponse::json_ok(&Value::Array(entries))
}

fn api_session_dates() -> HttpResponse {
    let sessions_dir = format!("{}/sessions", APP_DATA_DIR);
    let dates = collect_dates(&sessions_dir);
    HttpResponse::json_ok(&json!({"dates": dates}))
}

fn api_session_detail(id: &str) -> HttpResponse {
    if id.contains("..") || id.contains('/') {
        return HttpResponse::json_error(400, "Invalid id");
    }
    let path = format!("{}/sessions/{}.md", APP_DATA_DIR, id);
    match std::fs::read_to_string(&path) {
        Ok(content) => HttpResponse::json_ok(&json!({"id": id, "content": content})),
        Err(_) => HttpResponse::json_error(404, "Session not found"),
    }
}

fn api_tasks() -> HttpResponse {
    let tasks_dir = format!("{}/tasks", APP_DATA_DIR);
    let entries = list_md_files(&tasks_dir);
    HttpResponse::json_ok(&Value::Array(entries))
}

fn api_task_dates() -> HttpResponse {
    let tasks_dir = format!("{}/tasks", APP_DATA_DIR);
    let dates = collect_dates(&tasks_dir);
    HttpResponse::json_ok(&json!({"dates": dates}))
}

fn api_task_detail(file: &str) -> HttpResponse {
    if file.contains("..") || file.contains('/') {
        return HttpResponse::json_error(400, "Invalid file");
    }
    let fname = if file.ends_with(".md") { file.to_string() } else { format!("{}.md", file) };
    let path = format!("{}/tasks/{}", APP_DATA_DIR, fname);
    match std::fs::read_to_string(&path) {
        Ok(content) => HttpResponse::json_ok(&json!({"file": fname, "content": content})),
        Err(_) => HttpResponse::json_error(404, "Task not found"),
    }
}

fn api_logs(req: &HttpRequest) -> HttpResponse {
    let date = req.query.get("date")
        .cloned()
        .unwrap_or_else(today_date_str);

    if date.len() != 10 || date.as_bytes().get(4) != Some(&b'-') || date.as_bytes().get(7) != Some(&b'-') {
        return HttpResponse::json_error(400, "Invalid date format");
    }

    let log_path = format!("{}/audit/{}.md", APP_DATA_DIR, date);
    let mut logs = vec![];
    if let Ok(content) = std::fs::read_to_string(&log_path) {
        logs.push(json!({"date": date, "content": content}));
    }
    HttpResponse::json_ok(&Value::Array(logs))
}

fn api_log_dates() -> HttpResponse {
    let audit_dir = format!("{}/audit", APP_DATA_DIR);
    let dates = collect_dates(&audit_dir);
    HttpResponse::json_ok(&json!({"dates": dates}))
}

fn api_auth_login(
    req: &HttpRequest,
    admin_pw: &Mutex<String>,
    tokens: &Mutex<HashSet<String>>,
) -> HttpResponse {
    if req.method != "POST" {
        return HttpResponse::json_error(405, "Method not allowed");
    }
    let payload: Value = serde_json::from_str(&req.body).unwrap_or(json!({}));
    let password = payload["password"].as_str().unwrap_or("");
    let hash = sha256_hex(password);

    let stored = admin_pw.lock().map(|h| h.clone()).unwrap_or_default();
    if hash == stored {
        let token = generate_token();
        if let Ok(mut t) = tokens.lock() {
            t.insert(token.clone());
        }
        log::info!("Admin login successful");
        HttpResponse::json_ok(&json!({"status": "ok", "token": token}))
    } else {
        log::warn!("Admin login failed");
        HttpResponse::json_error(401, "Invalid password")
    }
}

fn api_auth_change_password(
    req: &HttpRequest,
    admin_pw: &Mutex<String>,
    tokens: &Mutex<HashSet<String>>,
    config_dir: &Path,
) -> HttpResponse {
    if req.method != "POST" {
        return HttpResponse::json_error(405, "Method not allowed");
    }
    if !validate_token(req, tokens) {
        return HttpResponse::json_error(401, "Unauthorized");
    }

    let payload: Value = serde_json::from_str(&req.body).unwrap_or(json!({}));
    let current = payload["current_password"].as_str().unwrap_or("");
    let new_pw = payload["new_password"].as_str().unwrap_or("");

    let stored = admin_pw.lock().map(|h| h.clone()).unwrap_or_default();
    if sha256_hex(current) != stored {
        return HttpResponse::json_error(403, "Current password incorrect");
    }
    if new_pw.is_empty() {
        return HttpResponse::json_error(400, "New password empty");
    }

    let new_hash = sha256_hex(new_pw);
    if let Ok(mut h) = admin_pw.lock() {
        *h = new_hash.clone();
    }
    // Save to file
    let pw_file = config_dir.join("admin_password.json");
    let _ = std::fs::write(&pw_file, json!({"password_hash": new_hash}).to_string());
    log::info!("Admin password changed");
    HttpResponse::json_ok(&json!({"status": "ok"}))
}

fn api_config_list(
    req: &HttpRequest,
    tokens: &Mutex<HashSet<String>>,
    config_dir: &Path,
) -> HttpResponse {
    if !validate_token(req, tokens) {
        return HttpResponse::json_error(401, "Unauthorized");
    }
    let configs: Vec<Value> = ALLOWED_CONFIGS.iter().map(|name| {
        let exists = config_dir.join(name).exists();
        json!({"name": name, "exists": exists})
    }).collect();
    HttpResponse::json_ok(&json!({"status": "ok", "configs": configs}))
}

fn api_config_get(
    name: &str,
    tokens: &Mutex<HashSet<String>>,
    req: &HttpRequest,
    config_dir: &Path,
) -> HttpResponse {
    if !validate_token(req, tokens) {
        return HttpResponse::json_error(401, "Unauthorized");
    }
    if !ALLOWED_CONFIGS.contains(&name) {
        return HttpResponse::json_error(403, "Not allowed");
    }
    let fpath = config_dir.join(name);
    match std::fs::read_to_string(&fpath) {
        Ok(content) => HttpResponse::json_ok(&json!({"status":"ok","name":name,"content":content})),
        Err(_) => {
            let sample = std::fs::read_to_string(config_dir.join(format!("{}.sample", name)))
                .unwrap_or_default();
            HttpResponse::json_ok(&json!({"status":"not_found","name":name,"error":"Config not found","sample":sample}))
        }
    }
}

fn api_config_set(
    req: &HttpRequest,
    name: &str,
    tokens: &Mutex<HashSet<String>>,
    config_dir: &Path,
) -> HttpResponse {
    if !validate_token(req, tokens) {
        return HttpResponse::json_error(401, "Unauthorized");
    }
    if !ALLOWED_CONFIGS.contains(&name) {
        return HttpResponse::json_error(403, "Not allowed");
    }
    let payload: Value = serde_json::from_str(&req.body).unwrap_or(json!({}));
    let content = payload["content"].as_str().unwrap_or("");

    let fpath = config_dir.join(name);
    // Backup
    if fpath.exists() {
        let _ = std::fs::copy(&fpath, config_dir.join(format!("{}.bak", name)));
    }
    match std::fs::write(&fpath, content) {
        Ok(()) => {
            log::info!("Config saved: {}", name);
            HttpResponse::json_ok(&json!({"status":"ok","name":name}))
        }
        Err(_) => HttpResponse::json_error(500, "Failed to write config"),
    }
}

fn api_apps_list() -> HttpResponse {
    let apps_dir = format!("{}/web/apps", APP_DATA_DIR);
    let mut apps = vec![];
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let dirname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if dirname.starts_with('.') { continue; }

            let mut app = json!({"app_id": dirname, "url": format!("/apps/{}/", dirname)});
            let manifest_path = path.join("manifest.json");
            if let Ok(manifest_str) = std::fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_json::from_str::<Value>(&manifest_str) {
                    app["title"] = manifest.get("title").cloned().unwrap_or(json!(dirname));
                }
            }
            apps.push(app);
        }
    }
    HttpResponse::json_ok(&Value::Array(apps))
}

fn api_app_detail(app_id: &str) -> HttpResponse {
    if app_id.contains("..") || app_id.contains('/') {
        return HttpResponse::json_error(400, "Invalid app_id");
    }
    let app_dir = format!("{}/web/apps/{}", APP_DATA_DIR, app_id);
    let path = Path::new(&app_dir);
    if !path.is_dir() {
        return HttpResponse::json_error(404, "App not found");
    }

    let mut app = json!({"app_id": app_id, "url": format!("/apps/{}/", app_id)});
    if let Ok(manifest_str) = std::fs::read_to_string(path.join("manifest.json")) {
        if let Ok(manifest) = serde_json::from_str::<Value>(&manifest_str) {
            if let Some(obj) = manifest.as_object() {
                for (k, v) in obj {
                    app[k] = v.clone();
                }
            }
        }
    }

    let mut files = vec![];
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(json!(name));
                }
            }
        }
    }
    app["files"] = Value::Array(files);
    HttpResponse::json_ok(&app)
}

fn api_agent_card() -> HttpResponse {
    HttpResponse::json_ok(&json!({
        "name": "TizenClaw Agent",
        "description": "TizenClaw AI Agent System for Tizen devices",
        "url": "http://localhost:9090",
        "version": "1.0.0",
        "protocol": "a2a",
        "protocolVersion": "0.1",
        "capabilities": {
            "streaming": false,
            "pushNotifications": false,
            "stateTransitionHistory": false
        },
        "authentication": {
            "schemes": [{"scheme": "bearer", "description": "Bearer token authentication"}]
        },
        "defaultInputModes": ["text"],
        "defaultOutputModes": ["text"],
        "skills": [
            {"id": "general", "name": "General Assistant",
             "description": "General-purpose AI assistant for Tizen device management"},
            {"id": "device_control", "name": "Device Controller",
             "description": "Control and monitor Tizen devices"},
            {"id": "code_execution", "name": "Code Executor",
             "description": "Execute code in sandboxed containers"}
        ]
    }))
}

fn api_a2a(req: &HttpRequest) -> HttpResponse {
    if req.method != "POST" {
        return HttpResponse::json_error(405, "Method not allowed");
    }
    let request: Value = match serde_json::from_str(&req.body) {
        Ok(v) => v,
        Err(_) => {
            return HttpResponse::json_ok(&json!({
                "jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": "Parse error"}
            }));
        }
    };

    let method = request["method"].as_str().unwrap_or("");
    let id = request.get("id").cloned().unwrap_or(Value::Null);

    let result = match method {
        "tasks/send" => json!({"id": "a2a-stub", "status": "completed"}),
        "tasks/get" => json!({"error": "task not found"}),
        "tasks/cancel" => json!({"error": "task not found"}),
        _ => {
            return HttpResponse::json_ok(&json!({
                "jsonrpc": "2.0", "id": id,
                "error": {"code": -32601, "message": format!("Method not found: {}", method)}
            }));
        }
    };
    HttpResponse::json_ok(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

// ─── Static file serving ─────────────────────────────────

fn serve_static(path: &str, web_root: &Path) -> HttpResponse {
    let file_path = if path == "/" || path.is_empty() {
        web_root.join("index.html")
    } else {
        if path.contains("..") {
            return HttpResponse::json_error(403, "Forbidden");
        }
        web_root.join(path.trim_start_matches('/'))
    };

    match std::fs::read(&file_path) {
        Ok(content) => {
            let ct = mime_type(file_path.to_str().unwrap_or(""));
            HttpResponse::static_file(content, ct)
        }
        Err(_) => HttpResponse::json_error(404, "Not Found"),
    }
}

fn serve_app_file(req: &HttpRequest, web_root: &Path) -> HttpResponse {
    let rel = req.path.trim_start_matches('/');
    if rel.contains("..") {
        return HttpResponse::json_error(403, "Forbidden");
    }
    let file_path = web_root.join(rel);
    match std::fs::read(&file_path) {
        Ok(content) => {
            let ct = mime_type(file_path.to_str().unwrap_or(""));
            HttpResponse::static_file(content, ct)
        }
        Err(_) => HttpResponse::json_error(404, "Not Found"),
    }
}

fn mime_type(path: &str) -> &'static str {
    if path.ends_with(".html") { "text/html" }
    else if path.ends_with(".css") { "text/css" }
    else if path.ends_with(".js") { "application/javascript" }
    else if path.ends_with(".json") { "application/json" }
    else if path.ends_with(".png") { "image/png" }
    else if path.ends_with(".jpg") || path.ends_with(".jpeg") { "image/jpeg" }
    else if path.ends_with(".svg") { "image/svg+xml" }
    else if path.ends_with(".ico") { "image/x-icon" }
    else if path.ends_with(".woff2") { "font/woff2" }
    else if path.ends_with(".woff") { "font/woff" }
    else { "application/octet-stream" }
}

// ─── Utility functions ───────────────────────────────────

fn sha256_hex(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Simple hash for development; production should use ring/sha2
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    let h1 = hasher.finish();
    input.len().hash(&mut hasher);
    let h2 = hasher.finish();
    format!("{:016x}{:016x}", h1, h2)
}

fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:032x}", ts)
}

fn validate_token(req: &HttpRequest, tokens: &Mutex<HashSet<String>>) -> bool {
    let auth = match req.headers.get("authorization") {
        Some(a) => a,
        None => return false,
    };
    let token = match auth.strip_prefix("Bearer ") {
        Some(t) => t,
        None => return false,
    };
    tokens.lock().map(|t| t.contains(token)).unwrap_or(false)
}

/// Parse /proc/self/status for VmRSS, VmSize, Threads.
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

/// Parse /proc/loadavg for 1m, 5m, 15m load averages.
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

/// Get process uptime in seconds from /proc/self/stat and /proc/uptime.
fn get_process_uptime() -> f64 {
    // Read system uptime
    let sys_uptime = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0);

    // Read process start time (field 22 in /proc/self/stat, in clock ticks)
    let proc_start = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            // Fields after closing ')' of comm field
            let after_comm = s.rfind(')')? ;
            let rest = &s[after_comm + 2..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            // Field 22 is at index 19 after the 3 fields (state, ppid, pgrp...)
            // starttime is field 22 (1-indexed), which is index 19 after ')' skip
            fields.get(19).and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    // Clock ticks per second (typically 100 on Linux)
    let clk_tck: f64 = 100.0;
    let start_secs = proc_start / clk_tck;

    if sys_uptime > start_secs {
        sys_uptime - start_secs
    } else {
        0.0
    }
}

fn load_admin_password(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let j: Value = serde_json::from_str(&content).ok()?;
    j["password_hash"].as_str().map(|s| s.to_string())
}

fn today_date_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // UTC date — same approach as C++ TodayDateStr
    let days = secs / 86400;
    let y = (days * 4 + 2) / 1461 + 1970;
    let mut doy = days - ((y - 1970) * 365 + (y - 1969) / 4);
    let leap = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 1 } else { 0 };
    let months = [31, 28 + leap, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0u64;
    for (i, &ml) in months.iter().enumerate() {
        if doy < ml { m = i as u64 + 1; break; }
        doy -= ml;
    }
    if m == 0 { m = 12; }
    format!("{:04}-{:02}-{:02}", y, m, doy + 1)
}

fn list_md_files(dir: &str) -> Vec<Value> {
    let mut entries = vec![];
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if name.starts_with('.') || !name.ends_with(".md") { continue; }
            let id = name.trim_end_matches(".md");
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            entries.push(json!({"id": id, "file": name, "size_bytes": size}));
        }
    }
    entries
}

fn collect_dates(dir: &str) -> Vec<String> {
    let mut dates: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() == 13 && name.ends_with(".md") && name.as_bytes()[4] == b'-' && name.as_bytes()[7] == b'-' {
                dates.insert(name[..10].to_string());
            }
        }
    }
    dates.into_iter().collect()
}
