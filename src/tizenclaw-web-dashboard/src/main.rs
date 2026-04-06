//! tizenclaw-web-dashboard — standalone HTTP dashboard binary.
//!
//! Serves the TizenClaw web UI and REST API. Communicates with the main
//! tizenclaw daemon via IPC (Unix abstract socket) for chat requests.
//!
//! Usage:
//!   tizenclaw-web-dashboard [--port 8080] [--web-root PATH]
//!                           [--config-dir PATH] [--data-dir PATH]
//!                           [--localhost-only]

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware,
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};

static RUNNING: AtomicBool = AtomicBool::new(true);
static DASHBOARD_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

async fn add_no_cache_headers(response: Response) -> Response {
    let mut response = response;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert(header::EXPIRES, HeaderValue::from_static("0"));
    response
}

extern "C" fn signal_handler(_: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

// ─── Simple stderr logger ─────────────────────────────────────

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!(
            "[{}] [WEB-DASHBOARD] {}:{} — {}",
            record.level(),
            record.file().unwrap_or("?"),
            record.line().unwrap_or(0),
            record.args()
        );
    }
    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

fn default_data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("TIZENCLAW_DATA_DIR") {
        return PathBuf::from(path);
    }
    if is_tizen_runtime() {
        return PathBuf::from("/opt/usr/share/tizenclaw");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".tizenclaw")
}

fn is_tizen_runtime() -> bool {
    std::path::Path::new("/etc/tizen-release").exists()
        || std::path::Path::new("/opt/usr/share/tizenclaw").exists()
}

fn default_dashboard_port() -> u16 {
    if is_tizen_runtime() {
        9090
    } else {
        8080
    }
}

// ─── Config ───────────────────────────────────────────────────

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
const BRIDGE_RATE_LIMIT_PER_SECOND: usize = 10;

// ─── AppState ─────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    web_root: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    admin_pw_hash: Arc<Mutex<String>>,
    active_tokens: Arc<Mutex<HashSet<String>>>,
    bridge_rate: Arc<Mutex<HashMap<String, Vec<u64>>>>,
}

#[derive(Clone)]
struct SessionSummary {
    id: String,
    date: Option<String>,
    modified: u64,
    size_bytes: u64,
    message_count: usize,
    title: String,
    preview: String,
}

#[derive(Clone)]
struct TaskSummary {
    id: String,
    file: String,
    title: String,
    date: Option<String>,
    modified: u64,
    size_bytes: u64,
    preview: String,
}

#[derive(Clone)]
struct LogEntry {
    date: String,
    file: String,
    label: String,
    content: String,
}

#[derive(Clone, serde::Serialize)]
struct DashboardSessionMessage {
    role: String,
    text: String,
}

// ─── Main ─────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);

    unsafe {
        libc::signal(
            libc::SIGINT,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    let args: Vec<String> = std::env::args().collect();
    let mut port: u16 = default_dashboard_port();
    let mut localhost_only = false;
    let mut web_root_str = String::new();
    let mut config_dir_str = String::new();
    let mut data_dir_str = String::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(default_dashboard_port());
                i += 2;
            }
            "--web-root" if i + 1 < args.len() => {
                web_root_str = args[i + 1].clone();
                i += 2;
            }
            "--config-dir" if i + 1 < args.len() => {
                config_dir_str = args[i + 1].clone();
                i += 2;
            }
            "--data-dir" if i + 1 < args.len() => {
                data_dir_str = args[i + 1].clone();
                i += 2;
            }
            "--localhost-only" => {
                localhost_only = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Fall back to env vars / default paths
    let data_dir = if !data_dir_str.is_empty() {
        PathBuf::from(&data_dir_str)
    } else {
        default_data_dir()
    };
    let config_dir = if !config_dir_str.is_empty() {
        PathBuf::from(&config_dir_str)
    } else {
        data_dir.join("config")
    };
    let web_root = if !web_root_str.is_empty() {
        PathBuf::from(&web_root_str)
    } else {
        data_dir.join("web")
    };

    let _ = std::fs::create_dir_all(&config_dir);

    let default_hash = sha256_hex("admin");
    let pw_hash = load_admin_password(
        config_dir
            .join("admin_password.json")
            .to_str()
            .unwrap_or(""),
    )
    .unwrap_or(default_hash);

    let state = AppState {
        web_root: web_root.clone(),
        config_dir,
        data_dir,
        admin_pw_hash: Arc::new(Mutex::new(pw_hash)),
        active_tokens: Arc::new(Mutex::new(HashSet::new())),
        bridge_rate: Arc::new(Mutex::new(HashMap::new())),
    };

    let bind_addr = if localhost_only {
        format!("127.0.0.1:{}", port)
    } else {
        format!("0.0.0.0:{}", port)
    };

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
        .route(
            "/api/sessions",
            get(api_sessions).delete(api_sessions_delete),
        )
        .route(
            "/api/sessions/:id",
            get(api_session_detail).delete(api_session_delete),
        )
        .route("/api/tasks/dates", get(api_task_dates))
        .route("/api/tasks", get(api_tasks).delete(api_tasks_delete))
        .route(
            "/api/tasks/:id",
            get(api_task_detail).delete(api_task_delete),
        )
        .route("/api/logs/dates", get(api_log_dates))
        .route("/api/logs", get(api_logs))
        .route("/api/auth/login", post(api_auth_login))
        .route("/api/auth/logout", post(api_auth_logout))
        .route("/api/auth/session", get(api_auth_session))
        .route("/api/auth/change_password", post(api_auth_change_password))
        .route("/api/config/list", get(api_config_list))
        .route(
            "/api/config/:name",
            get(api_config_get).post(api_config_set),
        )
        .route("/api/apps", get(api_apps_list))
        .route("/api/apps/:id", get(api_app_detail).delete(api_app_delete))
        .route("/api/bridge/tool", post(api_bridge_tool))
        .route("/api/bridge/tools", get(api_bridge_tools))
        .route(
            "/api/bridge/data",
            get(api_bridge_data_get).post(api_bridge_data_post),
        )
        .route("/api/bridge/chat", post(api_bridge_chat))
        .route("/api/a2a", post(api_a2a));

    let app = Router::new()
        .nest_service("/apps", ServeDir::new(web_root.join("apps")))
        .merge(api_routes)
        .nest_service("/", ServeDir::new(&web_root))
        .layer(cors)
        .layer(middleware::map_response(add_no_cache_headers))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind {}: {}", bind_addr, e);
            std::process::exit(1);
        }
    };

    log::info!("WebDashboard listening on {}", bind_addr);

    let serve_future = axum::serve(listener, app).with_graceful_shutdown(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
        while RUNNING.load(Ordering::SeqCst) {
            interval.tick().await;
        }
        log::info!("WebDashboard shutting down");
    });

    if let Err(e) = serve_future.await {
        log::error!("Server error: {}", e);
    }
}

// ─── Handlers ─────────────────────────────────────────────────

fn json_error(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": msg})))
}

fn generate_session_id(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = DASHBOARD_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}_{}", prefix, ts, seq)
}

fn parse_session_markdown(content: &str) -> Vec<DashboardSessionMessage> {
    let mut messages = Vec::new();
    let mut current_role = String::new();
    let mut current_text: Vec<&str> = Vec::new();

    for line in content.lines() {
        if let Some(role_str) = line.strip_prefix("## ") {
            if !current_role.is_empty() {
                let text = current_text.join("\n").trim().to_string();
                if !text.is_empty() {
                    messages.push(DashboardSessionMessage {
                        role: current_role.clone(),
                        text,
                    });
                }
                current_text.clear();
            }
            current_role = role_str.trim().to_string();
        } else if !current_role.is_empty() && !line.starts_with("---") {
            current_text.push(line);
        }
    }

    if !current_role.is_empty() {
        let text = current_text.join("\n").trim().to_string();
        if !text.is_empty() {
            messages.push(DashboardSessionMessage {
                role: current_role,
                text,
            });
        }
    }

    messages
}

fn deduplicate_after_compacted(
    compacted: &[DashboardSessionMessage],
    today: &[DashboardSessionMessage],
) -> Vec<DashboardSessionMessage> {
    if compacted.is_empty() {
        return today.to_vec();
    }

    let compacted_set: std::collections::HashSet<(String, String)> = compacted
        .iter()
        .map(|msg| {
            (
                msg.role.clone(),
                msg.text.chars().take(100).collect::<String>(),
            )
        })
        .collect();

    today
        .iter()
        .filter(|msg| {
            let preview = msg.text.chars().take(100).collect::<String>();
            !compacted_set.contains(&(msg.role.clone(), preview))
        })
        .cloned()
        .collect()
}

fn load_session_messages(session_dir: &std::path::Path) -> Vec<DashboardSessionMessage> {
    let compacted_path = session_dir.join("compacted.md");
    let compacted = std::fs::read_to_string(&compacted_path)
        .ok()
        .map(|content| parse_session_markdown(&content))
        .unwrap_or_default();

    let mut day_files: Vec<_> = std::fs::read_dir(session_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "md")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name != "compacted.md")
                    .unwrap_or(false)
        })
        .collect();
    day_files.sort();

    if compacted.is_empty() {
        let mut all = Vec::new();
        for path in day_files {
            if let Ok(content) = std::fs::read_to_string(path) {
                all.extend(parse_session_markdown(&content));
            }
        }
        return all;
    }

    let today = day_files
        .last()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|content| parse_session_markdown(&content))
        .unwrap_or_default();

    let mut merged = compacted.clone();
    merged.extend(deduplicate_after_compacted(&compacted, &today));
    merged
}

fn render_session_markdown(messages: &[DashboardSessionMessage]) -> String {
    let mut out = String::new();
    for message in messages {
        out.push_str(&format!("## {}\n{}\n\n", message.role, message.text));
    }
    out.trim_end().to_string()
}

fn session_display_title(messages: &[DashboardSessionMessage], fallback: &str) -> String {
    messages
        .iter()
        .find(|msg| !msg.text.trim().is_empty() && msg.role == "user")
        .or_else(|| messages.iter().find(|msg| !msg.text.trim().is_empty()))
        .map(|msg| first_line_preview(&msg.text, 48))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn collect_session_summaries(data_dir: &std::path::Path) -> Vec<SessionSummary> {
    let sessions_dir = data_dir.join("sessions");
    let mut sessions = Vec::new();

    let entries = match std::fs::read_dir(&sessions_dir) {
        Ok(entries) => entries,
        Err(_) => return sessions,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if id.starts_with('.') || id.is_empty() {
            continue;
        }

        let messages = load_session_messages(&path);
        let title = session_display_title(&messages, &id);
        let preview = messages
            .iter()
            .find(|msg| !msg.text.trim().is_empty())
            .map(|msg| snippet_preview(&msg.text, 120))
            .unwrap_or_default();

        let mut modified = 0u64;
        let mut size_bytes = 0u64;
        let mut latest_date: Option<String> = None;

        if let Ok(files) = std::fs::read_dir(&path) {
            for file in files.flatten() {
                let file_path = file.path();
                if !file_path.is_file() {
                    continue;
                }

                if let Ok(meta) = file.metadata() {
                    size_bytes += meta.len();
                    if let Ok(mtime) = meta.modified() {
                        if let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH) {
                            modified = modified.max(duration.as_secs());
                        }
                    }
                }

                if let Some(name) = file_path.file_name().and_then(|name| name.to_str()) {
                    if name.len() >= 10 && name.as_bytes().get(4) == Some(&b'-') {
                        latest_date = Some(
                            latest_date
                                .map(|current| current.max(name[..10].to_string()))
                                .unwrap_or_else(|| name[..10].to_string()),
                        );
                    }
                }
            }
        }

        sessions.push(SessionSummary {
            id,
            date: latest_date,
            modified,
            size_bytes,
            message_count: messages.len(),
            title,
            preview,
        });
    }

    sessions.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| left.id.cmp(&right.id))
    });
    sessions
}

fn first_line_preview(text: &str, max_chars: usize) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    truncate_chars(line, max_chars)
}

fn snippet_preview(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&normalized, max_chars)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

async fn validate_token(headers: &HeaderMap, state: &AppState) -> bool {
    let stored = state
        .admin_pw_hash
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();

    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|token| {
            let in_memory = state
                .active_tokens
                .lock()
                .map(|t| t.contains(token))
                .unwrap_or(false);
            in_memory || validate_auth_token(token, &stored)
        })
        .unwrap_or(false)
}

async fn api_status() -> Json<Value> {
    Json(json!({"status": "running", "version": "1.0.0", "channels": "active"}))
}

async fn api_metrics() -> Json<Value> {
    let (rss_kb, vm_kb, threads) = parse_proc_status();
    let (load_1m, load_5m, load_15m) = parse_loadavg();
    let uptime_secs = get_process_uptime();
    let hours = uptime_secs as u64 / 3600;
    let minutes = (uptime_secs as u64 % 3600) / 60;
    let seconds = uptime_secs as u64 % 60;

    // Query live usage counters from the agent daemon via IPC.
    let usage = tokio::task::spawn_blocking(ipc_get_usage)
        .await
        .ok()
        .flatten();
    let agent_connected = usage.is_some();
    let llm_calls = usage
        .as_ref()
        .and_then(|u| u["total_requests"].as_i64())
        .unwrap_or(0);
    let prompt_tokens = usage
        .as_ref()
        .and_then(|u| u["prompt_tokens"].as_i64())
        .unwrap_or(0);
    let completion_tokens = usage
        .as_ref()
        .and_then(|u| u["completion_tokens"].as_i64())
        .unwrap_or(0);
    let cache_read_tokens = usage
        .as_ref()
        .and_then(|u| u["cache_read_input_tokens"].as_i64())
        .unwrap_or(0);
    let cache_write_tokens = usage
        .as_ref()
        .and_then(|u| u["cache_creation_input_tokens"].as_i64())
        .unwrap_or(0);

    Json(json!({
        "version": "1.0.0",
        "status": if agent_connected { "running" } else { "disconnected" },
        "agent_connected": agent_connected,
        "uptime": {
            "seconds": uptime_secs,
            "formatted": format!("{}h {}m {}s", hours, minutes, seconds)
        },
        "counters": {
            "requests": 0,
            "errors": 0,
            "llm_calls": llm_calls,
            "tool_calls": 0
        },
        "tokens": {
            "prompt": prompt_tokens,
            "completion": completion_tokens,
            "cache_read": cache_read_tokens,
            "cache_write": cache_write_tokens,
            "total": prompt_tokens + completion_tokens
        },
        "memory": {"vm_rss_kb": rss_kb, "vm_size_kb": vm_kb},
        "cpu": {"load_1m": load_1m, "load_5m": load_5m, "load_15m": load_15m},
        "threads": threads,
        "pid": std::process::id()
    }))
}

async fn api_chat(Json(payload): Json<Value>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let prompt = payload
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let requested_session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let session_id = if requested_session_id.trim().is_empty() {
        generate_session_id("web")
    } else {
        requested_session_id.trim().to_string()
    };
    if prompt.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "Empty prompt"));
    }
    let sid = session_id.clone();
    let p = prompt.clone();
    let response = tokio::task::spawn_blocking(move || ipc_send_prompt(&sid, &p))
        .await
        .map_err(|e| json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    match response {
        Ok(text) => Ok(Json(
            json!({"status": "ok", "session_id": session_id, "response": text}),
        )),
        Err(e) => Err(json_error(
            StatusCode::BAD_GATEWAY,
            &format!("Agent error: {}", e),
        )),
    }
}

async fn api_sessions(State(state): State<AppState>) -> Json<Value> {
    Json(Value::Array(
        collect_session_summaries(&state.data_dir)
            .into_iter()
            .map(|session| {
                json!({
                    "id": session.id,
                    "title": session.title,
                    "date": session.date,
                    "modified": session.modified,
                    "size_bytes": session.size_bytes,
                    "message_count": session.message_count,
                    "content_preview": session.preview
                })
            })
            .collect(),
    ))
}

async fn api_session_dates(State(state): State<AppState>) -> Json<Value> {
    let mut dates = std::collections::BTreeSet::new();
    for session in collect_session_summaries(&state.data_dir) {
        if let Some(date) = session.date {
            dates.insert(date);
        }
    }
    Json(json!({"dates": dates.into_iter().collect::<Vec<_>>()}))
}

#[derive(serde::Deserialize)]
struct SessionDeletePayload {
    ids: Vec<String>,
}

async fn api_session_detail(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if id.contains("..") || id.contains('/') {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid id"));
    }
    let path = state.data_dir.join("sessions").join(&id);
    if !path.is_dir() {
        return Err(json_error(StatusCode::NOT_FOUND, "Session not found"));
    }

    let messages = load_session_messages(&path);
    let content = render_session_markdown(&messages);

    Ok(Json(json!({
        "id": id,
        "content": content,
        "messages": messages
    })))
}

async fn api_session_delete(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    delete_session_dirs(&state.data_dir, &[id])
}

async fn api_sessions_delete(
    State(state): State<AppState>,
    Json(payload): Json<SessionDeletePayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    delete_session_dirs(&state.data_dir, &payload.ids)
}

async fn api_tasks(State(state): State<AppState>) -> Json<Value> {
    let dir = state.data_dir.join("tasks");
    Json(Value::Array(
        collect_task_summaries(&dir)
            .into_iter()
            .map(|task| {
                json!({
                    "id": task.id,
                    "file": task.file,
                    "title": task.title,
                    "date": task.date,
                    "modified": task.modified,
                    "size_bytes": task.size_bytes,
                    "content_preview": task.preview
                })
            })
            .collect(),
    ))
}

async fn api_task_dates(State(state): State<AppState>) -> Json<Value> {
    let dir = state.data_dir.join("tasks");
    let dates = collect_task_summaries(&dir)
        .into_iter()
        .filter_map(|task| task.date)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    Json(json!({"dates": dates}))
}

async fn api_task_detail(
    State(state): State<AppState>,
    AxumPath(file): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if file.contains("..") || file.contains('/') {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid file"));
    }
    let fname = if file.ends_with(".md") {
        file.clone()
    } else {
        format!("{}.md", file)
    };
    let path = state.data_dir.join("tasks").join(&fname);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Json(json!({"file": fname, "content": content}))),
        Err(_) => Err(json_error(StatusCode::NOT_FOUND, "Task not found")),
    }
}

async fn api_task_delete(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    delete_task_files(&state.data_dir, &[id])
}

async fn api_tasks_delete(
    State(state): State<AppState>,
    Json(payload): Json<SessionDeletePayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    delete_task_files(&state.data_dir, &payload.ids)
}

#[derive(serde::Deserialize)]
struct LogQuery {
    date: Option<String>,
}

async fn api_logs(
    State(state): State<AppState>,
    Query(q): Query<LogQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let date = q.date.unwrap_or_else(today_date_str);
    if !is_valid_date(&date) {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid date format"));
    }

    let logs = collect_logs_for_date(&state.data_dir.join("logs"), &date)
        .into_iter()
        .map(|entry| {
            json!({
                "date": entry.date,
                "file": entry.file,
                "label": entry.label,
                "content": entry.content
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(Value::Array(logs)))
}

async fn api_log_dates(State(state): State<AppState>) -> Json<Value> {
    let dir = state.data_dir.join("logs");
    Json(json!({"dates": collect_log_dates(&dir)}))
}

async fn api_auth_login(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let password = payload
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let hash = sha256_hex(password);
    let stored = state
        .admin_pw_hash
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();
    if hash == stored {
        let token = generate_auth_token(&stored);
        if let Ok(mut t) = state.active_tokens.lock() {
            t.insert(token.clone());
        }
        Ok(Json(json!({"status": "ok", "token": token})))
    } else {
        Err(json_error(StatusCode::UNAUTHORIZED, "Invalid password"))
    }
}

async fn api_auth_logout(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string();

    if token.is_empty() {
        return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
    }

    if let Ok(mut tokens) = state.active_tokens.lock() {
        tokens.remove(&token);
    }

    Ok(Json(json!({"status": "ok"})))
}

async fn api_auth_session(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await {
        return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
    }

    Ok(Json(json!({
        "status": "ok",
        "authenticated": true,
    })))
}

async fn api_auth_change_password(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await {
        return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
    }
    let current = payload
        .get("current_password")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_pw = payload
        .get("new_password")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stored = state
        .admin_pw_hash
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();
    if sha256_hex(current) != stored {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "Current password incorrect",
        ));
    }
    if new_pw.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "New password empty"));
    }
    let new_hash = sha256_hex(new_pw);
    if let Ok(mut h) = state.admin_pw_hash.lock() {
        *h = new_hash.clone();
    }
    let _ = std::fs::write(
        state.config_dir.join("admin_password.json"),
        json!({"password_hash": new_hash}).to_string(),
    );
    Ok(Json(json!({"status": "ok"})))
}

async fn api_config_list(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await {
        return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
    }
    let configs: Vec<Value> = ALLOWED_CONFIGS
        .iter()
        .map(|name| json!({"name": name, "exists": state.config_dir.join(name).exists()}))
        .collect();
    Ok(Json(json!({"status": "ok", "configs": configs})))
}

async fn api_config_get(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await {
        return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
    }
    if !ALLOWED_CONFIGS.contains(&name.as_str()) {
        return Err(json_error(StatusCode::FORBIDDEN, "Not allowed"));
    }
    let fpath = state.config_dir.join(&name);
    match std::fs::read_to_string(&fpath) {
        Ok(content) => Ok(Json(
            json!({"status": "ok", "name": name, "content": content}),
        )),
        Err(_) => {
            let sample = std::fs::read_to_string(state.config_dir.join(format!("{}.sample", name)))
                .unwrap_or_default();
            Ok(Json(
                json!({"status": "not_found", "name": name, "error": "Config not found", "sample": sample}),
            ))
        }
    }
}

async fn api_config_set(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(name): AxumPath<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !validate_token(&headers, &state).await {
        return Err(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
    }
    if !ALLOWED_CONFIGS.contains(&name.as_str()) {
        return Err(json_error(StatusCode::FORBIDDEN, "Not allowed"));
    }
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let fpath = state.config_dir.join(&name);
    if fpath.exists() {
        let _ = std::fs::copy(&fpath, state.config_dir.join(format!("{}.bak", name)));
    }
    match std::fs::write(&fpath, content) {
        Ok(()) => Ok(Json(json!({"status": "ok", "name": name}))),
        Err(_) => Err(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to write config",
        )),
    }
}

fn is_safe_app_id(app_id: &str) -> bool {
    !app_id.is_empty() && !app_id.contains("..") && !app_id.contains('/')
}

fn is_safe_app_key(key: &str) -> bool {
    !key.is_empty() && !key.contains("..") && !key.contains('/')
}

fn load_app_allowed_tools(web_root: &std::path::Path, app_id: &str) -> Vec<String> {
    if !is_safe_app_id(app_id) {
        return Vec::new();
    }
    let manifest_path = web_root.join("apps").join(app_id).join("manifest.json");
    let Ok(content) = std::fs::read_to_string(manifest_path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    manifest
        .get("allowed_tools")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|item| item.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn check_bridge_rate_limit(state: &AppState, app_id: &str) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let cutoff = now_ms.saturating_sub(1000);
    let Ok(mut guard) = state.bridge_rate.lock() else {
        return false;
    };
    let timestamps = guard.entry(app_id.to_string()).or_default();
    timestamps.retain(|ts| *ts >= cutoff);
    if timestamps.len() >= BRIDGE_RATE_LIMIT_PER_SECOND {
        return false;
    }
    timestamps.push(now_ms);
    true
}

#[derive(serde::Deserialize)]
struct BridgeToolsQuery {
    app_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct BridgeDataQuery {
    app_id: String,
    key: String,
}

#[derive(serde::Deserialize)]
struct BridgeToolPayload {
    app_id: String,
    tool_name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(serde::Deserialize)]
struct BridgeDataPayload {
    app_id: String,
    key: String,
    value: Value,
}

#[derive(serde::Deserialize)]
struct BridgeChatPayload {
    app_id: Option<String>,
    prompt: String,
}

async fn api_apps_list(State(state): State<AppState>) -> Json<Value> {
    let apps_dir = state.web_root.join("apps");
    let mut apps = vec![];
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dirname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if dirname.starts_with('.') {
                continue;
            }
            let mut app = json!({"app_id": dirname, "url": format!("/apps/{}/", dirname)});
            if let Ok(ms) = std::fs::read_to_string(path.join("manifest.json")) {
                if let Ok(manifest) = serde_json::from_str::<Value>(&ms) {
                    app["title"] = manifest.get("title").cloned().unwrap_or(json!(dirname));
                    app["created_at"] = manifest.get("created_at").cloned().unwrap_or(Value::Null);
                    app["has_css"] = manifest.get("has_css").cloned().unwrap_or(json!(false));
                    app["has_js"] = manifest.get("has_js").cloned().unwrap_or(json!(false));
                }
            }
            apps.push(app);
        }
    }
    Json(Value::Array(apps))
}

async fn api_app_detail(
    State(state): State<AppState>,
    AxumPath(app_id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !is_safe_app_id(&app_id) {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid app_id"));
    }
    let path = state.web_root.join("apps").join(&app_id);
    if !path.is_dir() {
        return Err(json_error(StatusCode::NOT_FOUND, "App not found"));
    }
    let mut app = json!({"app_id": app_id, "url": format!("/apps/{}/", app_id)});
    if let Ok(ms) = std::fs::read_to_string(path.join("manifest.json")) {
        if let Ok(manifest) = serde_json::from_str::<Value>(&ms) {
            if let Some(obj) = manifest.as_object() {
                for (k, v) in obj {
                    app[k] = v.clone();
                }
            }
        }
    }
    let mut files = vec![];
    if let Ok(entries) = std::fs::read_dir(&path) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(json!(name));
                }
            }
        }
    }
    app["files"] = Value::Array(files);
    Ok(Json(app))
}

async fn api_app_delete(
    State(state): State<AppState>,
    AxumPath(app_id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !is_safe_app_id(&app_id) {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid app_id"));
    }
    let path = state.web_root.join("apps").join(&app_id);
    if !path.is_dir() {
        return Err(json_error(StatusCode::NOT_FOUND, "App not found"));
    }
    std::fs::remove_dir_all(&path)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    Ok(Json(json!({"status": "deleted", "app_id": app_id})))
}

async fn api_bridge_tool(
    State(state): State<AppState>,
    Json(payload): Json<BridgeToolPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !is_safe_app_id(&payload.app_id) || payload.tool_name.trim().is_empty() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            "app_id and tool_name are required",
        ));
    }
    if !check_bridge_rate_limit(&state, &payload.app_id) {
        return Err(json_error(StatusCode::FORBIDDEN, "Rate limit exceeded"));
    }
    let allowed_tools = load_app_allowed_tools(&state.web_root, &payload.app_id);
    if allowed_tools.is_empty() {
        return Err(json_error(
            StatusCode::FORBIDDEN,
            "No bridge tools are allowed for this app",
        ));
    }
    let tool_name = payload.tool_name.clone();
    let arguments = payload.arguments.clone();
    let result =
        tokio::task::spawn_blocking(move || ipc_bridge_tool(&tool_name, arguments, allowed_tools))
            .await
            .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    match result {
        Ok(value) => {
            if value.get("error").is_some() {
                Ok(Json(
                    json!({"status": "error", "error": value["error"].clone()}),
                ))
            } else {
                Ok(Json(json!({"status": "ok", "result": value})))
            }
        }
        Err(err) => Err(json_error(
            StatusCode::BAD_GATEWAY,
            &format!("Agent error: {}", err),
        )),
    }
}

async fn api_bridge_tools(
    State(state): State<AppState>,
    Query(query): Query<BridgeToolsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let allowed_tools = query
        .app_id
        .as_deref()
        .map(|app_id| load_app_allowed_tools(&state.web_root, app_id))
        .unwrap_or_default();
    if query.app_id.is_some() && allowed_tools.is_empty() {
        return Ok(Json(json!({"tools": [], "count": 0})));
    }
    let result = tokio::task::spawn_blocking(move || ipc_bridge_list_tools(allowed_tools))
        .await
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    match result {
        Ok(value) => {
            let count = value
                .get("tools")
                .and_then(|tools| tools.as_array())
                .map(|tools| tools.len())
                .unwrap_or(0);
            Ok(Json(json!({
                "tools": value.get("tools").cloned().unwrap_or_else(|| Value::Array(vec![])),
                "count": count
            })))
        }
        Err(err) => Err(json_error(
            StatusCode::BAD_GATEWAY,
            &format!("Agent error: {}", err),
        )),
    }
}

async fn api_bridge_data_get(
    State(state): State<AppState>,
    Query(query): Query<BridgeDataQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !is_safe_app_id(&query.app_id) || !is_safe_app_key(&query.key) {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid parameters"));
    }
    let path = state
        .web_root
        .join("apps")
        .join(&query.app_id)
        .join("data")
        .join(format!("{}.json", query.key));
    if !path.exists() {
        return Ok(Json(json!({"key": query.key, "value": Value::Null})));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let value = serde_json::from_str::<Value>(&content).unwrap_or(Value::String(content));
    Ok(Json(json!({"key": query.key, "value": value})))
}

async fn api_bridge_data_post(
    State(state): State<AppState>,
    Json(payload): Json<BridgeDataPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !is_safe_app_id(&payload.app_id) || !is_safe_app_key(&payload.key) {
        return Err(json_error(StatusCode::BAD_REQUEST, "Invalid parameters"));
    }
    let data_dir = state
        .web_root
        .join("apps")
        .join(&payload.app_id)
        .join("data");
    std::fs::create_dir_all(&data_dir)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let encoded = serde_json::to_vec(&payload.value)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    std::fs::write(data_dir.join(format!("{}.json", payload.key)), encoded)
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    Ok(Json(json!({"status": "ok", "key": payload.key})))
}

async fn api_bridge_chat(
    Json(payload): Json<BridgeChatPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let prompt = payload.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(json_error(StatusCode::BAD_REQUEST, "prompt required"));
    }
    let app_id = payload
        .app_id
        .filter(|value| is_safe_app_id(value))
        .unwrap_or_else(|| "anonymous".to_string());
    let session_id = format!("webapp_{}", app_id);
    let response = tokio::task::spawn_blocking(move || ipc_send_prompt(&session_id, &prompt))
        .await
        .map_err(|err| json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    match response {
        Ok(text) => Ok(Json(json!({"status": "ok", "response": text}))),
        Err(err) => Err(json_error(
            StatusCode::BAD_GATEWAY,
            &format!("Agent error: {}", err),
        )),
    }
}

async fn api_agent_card() -> Json<Value> {
    Json(json!({
        "name": "TizenClaw Agent",
        "description": "TizenClaw AI Agent System for Tizen devices",
        "url": "http://localhost:9090",
        "version": "1.0.0",
        "protocol": "a2a",
        "protocolVersion": "0.1",
        "authentication": {"schemes": [{"scheme": "bearer"}]},
        "skills": [{"id": "general", "name": "General Assistant"}]
    }))
}

async fn api_a2a() -> Json<Value> {
    Json(json!({"error": "A2A not implemented natively yet"}))
}

// ─── IPC helper ───────────────────────────────────────────────

/// Query live token-usage counters from the agent daemon.
/// Returns `None` when the agent is unreachable or returns an error.
fn ipc_get_usage() -> Option<Value> {
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return None;
        }

        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as u16;
        let name = b"tizenclaw.sock";
        for (i, b) in name.iter().enumerate() {
            addr.sun_path[1 + i] = *b as libc::c_char;
        }
        let addr_len =
            (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

        // Short timeout: don't block the metrics endpoint.
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
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_SNDTIMEO,
            &timeout as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        );

        if libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
            libc::close(fd);
            return None;
        }

        let req = json!({"jsonrpc": "2.0", "method": "get_usage", "id": 1, "params": {}});
        let data = req.to_string();
        let len_bytes = (data.len() as u32).to_be_bytes();
        if libc::write(fd, len_bytes.as_ptr() as *const _, 4) != 4 {
            libc::close(fd);
            return None;
        }
        let mut sent = 0usize;
        while sent < data.len() {
            let n = libc::write(fd, data.as_ptr().add(sent) as *const _, data.len() - sent);
            if n <= 0 {
                libc::close(fd);
                return None;
            }
            sent += n as usize;
        }

        let mut len_buf = [0u8; 4];
        if libc::recv(fd, len_buf.as_mut_ptr() as *mut _, 4, libc::MSG_WAITALL) != 4 {
            libc::close(fd);
            return None;
        }
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        if resp_len == 0 || resp_len > 1024 * 1024 {
            libc::close(fd);
            return None;
        }
        let mut buf = vec![0u8; resp_len];
        let mut got = 0usize;
        while got < resp_len {
            let n = libc::recv(fd, buf.as_mut_ptr().add(got) as *mut _, resp_len - got, 0);
            if n <= 0 {
                break;
            }
            got += n as usize;
        }
        libc::close(fd);

        let raw = String::from_utf8_lossy(&buf[..got]).to_string();
        let resp: Value = serde_json::from_str(&raw).ok()?;
        resp.get("result").cloned()
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
        let addr_len =
            (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

        if libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
            libc::close(fd);
            return Err("Failed to connect to agent".into());
        }

        let req = json!({
            "jsonrpc": "2.0", "method": "prompt", "id": 1,
            "params": {"session_id": session_id, "text": prompt}
        });
        let data = req.to_string();
        let len_bytes = (data.len() as u32).to_be_bytes();
        if libc::write(fd, len_bytes.as_ptr() as *const _, 4) != 4 {
            libc::close(fd);
            return Err("Failed to send request length".into());
        }
        let mut sent = 0usize;
        while sent < data.len() {
            let n = libc::write(fd, data.as_ptr().add(sent) as *const _, data.len() - sent);
            if n <= 0 {
                libc::close(fd);
                return Err("Failed to send request".into());
            }
            sent += n as usize;
        }

        let mut len_buf = [0u8; 4];
        if libc::recv(fd, len_buf.as_mut_ptr() as *mut _, 4, libc::MSG_WAITALL) != 4 {
            libc::close(fd);
            return Err("Failed to receive response".into());
        }
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        if resp_len == 0 || resp_len > 10 * 1024 * 1024 {
            libc::close(fd);
            return Err("Invalid response length".into());
        }
        let mut buf = vec![0u8; resp_len];
        let mut got = 0usize;
        while got < resp_len {
            let n = libc::recv(fd, buf.as_mut_ptr().add(got) as *mut _, resp_len - got, 0);
            if n <= 0 {
                break;
            }
            got += n as usize;
        }
        libc::close(fd);

        let raw = String::from_utf8_lossy(&buf[..got]).to_string();
        let resp: Value = serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {}", e))?;
        if let Some(result) = resp.get("result") {
            if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
                return Ok(text.to_string());
            }
            return Ok(serde_json::to_string_pretty(result).unwrap_or_default());
        }
        if let Some(err) = resp.get("error") {
            return Err(err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string());
        }
        Err("Unexpected response format".into())
    }
}

fn ipc_call(method: &str, params: Value) -> Result<Value, String> {
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
        let addr_len =
            (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

        if libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
            libc::close(fd);
            return Err("Failed to connect to agent".into());
        }

        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "id": 1,
            "params": params
        });
        let data = req.to_string();
        let len_bytes = (data.len() as u32).to_be_bytes();
        if libc::write(fd, len_bytes.as_ptr() as *const _, 4) != 4 {
            libc::close(fd);
            return Err("Failed to send request length".into());
        }

        let mut sent = 0usize;
        while sent < data.len() {
            let n = libc::write(fd, data.as_ptr().add(sent) as *const _, data.len() - sent);
            if n <= 0 {
                libc::close(fd);
                return Err("Failed to send request".into());
            }
            sent += n as usize;
        }

        let mut len_buf = [0u8; 4];
        if libc::recv(fd, len_buf.as_mut_ptr() as *mut _, 4, libc::MSG_WAITALL) != 4 {
            libc::close(fd);
            return Err("Failed to receive response".into());
        }
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        if resp_len == 0 || resp_len > 10 * 1024 * 1024 {
            libc::close(fd);
            return Err("Invalid response length".into());
        }
        let mut buf = vec![0u8; resp_len];
        let mut got = 0usize;
        while got < resp_len {
            let n = libc::recv(fd, buf.as_mut_ptr().add(got) as *mut _, resp_len - got, 0);
            if n <= 0 {
                break;
            }
            got += n as usize;
        }
        libc::close(fd);

        let raw = String::from_utf8_lossy(&buf[..got]).to_string();
        let resp: Value = serde_json::from_str(&raw).map_err(|e| format!("Invalid JSON: {}", e))?;
        if let Some(result) = resp.get("result") {
            return Ok(result.clone());
        }
        if let Some(err) = resp.get("error") {
            return Err(err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string());
        }
        Err("Unexpected response format".into())
    }
}

fn ipc_bridge_tool(
    tool_name: &str,
    args: Value,
    allowed_tools: Vec<String>,
) -> Result<Value, String> {
    ipc_call(
        "bridge_tool",
        json!({
            "tool_name": tool_name,
            "args": args,
            "allowed_tools": allowed_tools,
        }),
    )
}

fn ipc_bridge_list_tools(allowed_tools: Vec<String>) -> Result<Value, String> {
    ipc_call(
        "bridge_list_tools",
        json!({
            "allowed_tools": allowed_tools,
        }),
    )
}

// ─── Utility ──────────────────────────────────────────────────

fn sha256_hex(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    input.hash(&mut h);
    let h1 = h.finish();
    input.len().hash(&mut h);
    let h2 = h.finish();
    format!("{:016x}{:016x}", h1, h2)
}

fn generate_auth_token(password_hash: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let expires_at = ts + 60 * 60 * 12;
    let signature = sha256_hex(&format!("{}:{}", password_hash, expires_at));
    format!("v1.{}.{}", expires_at, signature)
}

fn validate_auth_token(token: &str, password_hash: &str) -> bool {
    let mut parts = token.split('.');
    let version = parts.next().unwrap_or("");
    let expires_at = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let signature = parts.next().unwrap_or("");

    if version != "v1" || signature.is_empty() || parts.next().is_some() {
        return false;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if expires_at <= now {
        return false;
    }

    let expected = sha256_hex(&format!("{}:{}", password_hash, expires_at));
    signature == expected
}

fn load_admin_password(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|j| j["password_hash"].as_str().map(|s| s.to_string()))
}

fn parse_proc_status() -> (i64, i64, i32) {
    let mut rss_kb = 0i64;
    let mut vm_kb = 0i64;
    let mut threads = 0i32;
    if let Ok(content) = std::fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("VmRSS:") {
                rss_kb = v
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("VmSize:") {
                vm_kb = v
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("Threads:") {
                threads = v.trim().parse().unwrap_or(0);
            }
        }
    }
    (rss_kb, vm_kb, threads)
}

fn parse_loadavg() -> (f64, f64, f64) {
    if let Ok(s) = std::fs::read_to_string("/proc/loadavg") {
        let p: Vec<&str> = s.split_whitespace().collect();
        return (
            p.first().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            p.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0),
            p.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0),
        );
    }
    (0.0, 0.0, 0.0)
}

fn get_process_uptime() -> f64 {
    let sys = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    let start = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            let after_comm = s.rfind(')')?;
            s[after_comm + 2..]
                .split_whitespace()
                .nth(19)
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    let start_secs = start / 100.0;
    if sys > start_secs {
        sys - start_secs
    } else {
        0.0
    }
}

fn today_date_str() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now, &mut tm_buf);
    }
    format!(
        "{:04}-{:02}-{:02}",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday
    )
}

fn delete_session_dirs(
    data_dir: &std::path::Path,
    ids: &[String],
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut deleted = Vec::new();

    for id in ids {
        if id.contains("..") || id.contains('/') || id.trim().is_empty() {
            return Err(json_error(StatusCode::BAD_REQUEST, "Invalid session id"));
        }

        let path = data_dir.join("sessions").join(id);
        if path.is_dir() && std::fs::remove_dir_all(&path).is_ok() {
            deleted.push(id.clone());
        }
    }

    Ok(Json(json!({"status": "ok", "deleted_ids": deleted})))
}

fn collect_task_summaries(dir: &std::path::Path) -> Vec<TaskSummary> {
    let mut entries = Vec::new();
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(_) => return entries,
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file()
            || path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext != "md")
                .unwrap_or(true)
        {
            continue;
        }

        let file = entry.file_name().to_string_lossy().to_string();
        if file.starts_with('.') {
            continue;
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let (title, preview) = parse_task_markdown_summary(&file, &content);
        let date = extract_date_prefix(&file);
        let modified = path
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let size_bytes = path.metadata().map(|meta| meta.len()).unwrap_or(0);

        entries.push(TaskSummary {
            id: file.trim_end_matches(".md").to_string(),
            file,
            title,
            date,
            modified,
            size_bytes,
            preview,
        });
    }

    entries.sort_by(|left, right| right.modified.cmp(&left.modified));
    entries
}

fn delete_task_files(
    data_dir: &std::path::Path,
    ids: &[String],
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut deleted = Vec::new();
    for id in ids {
        if id.contains("..") || id.contains('/') || id.trim().is_empty() {
            return Err(json_error(StatusCode::BAD_REQUEST, "Invalid task id"));
        }

        let file_name = if id.ends_with(".md") {
            id.clone()
        } else {
            format!("{}.md", id)
        };
        let path = data_dir.join("tasks").join(&file_name);
        if path.is_file() && std::fs::remove_file(&path).is_ok() {
            deleted.push(file_name.trim_end_matches(".md").to_string());
        }
    }

    Ok(Json(json!({"status": "ok", "deleted_ids": deleted})))
}

fn parse_task_markdown_summary(file: &str, content: &str) -> (String, String) {
    let mut title = file.trim_end_matches(".md").to_string();
    let mut preview_lines = Vec::new();
    let mut in_frontmatter = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }

        if in_frontmatter {
            if let Some((key, value)) = trimmed.split_once(':') {
                if key.trim() == "name" {
                    title = value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                }
            }
            continue;
        }

        if !trimmed.is_empty() {
            preview_lines.push(trimmed);
        }
    }

    let preview = truncate_chars(&preview_lines.join(" "), 120);
    (title, preview)
}

fn extract_date_prefix(name: &str) -> Option<String> {
    if name.len() >= 10 {
        let prefix = &name[..10];
        if is_valid_date(prefix) {
            return Some(prefix.to_string());
        }
    }
    None
}

fn is_valid_date(date: &str) -> bool {
    date.len() == 10
        && date.as_bytes().get(4) == Some(&b'-')
        && date.as_bytes().get(7) == Some(&b'-')
        && date
            .chars()
            .enumerate()
            .all(|(idx, ch)| idx == 4 || idx == 7 || ch.is_ascii_digit())
}

fn collect_log_dates(logs_dir: &std::path::Path) -> Vec<String> {
    let mut dates = std::collections::BTreeSet::new();

    let years = match std::fs::read_dir(logs_dir) {
        Ok(years) => years,
        Err(_) => return Vec::new(),
    };

    for year in years.flatten() {
        let year_name = year.file_name().to_string_lossy().to_string();
        if year_name.len() != 4 || !year_name.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }

        if let Ok(months) = std::fs::read_dir(year.path()) {
            for month in months.flatten() {
                let month_name = month.file_name().to_string_lossy().to_string();
                if month_name.len() != 2 || !month_name.chars().all(|ch| ch.is_ascii_digit()) {
                    continue;
                }

                if let Ok(days) = std::fs::read_dir(month.path()) {
                    for day in days.flatten() {
                        let day_name = day.file_name().to_string_lossy().to_string();
                        if day_name.len() != 2 || !day_name.chars().all(|ch| ch.is_ascii_digit()) {
                            continue;
                        }

                        let date = format!("{}-{}-{}", year_name, month_name, day_name);
                        if is_valid_date(&date) {
                            dates.insert(date);
                        }
                    }
                }
            }
        }
    }

    dates.into_iter().collect()
}

fn collect_logs_for_date(logs_dir: &std::path::Path, date: &str) -> Vec<LogEntry> {
    let day_dir = logs_dir.join(date.replace('-', "/"));
    let mut entries = Vec::new();
    let read_dir = match std::fs::read_dir(day_dir) {
        Ok(read_dir) => read_dir,
        Err(_) => return entries,
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file()
            || path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext != "log")
                .unwrap_or(true)
        {
            continue;
        }

        let file = entry.file_name().to_string_lossy().to_string();
        let label = file.trim_end_matches(".log").to_string();
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        entries.push(LogEntry {
            date: date.to_string(),
            file,
            label,
            content,
        });
    }

    entries.sort_by(|left, right| left.file.cmp(&right.file));
    entries
}
