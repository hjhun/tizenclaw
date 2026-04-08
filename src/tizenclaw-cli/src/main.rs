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
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use tizenclaw::api::TizenClaw;

static CLI_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn create_client() -> Result<TizenClaw, String> {
    let mut client = TizenClaw::new();
    client.initialize()?;
    Ok(client)
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn print_error_and_exit(error: &str) -> ! {
    eprintln!("Error: {}", error);
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
    Path::new("/etc/tizen-release").exists() || Path::new("/opt/usr/share/tizenclaw").exists()
}

fn setup_data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("TIZENCLAW_DATA_DIR") {
        return PathBuf::from(path);
    }
    if is_tizen_runtime() {
        return PathBuf::from("/opt/usr/share/tizenclaw");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".tizenclaw")
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
                "temperature": 0.7,
                "max_tokens": 4096
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
                "api_path": "/responses",
                "service_tier": "priority",
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
                "temperature": 0.7,
                "max_tokens": 4096
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

fn codex_oauth_snapshot(auth_doc: &Value) -> Value {
    let tokens = auth_doc
        .get("tokens")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut oauth = Map::new();

    for key in ["access_token", "refresh_token", "id_token", "account_id"] {
        if let Some(value) = tokens.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                oauth.insert(key.to_string(), Value::String(trimmed.to_string()));
            }
        }
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
        let account_id = doc
            .get("tokens")
            .and_then(|value| value.get("account_id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
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

fn try_reload_llm_backends() -> (bool, Option<String>) {
    match create_client() {
        Ok(client) => match client.reload_llm_backends() {
            Ok(_) => (true, None),
            Err(err) => (false, Some(err)),
        },
        Err(err) => (false, Some(err)),
    }
}

fn connect_codex_session() -> Result<Value, String> {
    let status = codex_login_status();
    if status
        .get("codex_login_state")
        .and_then(Value::as_str)
        != Some("logged_in")
    {
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
        doc["backends"]["openai-codex"] =
            default_llm_config()["backends"]["openai-codex"].clone();
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

    Ok(json!({
        "status": "ok",
        "provider": "openai-codex",
        "linked": true,
        "config_path": config_path.display().to_string(),
        "active_backend": "openai-codex",
        "reloaded": reloaded,
        "reload_error": reload_error,
        "dashboard_url": dashboard_url(),
        "message": if reloaded {
            "TizenClaw is now linked to the Codex CLI session and the daemon reloaded the backend."
        } else {
            "TizenClaw is now linked to the Codex CLI session. Start or reload the daemon to activate it."
        }
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

fn show_usage(client: &TizenClaw, session_id: Option<&str>, baseline: Option<&Value>) {
    match client.get_usage(session_id, baseline) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn send_prompt(
    client: &TizenClaw,
    session_id: &str,
    prompt: &str,
    stream: bool,
) -> Result<String, String> {
    let response = if stream {
        client.process_prompt_streaming(session_id, prompt, |chunk| {
            print!("{}", chunk);
            io::stdout().flush().ok();
        })?
    } else {
        let text = client.process_prompt(session_id, prompt)?;
        tizenclaw::api::PromptResponse {
            session_id: session_id.to_string(),
            text,
            stream_received: false,
        }
    };

    if !response.stream_received {
        println!("{}", response.text);
    } else {
        println!();
    }

    Ok(response.session_id)
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
fn cmd_dashboard(client: &TizenClaw, command: &str) {
    let (action, port) = parse_dashboard_command(command);

    match action.as_str() {
        "start" => match client.start_dashboard(port) {
            Ok(_) => {
                if let Some(port) = port {
                    println!("Dashboard started on port {}.", port);
                } else {
                    println!("Dashboard started.");
                }
            }
            Err(error) => print_error_and_exit(&error),
        },
        "stop" => match client.stop_dashboard() {
            Ok(_) => println!("Dashboard stopped."),
            Err(error) => print_error_and_exit(&error),
        },
        "status" => match client.dashboard_status() {
            Ok(result) => {
                let running = result
                    .get("running")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                println!("Dashboard: {}", if running { "running" } else { "stopped" });
            }
            Err(error) => print_error_and_exit(&error),
        },
        _ => {
            eprintln!(
                "Unknown dashboard action '{}'. Use: start [--port N] | stop | status",
                action
            );
            std::process::exit(1);
        }
    }
}

/// Interactive REPL mode.
fn interactive_mode(client: &TizenClaw, explicit_session_id: Option<&str>, stream: bool) {
    match explicit_session_id {
        Some(session_id) => println!("TizenClaw Interactive CLI (session: {})", session_id),
        None => println!("TizenClaw Interactive CLI (new session per prompt)"),
    }
    println!("Type 'quit' or 'exit' to leave. Type '/help' for commands.\n");

    let stdin = io::stdin();
    loop {
        print!("tizenclaw> ");
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
            cmd if cmd.starts_with("/dashboard ") => {
                let action = cmd.trim_start_matches("/dashboard ").trim();
                cmd_dashboard(client, action);
            }
            prompt => {
                let session_id = explicit_session_id
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(generate_session_id);
                if let Err(error) = send_prompt(client, &session_id, prompt, stream) {
                    eprintln!("Error: {}", error);
                }
            }
        }
    }
}

fn cmd_config_get(client: &TizenClaw, path: Option<&str>) {
    match client.get_llm_config(path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_set(client: &TizenClaw, path: &str, raw_value: &str, strict_json: bool) {
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

fn cmd_config_unset(client: &TizenClaw, path: &str) {
    match client.unset_llm_config(path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_reload(client: &TizenClaw) {
    match client.reload_llm_backends() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config(client: &TizenClaw, args: &[String]) {
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
    eprintln!("  -s <id>           Reuse a fixed session ID");
    eprintln!("  --no-stream       Disable real-time streaming");
    eprintln!("  --usage           Show token usage");
    eprintln!("  --usage-baseline  JSON baseline for usage delta");
    eprintln!("  -h, --help        Show this help\n");
    eprintln!("Dashboard commands:");
    eprintln!("  tizenclaw-cli dashboard start [--port N]");
    eprintln!("                                   Start the web dashboard");
    eprintln!("  tizenclaw-cli dashboard stop    Stop the web dashboard");
    eprintln!("  tizenclaw-cli dashboard status  Show dashboard status\n");
    eprintln!("Registration commands:");
    eprintln!("  tizenclaw-cli register tool <path>");
    eprintln!("  tizenclaw-cli register skill <path>");
    eprintln!("  tizenclaw-cli unregister tool <path>");
    eprintln!("  tizenclaw-cli unregister skill <path>");
    eprintln!("  tizenclaw-cli list registrations\n");
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
    eprintln!("If no prompt given, starts interactive mode.");
}

fn cmd_register(client: &TizenClaw, kind: &str, path: &str) {
    match client.register_path(kind, path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_unregister(client: &TizenClaw, kind: &str, path: &str) {
    match client.unregister_path(kind, path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_list_registrations(client: &TizenClaw) {
    match client.list_registered_paths() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut session_id: Option<String> = None;
    let mut explicit_session_id = false;
    let mut stream = true;
    let mut usage_requested = false;
    let mut usage_baseline: Option<Value> = None;
    let mut prompt_parts: Vec<String> = vec![];
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-s" if i + 1 < args.len() => {
                i += 1;
                session_id = Some(args[i].clone());
                explicit_session_id = true;
            }
            "--no-stream" => stream = false,
            "--usage" => {
                usage_requested = true;
            }
            "--usage-baseline" if i + 1 < args.len() => {
                i += 1;
                usage_baseline = Some(parse_usage_baseline(&args[i]).unwrap_or_else(|err| {
                    eprintln!("{}", err);
                    std::process::exit(1);
                }));
            }
            "--usage-baseline" => {
                eprintln!("Usage: tizenclaw-cli --usage-baseline '<json>'");
                std::process::exit(1);
            }
            "dashboard" if i + 1 < args.len() => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                i += 1;
                let mut command = args[i].clone();
                i += 1;
                while i < args.len() {
                    command.push(' ');
                    command.push_str(&args[i]);
                    i += 1;
                }
                cmd_dashboard(&client, &command);
                return;
            }
            "register" if i + 2 < args.len() => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_register(&client, &args[i + 1], &args[i + 2]);
                return;
            }
            "unregister" if i + 2 < args.len() => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_unregister(&client, &args[i + 1], &args[i + 2]);
                return;
            }
            "list" if i + 1 < args.len() && args[i + 1] == "registrations" => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_list_registrations(&client);
                return;
            }
            "config" => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_config(&client, &args[i + 1..]);
                return;
            }
            "auth" => {
                cmd_auth(&args[i + 1..]);
                return;
            }
            "setup" => {
                cmd_setup();
                return;
            }
            "dashboard" => {
                eprintln!("Usage: tizenclaw-cli dashboard <start [--port N]|stop|status>");
                std::process::exit(1);
            }
            "register" => {
                eprintln!("Usage: tizenclaw-cli register <tool|skill> <path>");
                std::process::exit(1);
            }
            "unregister" => {
                eprintln!("Usage: tizenclaw-cli unregister <tool|skill> <path>");
                std::process::exit(1);
            }
            _ => {
                for arg in args.iter().skip(i) {
                    prompt_parts.push(arg.clone());
                }
                break;
            }
        }
        i += 1;
    }

    let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));

    if usage_requested {
        show_usage(&client, session_id.as_deref(), usage_baseline.as_ref());
        return;
    }

    let prompt = prompt_parts.join(" ");

    if !prompt.is_empty() {
        let resolved_session_id = session_id.unwrap_or_else(generate_session_id);
        if let Err(error) = send_prompt(&client, &resolved_session_id, &prompt, stream) {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    } else {
        let explicit = if explicit_session_id {
            session_id.as_deref()
        } else {
            None
        };
        interactive_mode(&client, explicit, stream);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        codex_oauth_snapshot, dashboard_port_from_doc, merge_missing, parse_chat_ids,
        parse_codex_login_state,
    };
    use serde_json::json;

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
        assert_eq!(doc["backends"]["openai-codex"]["model"], json!("custom-model"));
        assert_eq!(doc["backends"]["openai-codex"]["transport"], json!("responses"));
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
}
