//! Web Dashboard — pure-Rust HTTP server channel.
//!
//! Replaces C++ libsoup-based web dashboard with Axum framework.
//! Serves the web UI and provides REST API endpoints for:
//! - Status, metrics, chat
//! - Sessions, tasks, logs (file-based storage)
//! - Auth (login, password change)
//! - Config management
//! - Apps listing, A2A agent card
//! - Static file serving

use super::{Channel, ChannelConfig};
use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::collections::{HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};

fn get_app_data_dir() -> String {
    std::env::var("TIZENCLAW_DATA_DIR")
        .unwrap_or_else(|_| "/opt/usr/share/tizenclaw".to_string())
}

fn get_share_dir() -> String {
    std::env::var("TIZENCLAW_SHARE_DIR")
        .unwrap_or_else(|_| "/opt/usr/share/tizenclaw".to_string())
}
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

#[derive(Clone)]
struct AppState {
    web_root: PathBuf,
    config_dir: PathBuf,
    admin_pw_hash: Arc<Mutex<String>>,
    active_tokens: Arc<Mutex<HashSet<String>>>,
}

pub struct WebDashboard {
    name: String,
    port: u16,
    localhost_only: bool,
    web_root: PathBuf,
    config_dir: PathBuf,
    running: Arc<AtomicBool>,
    rt: Option<std::thread::JoinHandle<()>>,
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
        let default_web_root = format!("{}/web", get_app_data_dir());
        let web_root = config.settings.get("web_root")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_web_root)
            .to_string();

        let config_dir = format!("{}/config", &get_share_dir());
        let _ = std::fs::create_dir_all(&config_dir);
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
            rt: None,
            admin_pw_hash: Arc::new(Mutex::new(pw_hash)),
            active_tokens: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl Channel for WebDashboard {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }
        if !self.web_root.is_dir() {
            log::warn!("WebDashboard: web root not found: {:?}", self.web_root);
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        let state = AppState {
            web_root: self.web_root.clone(),
            config_dir: self.config_dir.clone(),
            admin_pw_hash: self.admin_pw_hash.clone(),
            active_tokens: self.active_tokens.clone(),
        };

        let bind_addr = if self.localhost_only {
            format!("127.0.0.1:{}", self.port)
        } else {
            format!("0.0.0.0:{}", self.port)
        };
        let port = self.port;

        self.rt = Some(std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async move {
                let cors = CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any);

                let api_routes = Router::new()
                    .route("/.well-known/agent.json", get(api_agent_card))
                    .route("/api/status", get(api_status))
                    .route("/api/metrics", get(api_metrics))
                    .route("/api/chat", post(api_chat))
                    .route("/api/sessions/dates", get(api_session_dates))
                    .route("/api/sessions", get(api_sessions))
                    .route("/api/sessions/:id", get(api_session_detail))
                    .route("/api/tasks/dates", get(api_task_dates))
                    .route("/api/tasks", get(api_tasks))
                    .route("/api/tasks/:id", get(api_task_detail))
                    .route("/api/logs/dates", get(api_log_dates))
                    .route("/api/logs", get(api_logs))
                    .route("/api/auth/login", post(api_auth_login))
                    .route("/api/auth/change_password", post(api_auth_change_password))
                    .route("/api/config/list", get(api_config_list))
                    .route("/api/config/:name", get(api_config_get).post(api_config_set))
                    .route("/api/apps", get(api_apps_list))
                    .route("/api/apps/:id", get(api_app_detail))
                    .route("/api/a2a", post(api_a2a));

                let app = Router::new()
                    .nest_service("/apps", ServeDir::new(state.web_root.join("apps")))
                    .merge(api_routes)
                    .nest_service("/", ServeDir::new(&state.web_root))
                    .layer(cors)
                    .with_state(state);

                let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("WebDashboard failed to bind {}: {}", bind_addr, e);
                        return;
                    }
                };

                log::info!("WebDashboard started using Axum on {}", bind_addr);

                let serve_future = axum::serve(listener, app).with_graceful_shutdown(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
                    while running.load(Ordering::SeqCst) {
                        interval.tick().await;
                    }
                    log::info!("WebDashboard stopping (port {})", port);
                });

                if let Err(e) = serve_future.await {
                    log::error!("WebDashboard server error: {}", e);
                }
            });
        }));

        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.rt.take() {
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

// ─── Handlers ────────────────────────────────────────────────

fn json_error(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": msg})))
}

async fn validate_token(headers: &HeaderMap, state: &AppState) -> bool {
    headers.get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|token| state.active_tokens.lock().map(|t| t.contains(token)).unwrap_or(false))
        .unwrap_or(false)
}

async fn api_status() -> Json<Value> {
    Json(json!({
        "status": "running",
        "version": "1.0.0",
        "channels": "active"
    }))
}

async fn api_metrics() -> Json<Value> {
    let (rss_kb, vm_kb, threads) = parse_proc_status();
    let (load_1m, load_5m, load_15m) = parse_loadavg();
    let uptime_secs = get_process_uptime();
    let hours = uptime_secs as u64 / 3600;
    let minutes = (uptime_secs as u64 % 3600) / 60;
    let seconds = uptime_secs as u64 % 60;
    let formatted = format!("{}h {}m {}s", hours, minutes, seconds);
    let pid = std::process::id();

    Json(json!({
        "version": "1.0.0",
        "status": "running",
        "uptime": { "seconds": uptime_secs, "formatted": formatted },
        "counters": { "requests": 0, "errors": 0, "llm_calls": 0, "tool_calls": 0 },
        "memory": { "vm_rss_kb": rss_kb, "vm_size_kb": vm_kb },
        "cpu": { "load_1m": load_1m, "load_5m": load_5m, "load_15m": load_15m },
        "threads": threads,
        "pid": pid
    }))
}

async fn api_chat(Json(payload): Json<Value>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let prompt = payload.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let session_id = payload.get("session_id").and_then(|v| v.as_str()).unwrap_or("web_dashboard").to_string();

    if prompt.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "Empty prompt"));
    }

    let session_id_clone = session_id.clone();
    let prompt_clone = prompt.clone();

    let response = tokio::task::spawn_blocking(move || ipc_send_prompt(&session_id_clone, &prompt_clone))
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Tokio error: {}", e)))?;

    match response {
        Ok(response_text) => Ok(Json(json!({
            "status": "ok",
            "session_id": session_id,
            "response": response_text
        }))),
        Err(e) => {
            log::error!("Dashboard chat IPC error: {}", e);
            Err(json_error(StatusCode::BAD_GATEWAY, &format!("Agent error: {}", e)))
        }
    }
}

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
            addr.sun_path[1 + i] = *b as libc::c_char;
        }
        let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

        if libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
            libc::close(fd);
            return Err("Failed to connect to agent (is tizenclaw running?)".into());
        }

        // Removed SO_RCVTIMEO to support infinite wait for long-running LLM generation (No Timeout)
        // This ensures the frontend doesn't drop early while the core is thinking.

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

async fn api_sessions() -> Json<Value> {
    let sessions_dir = format!("{}/sessions", &get_share_dir());
    Json(Value::Array(list_md_files(&sessions_dir)))
}

async fn api_session_dates() -> Json<Value> {
    let sessions_dir = format!("{}/sessions", &get_share_dir());
    let mut dates = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.len() == 10 { dates.insert(name); }
            }
        }
    }
    Json(json!({"dates": dates.into_iter().collect::<Vec<_>>()}))
}

async fn api_session_detail(AxumPath(id): AxumPath<String>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if id.contains("..") || id.contains('/') {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid id"));
    }
    let parts: Vec<&str> = id.split('_').collect();
    let path = if parts.len() >= 2 && parts[0].len() == 10 {
        format!("{}/sessions/{}/{}.md", &get_share_dir(), parts[0], id)
    } else {
        format!("{}/sessions/{}.md", &get_share_dir(), id)
    };
    
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Json(json!({"id": id, "content": content}))),
        Err(_) => Err(json_error(StatusCode::NOT_FOUND, "Session not found")),
    }
}

async fn api_tasks() -> Json<Value> {
    let tasks_dir = format!("{}/tasks", &get_share_dir());
    Json(Value::Array(list_md_files(&tasks_dir)))
}

async fn api_task_dates() -> Json<Value> {
    let tasks_dir = format!("{}/tasks", &get_share_dir());
    Json(json!({"dates": collect_dates(&tasks_dir)}))
}

async fn api_task_detail(AxumPath(file): AxumPath<String>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if file.contains("..") || file.contains('/') {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid file"));
    }
    let fname = if file.ends_with(".md") { file.clone() } else { format!("{}.md", file) };
    let path = format!("{}/tasks/{}", &get_share_dir(), fname);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Json(json!({"file": fname, "content": content}))),
        Err(_) => Err(json_error(StatusCode::NOT_FOUND, "Task not found")),
    }
}

#[derive(serde::Deserialize)]
struct LogQuery { date: Option<String> }

async fn api_logs(Query(q): Query<LogQuery>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let date = q.date.unwrap_or_else(today_date_str);
    if date.len() != 10 || date.as_bytes().get(4) != Some(&b'-') || date.as_bytes().get(7) != Some(&b'-') {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid date format"));
    }
    let log_path = format!("{}/audit/{}.md", &get_share_dir(), date);
    let mut logs = vec![];
    if let Ok(content) = std::fs::read_to_string(&log_path) {
        logs.push(json!({"date": date, "content": content}));
    }
    Ok(Json(Value::Array(logs)))
}

async fn api_log_dates() -> Json<Value> {
    let audit_dir = format!("{}/audit", &get_share_dir());
    Json(json!({"dates": collect_dates(&audit_dir)}))
}

async fn api_auth_login(State(state): State<AppState>, Json(payload): Json<Value>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let password = payload.get("password").and_then(|v| v.as_str()).unwrap_or("");
    let hash = sha256_hex(password);
    let stored = state.admin_pw_hash.lock().map(|h| h.clone()).unwrap_or_default();

    if hash == stored {
        let token = generate_token();
        if let Ok(mut t) = state.active_tokens.lock() { t.insert(token.clone()); }
        log::debug!("Admin login successful");
        Ok(Json(json!({"status": "ok", "token": token})))
    } else {
        log::warn!("Admin login failed");
        Err(json_error(StatusCode::UNAUTHORIZED, "Invalid password"))
    }
}

async fn api_auth_change_password(headers: HeaderMap, State(state): State<AppState>, Json(payload): Json<Value>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await { return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized")); }
    let current = payload.get("current_password").and_then(|v| v.as_str()).unwrap_or("");
    let new_pw = payload.get("new_password").and_then(|v| v.as_str()).unwrap_or("");

    let stored = state.admin_pw_hash.lock().map(|h| h.clone()).unwrap_or_default();
    if sha256_hex(current) != stored { return Err(json_error(StatusCode::FORBIDDEN, "Current password incorrect")); }
    if new_pw.is_empty() { return Err(json_error(StatusCode::BAD_REQUEST, "New password empty")); }

    let new_hash = sha256_hex(new_pw);
    if let Ok(mut h) = state.admin_pw_hash.lock() { *h = new_hash.clone(); }
    let _ = std::fs::write(state.config_dir.join("admin_password.json"), json!({"password_hash": new_hash}).to_string());
    log::debug!("Admin password changed");
    Ok(Json(json!({"status": "ok"})))
}

async fn api_config_list(headers: HeaderMap, State(state): State<AppState>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await { return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized")); }
    let configs: Vec<Value> = ALLOWED_CONFIGS.iter().map(|name| {
        json!({"name": name, "exists": state.config_dir.join(name).exists()})
    }).collect();
    Ok(Json(json!({"status": "ok", "configs": configs})))
}

async fn api_config_get(headers: HeaderMap, State(state): State<AppState>, AxumPath(name): AxumPath<String>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await { return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized")); }
    if !ALLOWED_CONFIGS.contains(&name.as_str()) { return Err(json_error(StatusCode::FORBIDDEN, "Not allowed")); }

    let fpath = state.config_dir.join(&name);
    match std::fs::read_to_string(&fpath) {
        Ok(content) => Ok(Json(json!({"status":"ok","name":name,"content":content}))),
        Err(_) => {
            let sample = std::fs::read_to_string(state.config_dir.join(format!("{}.sample", name))).unwrap_or_default();
            Ok(Json(json!({"status":"not_found","name":name,"error":"Config not found","sample":sample})))
        }
    }
}

async fn api_config_set(headers: HeaderMap, State(state): State<AppState>, AxumPath(name): AxumPath<String>, Json(payload): Json<Value>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await { return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized")); }
    if !ALLOWED_CONFIGS.contains(&name.as_str()) { return Err(json_error(StatusCode::FORBIDDEN, "Not allowed")); }

    let content = payload.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let fpath = state.config_dir.join(&name);
    if let Some(parent) = fpath.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if fpath.exists() { let _ = std::fs::copy(&fpath, state.config_dir.join(format!("{}.bak", name))); }
    match std::fs::write(&fpath, content) {
        Ok(()) => {
            log::debug!("Config saved: {}", name);
            Ok(Json(json!({"status":"ok","name":name})))
        }
        Err(_) => Err(json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to write config")),
    }
}

async fn api_apps_list() -> Json<Value> {
    let mut apps = vec![];
    if let Ok(entries) = std::fs::read_dir(format!("{}/web/apps", &get_app_data_dir())) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let dirname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if dirname.starts_with('.') { continue; }

            let mut app = json!({"app_id": dirname, "url": format!("/apps/{}/", dirname)});
            if let Ok(manifest_str) = std::fs::read_to_string(path.join("manifest.json")) {
                if let Ok(manifest) = serde_json::from_str::<Value>(&manifest_str) {
                    app["title"] = manifest.get("title").cloned().unwrap_or(json!(dirname));
                }
            }
            apps.push(app);
        }
    }
    Json(Value::Array(apps))
}

async fn api_app_detail(AxumPath(app_id): AxumPath<String>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if app_id.contains("..") || app_id.contains('/') { return Err(json_error(StatusCode::BAD_REQUEST, "Invalid app_id")); }
    let path = PathBuf::from(format!("{}/web/apps/{}", &get_app_data_dir(), app_id));
    if !path.is_dir() { return Err(json_error(StatusCode::NOT_FOUND, "App not found")); }

    let mut app = json!({"app_id": app_id, "url": format!("/apps/{}/", app_id)});
    if let Ok(ms) = std::fs::read_to_string(path.join("manifest.json")) {
        if let Ok(manifest) = serde_json::from_str::<Value>(&ms) {
            if let Some(obj) = manifest.as_object() {
                for (k, v) in obj { app[k] = v.clone(); }
            }
        }
    }
    let mut files = vec![];
    if let Ok(entries) = std::fs::read_dir(&path) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                if let Some(name) = entry.file_name().to_str() { files.push(json!(name)); }
            }
        }
    }
    app["files"] = Value::Array(files);
    Ok(Json(app))
}

async fn api_agent_card() -> Json<Value> {
    Json(json!({
        "name": "TizenClaw Agent",
        "description": "TizenClaw AI Agent System for Tizen devices",
        "url": "http://localhost:9090",
        "version": "1.0.0",
        "protocol": "a2a",
        "protocolVersion": "0.1",
        "authentication": { "schemes": [{"scheme": "bearer"}] },
        "skills": [ {"id": "general", "name": "General Assistant"} ]
    }))
}

async fn api_a2a() -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    Ok(Json(json!({"error": "A2A not implemented natively yet"})))
}

// ─── Utility functions ───────────────────────────────────

fn sha256_hex(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
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

fn parse_proc_status() -> (i64, i64, i32) {
    let mut rss_kb = 0; let mut vm_kb = 0; let mut threads = 0;
    if let Ok(content) = std::fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("VmRSS:") { rss_kb = val.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0); }
            else if let Some(val) = line.strip_prefix("VmSize:") { vm_kb = val.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0); }
            else if let Some(val) = line.strip_prefix("Threads:") { threads = val.trim().parse().unwrap_or(0); }
        }
    }
    (rss_kb, vm_kb, threads)
}

fn parse_loadavg() -> (f64, f64, f64) {
    if let Ok(content) = std::fs::read_to_string("/proc/loadavg") {
        let parts: Vec<&str> = content.split_whitespace().collect();
        (parts.first().and_then(|s| s.parse().ok()).unwrap_or(0.0), parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0), parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0))
    } else { (0.0, 0.0, 0.0) }
}

fn get_process_uptime() -> f64 {
    let sys_uptime = std::fs::read_to_string("/proc/uptime").ok().and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())).unwrap_or(0.0);
    let proc_start = std::fs::read_to_string("/proc/self/stat").ok().and_then(|s| {
        let after_comm = s.rfind(')')?;
        s[after_comm + 2..].split_whitespace().nth(19).and_then(|v| v.parse::<f64>().ok())
    }).unwrap_or(0.0);
    let clk_tck = 100.0; let start_secs = proc_start / clk_tck;
    if sys_uptime > start_secs { sys_uptime - start_secs } else { 0.0 }
}

fn load_admin_password(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|j| j["password_hash"].as_str().map(|s| s.to_string()))
}

fn today_date_str() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let days = secs / 86400; let y = (days * 4 + 2) / 1461 + 1970;
    let mut doy = days - ((y - 1970) * 365 + (y - 1969) / 4);
    let leap = if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) { 1 } else { 0 };
    let months = [31, 28 + leap, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    for (i, &ml) in months.iter().enumerate() { if doy < ml { m = i as u64 + 1; break; } doy -= ml; }
    if m == 0 { m = 12; }
    format!("{:04}-{:02}-{:02}", y, m, doy + 1)
}

fn list_md_files(dir: &str) -> Vec<Value> {
    let mut entries = vec![];
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path(); 
            if path.is_dir() {
                if let Ok(sub) = std::fs::read_dir(&path) {
                    for s_entry in sub.flatten() {
                        let s_path = s_entry.path();
                        if !s_path.is_file() { continue; }
                        let name = s_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        if name.starts_with('.') || !name.ends_with(".md") { continue; }
                        entries.push(json!({"id": name.trim_end_matches(".md"), "file": name, "size_bytes": s_path.metadata().map(|m| m.len()).unwrap_or(0)}));
                    }
                }
            } else if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if name.starts_with('.') || !name.ends_with(".md") { continue; }
                entries.push(json!({"id": name.trim_end_matches(".md"), "file": name, "size_bytes": path.metadata().map(|m| m.len()).unwrap_or(0)}));
            }
        }
    }
    entries
}

fn collect_dates(dir: &str) -> Vec<String> {
    let mut dates = std::collections::BTreeSet::new();
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
