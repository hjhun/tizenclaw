//! Telegram Bot API client — async long-polling channel.
//!
//! Uses `getUpdates` long-polling to receive messages. Polls natively
//! on the Tokio async reactor (epoll) avoiding expensive thread allocation.

use super::{Channel, ChannelConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_CONCURRENT_HANDLERS: i32 = 3;
const DEFAULT_CLI_TIMEOUT_SECS: u64 = 900;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelegramInteractionMode {
    Chat,
    Coding,
}

impl Default for TelegramInteractionMode {
    fn default() -> Self {
        Self::Chat
    }
}

impl TelegramInteractionMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "chat" => Some(Self::Chat),
            "coding" | "coding-agent" | "coding_agent" | "agent" => Some(Self::Coding),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Coding => "coding",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelegramCliBackend {
    Codex,
    Gemini,
    Claude,
}

impl Default for TelegramCliBackend {
    fn default() -> Self {
        Self::Codex
    }
}

impl TelegramCliBackend {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Some(Self::Codex),
            "gemini" => Some(Self::Gemini),
            "claude" | "claude-code" | "claude_code" => Some(Self::Claude),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Claude => "claude",
        }
    }

    fn binary_candidates(&self) -> &'static [&'static str] {
        match self {
            Self::Codex => &["codex"],
            Self::Gemini => &["gemini"],
            Self::Claude => &["claude", "claude-code"],
        }
    }

    fn all() -> [Self; 3] {
        [Self::Codex, Self::Gemini, Self::Claude]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelegramExecutionMode {
    Plan,
    Fast,
}

impl Default for TelegramExecutionMode {
    fn default() -> Self {
        Self::Plan
    }
}

impl TelegramExecutionMode {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "plan" => Some(Self::Plan),
            "fast" => Some(Self::Fast),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Fast => "fast",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct TelegramCliUsageStats {
    requests: u64,
    successes: u64,
    failures: u64,
    total_duration_ms: u64,
    last_started_at_ms: Option<u64>,
    last_completed_at_ms: Option<u64>,
    last_exit_code: Option<i32>,
}

impl TelegramCliUsageStats {
    fn average_duration_ms(&self) -> u64 {
        if self.requests == 0 {
            0
        } else {
            self.total_duration_ms / self.requests
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TelegramChatState {
    interaction_mode: TelegramInteractionMode,
    cli_backend: TelegramCliBackend,
    execution_mode: TelegramExecutionMode,
    auto_approve: bool,
    usage: HashMap<String, TelegramCliUsageStats>,
}

impl Default for TelegramChatState {
    fn default() -> Self {
        Self {
            interaction_mode: TelegramInteractionMode::Chat,
            cli_backend: TelegramCliBackend::Codex,
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            usage: HashMap::new(),
        }
    }
}

impl TelegramChatState {
    fn usage_for(&self, backend: TelegramCliBackend) -> TelegramCliUsageStats {
        self.usage
            .get(backend.as_str())
            .cloned()
            .unwrap_or_default()
    }
}

pub struct TelegramClient {
    name: String,
    bot_token: String,
    allowed_chat_ids: Arc<HashSet<i64>>,
    running: Arc<AtomicBool>,
    active_handlers: Arc<AtomicI32>,
    agent: Option<Arc<crate::core::agent_core::AgentCore>>,
    cli_workdir: Arc<PathBuf>,
    cli_timeout_secs: u64,
    cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
    chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
    chat_state_path: Arc<PathBuf>,
}

impl TelegramClient {
    pub fn new(
        config: &ChannelConfig,
        agent: Option<Arc<crate::core::agent_core::AgentCore>>,
    ) -> Self {
        let mut bot_token = config
            .settings
            .get("bot_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut allowed_ids = HashSet::new();
        if let Some(arr) = config
            .settings
            .get("allowed_chat_ids")
            .and_then(|v| v.as_array())
        {
            for id in arr {
                if let Some(n) = id.as_i64() {
                    allowed_ids.insert(n);
                }
            }
        }

        let default_workdir = std::env::current_dir()
            .unwrap_or_else(|_| crate::core::runtime_paths::default_data_dir());
        let mut cli_workdir = config
            .settings
            .get("cli_workdir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or(default_workdir);
        let mut cli_timeout_secs = config
            .settings
            .get("cli_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_CLI_TIMEOUT_SECS);
        let mut backend_overrides = HashMap::new();
        Self::read_backend_overrides(config.settings.get("cli_backends"), &mut backend_overrides);

        let config_dir = crate::core::runtime_paths::default_data_dir().join("config");
        let telegram_config = config_dir.join("telegram_config.json");
        if let Ok(content) = std::fs::read_to_string(&telegram_config) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                if let Some(token) = json.get("bot_token").and_then(|v| v.as_str()) {
                    if !token.is_empty() {
                        bot_token = token.to_string();
                        log::info!("TelegramClient: loaded bot_token override");
                    }
                }
                if let Some(arr) = json.get("allowed_chat_ids").and_then(|v| v.as_array()) {
                    if !arr.is_empty() {
                        allowed_ids.clear();
                        for id in arr {
                            if let Some(n) = id.as_i64() {
                                allowed_ids.insert(n);
                            }
                        }
                    }
                }
                if let Some(path) = json.get("cli_workdir").and_then(|v| v.as_str()) {
                    if !path.trim().is_empty() {
                        cli_workdir = PathBuf::from(path);
                    }
                }
                if let Some(timeout) = json.get("cli_timeout_secs").and_then(|v| v.as_u64()) {
                    cli_timeout_secs = timeout;
                }
                Self::read_backend_overrides(json.get("cli_backends"), &mut backend_overrides);
            }
        }

        let cli_backend_paths = Arc::new(Self::resolve_cli_backend_paths(&backend_overrides));
        let chat_state_path = Arc::new(config_dir.join("telegram_channel_state.json"));
        let chat_states = Arc::new(Mutex::new(Self::load_chat_states(&chat_state_path)));

        TelegramClient {
            name: config.name.clone(),
            bot_token,
            allowed_chat_ids: Arc::new(allowed_ids),
            running: Arc::new(AtomicBool::new(false)),
            active_handlers: Arc::new(AtomicI32::new(0)),
            agent,
            cli_workdir: Arc::new(cli_workdir),
            cli_timeout_secs,
            cli_backend_paths,
            chat_states,
            chat_state_path,
        }
    }

    fn read_backend_overrides(
        value: Option<&Value>,
        overrides: &mut HashMap<TelegramCliBackend, String>,
    ) {
        let Some(value) = value else {
            return;
        };
        let Some(object) = value.as_object() else {
            return;
        };

        for backend in TelegramCliBackend::all() {
            let Some(entry) = object.get(backend.as_str()) else {
                continue;
            };
            if let Some(path) = entry.as_str() {
                if !path.trim().is_empty() {
                    overrides.insert(backend, path.to_string());
                }
                continue;
            }

            if let Some(path) = entry.get("binary_path").and_then(|v| v.as_str()) {
                if !path.trim().is_empty() {
                    overrides.insert(backend, path.to_string());
                }
            }
        }
    }

    fn resolve_cli_backend_paths(
        overrides: &HashMap<TelegramCliBackend, String>,
    ) -> HashMap<TelegramCliBackend, String> {
        let mut resolved = HashMap::new();

        for backend in TelegramCliBackend::all() {
            if let Some(path) = overrides.get(&backend) {
                resolved.insert(backend, path.clone());
                continue;
            }

            if let Some(path) = Self::lookup_binary_on_path(backend.binary_candidates()) {
                resolved.insert(backend, path);
            }
        }

        resolved
    }

    fn lookup_binary_on_path(candidates: &[&str]) -> Option<String> {
        let path_var = std::env::var_os("PATH")?;
        let path_dirs = std::env::split_paths(&path_var).collect::<Vec<_>>();

        for candidate in candidates {
            let candidate_path = Path::new(candidate);
            if candidate_path.is_absolute() && candidate_path.is_file() {
                return Some(candidate_path.to_string_lossy().to_string());
            }

            for dir in &path_dirs {
                let path = dir.join(candidate);
                if path.is_file() {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }

        None
    }

    fn load_chat_states(path: &Path) -> HashMap<i64, TelegramChatState> {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => return HashMap::new(),
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    fn persist_chat_states(path: &Path, states: &HashMap<i64, TelegramChatState>) {
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                log::warn!(
                    "TelegramClient: failed to create state dir '{}': {}",
                    parent.display(),
                    err
                );
                return;
            }
        }

        let serialized = match serde_json::to_string_pretty(states) {
            Ok(serialized) => serialized,
            Err(err) => {
                log::warn!("TelegramClient: failed to serialize state: {}", err);
                return;
            }
        };

        if let Err(err) = std::fs::write(path, serialized) {
            log::warn!(
                "TelegramClient: failed to write state '{}': {}",
                path.display(),
                err
            );
        }
    }

    fn current_timestamp_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn truncate_chars(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }

        let truncated = text.chars().take(max_chars).collect::<String>();
        format!("{}\n...(truncated)", truncated)
    }

    // Static so it can be called inside spawned async tasks easily
    fn send_telegram_message(bot_token: &str, chat_id: i64, text: &str) {
        if bot_token.is_empty() {
            return;
        }

        let safe_text = Self::truncate_chars(text, 4000);

        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let payload = json!({
            "chat_id": chat_id,
            "text": safe_text,
            "parse_mode": "Markdown"
        })
        .to_string();

        let client = crate::infra::http_client::HttpClient::new();
        tokio::spawn(async move {
            match client.post(&url, &payload).await {
                Ok(resp) if resp.status_code >= 400 => {
                    let plain = json!({"chat_id": chat_id, "text": safe_text}).to_string();
                    let _ = client.post(&url, &plain).await;
                }
                Err(e) => log::error!("Telegram sendMessage failed: {}", e),
                _ => {}
            }
        });
    }

    fn supported_commands_text() -> String {
        [
            "Telegram coding-agent commands:",
            "/select <chat|coding> - switch between normal chat and local CLI coding mode",
            "/cli-backend <codex|gemini|claude> - choose the coding-agent backend",
            "/usage - show locally tracked usage for the selected CLI backend",
            "/mode <plan|fast> - switch planning style for coding mode prompts",
            "/status - show the current Telegram channel control state",
            "/auto-approve <on|off> - toggle backend auto approval when supported",
        ]
        .join("\n")
    }

    fn parse_command(text: &str) -> Option<(String, Vec<String>)> {
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let mut parts = trimmed.split_whitespace();
        let command_token = parts.next()?;
        let command = command_token
            .trim_start_matches('/')
            .split('@')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if command.is_empty() {
            return None;
        }

        Some((command, parts.map(|part| part.to_string()).collect()))
    }

    fn load_chat_state_snapshot(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        chat_id: i64,
    ) -> TelegramChatState {
        match chat_states.lock() {
            Ok(states) => states.get(&chat_id).cloned().unwrap_or_default(),
            Err(err) => {
                log::warn!("TelegramClient: state lock poisoned: {}", err);
                TelegramChatState::default()
            }
        }
    }

    fn mutate_chat_state<F>(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        mutator: F,
    ) -> String
    where
        F: FnOnce(&mut TelegramChatState) -> String,
    {
        let (reply, snapshot) = match chat_states.lock() {
            Ok(mut states) => {
                let state = states.entry(chat_id).or_default();
                let reply = mutator(state);
                (reply, states.clone())
            }
            Err(err) => {
                return format!("State update failed: {}", err);
            }
        };

        Self::persist_chat_states(state_path, &snapshot);
        reply
    }

    fn set_interaction_mode(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> String {
        let Some(mode_raw) = args.first() else {
            return "Usage: /select <chat|coding>".to_string();
        };
        let Some(mode) = TelegramInteractionMode::parse(mode_raw) else {
            return "Invalid mode. Use /select chat or /select coding".to_string();
        };

        Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
            state.interaction_mode = mode;
            format!(
                "Interaction mode set to `{}`.\nSelected CLI backend remains `{}`.",
                mode.as_str(),
                state.cli_backend.as_str()
            )
        })
    }

    fn set_cli_backend(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
    ) -> String {
        let Some(backend_raw) = args.first() else {
            return "Usage: /cli-backend <codex|gemini|claude>".to_string();
        };
        let Some(backend) = TelegramCliBackend::parse(backend_raw) else {
            return "Invalid CLI backend. Use codex, gemini, or claude".to_string();
        };

        let availability = cli_backend_paths
            .get(&backend)
            .map(|path| format!("Resolved binary: `{}`", path))
            .unwrap_or_else(|| {
                "Warning: backend binary was not found on PATH. You can still keep it selected, but execution will fail until the binary is installed or configured.".to_string()
            });

        Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
            state.cli_backend = backend;
            format!(
                "CLI backend set to `{}`.\n{}",
                backend.as_str(),
                availability
            )
        })
    }

    fn set_execution_mode(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> String {
        let Some(mode_raw) = args.first() else {
            return "Usage: /mode <plan|fast>".to_string();
        };
        let Some(mode) = TelegramExecutionMode::parse(mode_raw) else {
            return "Invalid execution mode. Use /mode plan or /mode fast".to_string();
        };

        Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
            state.execution_mode = mode;
            format!(
                "Execution mode set to `{}` for Telegram coding-agent prompts.",
                mode.as_str()
            )
        })
    }

    fn set_auto_approve(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> String {
        let Some(value_raw) = args.first() else {
            return "Usage: /auto-approve <on|off>".to_string();
        };
        let enabled = match value_raw.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => return "Invalid value. Use /auto-approve on or /auto-approve off".to_string(),
        };

        Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
            state.auto_approve = enabled;
            format!(
                "Auto-approve is now `{}` for the `{}` backend.",
                if enabled { "on" } else { "off" },
                state.cli_backend.as_str()
            )
        })
    }

    fn format_usage_text(state: &TelegramChatState, backend: TelegramCliBackend) -> String {
        let usage = state.usage_for(backend);
        format!(
            "Local usage for `{}` via Telegram:\n\
requests: {}\n\
successes: {}\n\
failures: {}\n\
avg duration: {} ms\n\
last exit code: {}\n\
last started: {}\n\
last completed: {}",
            backend.as_str(),
            usage.requests,
            usage.successes,
            usage.failures,
            usage.average_duration_ms(),
            usage
                .last_exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string()),
            usage
                .last_started_at_ms
                .map(|ts| ts.to_string())
                .unwrap_or_else(|| "-".to_string()),
            usage
                .last_completed_at_ms
                .map(|ts| ts.to_string())
                .unwrap_or_else(|| "-".to_string())
        )
    }

    fn format_status_text(
        chat_id: i64,
        state: &TelegramChatState,
        cli_workdir: &Path,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        active_handlers: i32,
    ) -> String {
        let backend_path = cli_backend_paths
            .get(&state.cli_backend)
            .map(|path| path.as_str())
            .unwrap_or("not found");
        let usage = state.usage_for(state.cli_backend);

        format!(
            "Telegram channel status:\n\
chat_id: `{}`\n\
interaction mode: `{}`\n\
cli backend: `{}`\n\
execution mode: `{}`\n\
auto approve: `{}`\n\
cli binary: `{}`\n\
cli workdir: `{}`\n\
active handlers: `{}`\n\
backend requests: `{}`\n\
backend successes: `{}`\n\
backend failures: `{}`",
            chat_id,
            state.interaction_mode.as_str(),
            state.cli_backend.as_str(),
            state.execution_mode.as_str(),
            if state.auto_approve { "on" } else { "off" },
            backend_path,
            cli_workdir.display(),
            active_handlers,
            usage.requests,
            usage.successes,
            usage.failures
        )
    }

    fn handle_command(
        chat_id: i64,
        text: &str,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        cli_workdir: &Path,
        active_handlers: i32,
    ) -> Option<String> {
        let (command, args) = Self::parse_command(text)?;

        let reply = match command.as_str() {
            "start" | "help" => Self::supported_commands_text(),
            "select" => Self::set_interaction_mode(chat_states, state_path, chat_id, &args),
            "cli-backend" | "cli_backend" => {
                Self::set_cli_backend(chat_states, state_path, chat_id, &args, cli_backend_paths)
            }
            "mode" => Self::set_execution_mode(chat_states, state_path, chat_id, &args),
            "auto-approve" | "auto_approve" => {
                Self::set_auto_approve(chat_states, state_path, chat_id, &args)
            }
            "usage" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                Self::format_usage_text(&state, state.cli_backend)
            }
            "status" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                Self::format_status_text(
                    chat_id,
                    &state,
                    cli_workdir,
                    cli_backend_paths,
                    active_handlers,
                )
            }
            _ => format!(
                "Unknown command `/{}`.\n\n{}",
                command,
                Self::supported_commands_text()
            ),
        };

        Some(reply)
    }

    fn build_cli_prompt(
        execution_mode: TelegramExecutionMode,
        backend: TelegramCliBackend,
        cli_workdir: &Path,
        text: &str,
    ) -> String {
        let mode_prefix = match execution_mode {
            TelegramExecutionMode::Plan => {
                "You are operating in TizenClaw Telegram coding mode. Start with a short plan, then perform the work carefully. Keep the final response concise and actionable."
            }
            TelegramExecutionMode::Fast => {
                "You are operating in TizenClaw Telegram coding mode. Optimize for speed, keep the response concise, and take the fastest reasonable path."
            }
        };

        format!(
            "{}\n\
\n\
Selected backend: {}\n\
Workspace: {}\n\
\n\
User request:\n{}",
            mode_prefix,
            backend.as_str(),
            cli_workdir.display(),
            text.trim()
        )
    }

    fn build_cli_invocation(
        state: &TelegramChatState,
        cli_workdir: &Path,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        text: &str,
    ) -> Result<(String, Vec<String>), String> {
        let binary = cli_backend_paths
            .get(&state.cli_backend)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Selected backend `{}` is not available on PATH.",
                    state.cli_backend.as_str()
                )
            })?;

        let prompt = Self::build_cli_prompt(
            state.execution_mode,
            state.cli_backend,
            cli_workdir,
            text,
        );
        let mut args = Vec::new();

        match state.cli_backend {
            TelegramCliBackend::Codex => {
                args.push("exec".to_string());
                if state.auto_approve {
                    args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                } else {
                    args.push("--full-auto".to_string());
                }
                args.push("-C".to_string());
                args.push(cli_workdir.to_string_lossy().to_string());
                args.push("--skip-git-repo-check".to_string());
                args.push(prompt);
            }
            TelegramCliBackend::Gemini => {
                if state.auto_approve {
                    args.push("-y".to_string());
                    args.push("--approval-mode".to_string());
                    args.push("yolo".to_string());
                } else {
                    args.push("--approval-mode".to_string());
                    args.push("default".to_string());
                }
                args.push("--prompt".to_string());
                args.push(prompt);
            }
            TelegramCliBackend::Claude => {
                args.push("--print".to_string());
                args.push("--output-format".to_string());
                args.push("text".to_string());
                args.push("--permission-mode".to_string());
                args.push(if state.auto_approve {
                    "bypassPermissions".to_string()
                } else {
                    "default".to_string()
                });
                args.push(prompt);
            }
        }

        Ok((binary, args))
    }

    fn format_cli_result(
        backend: TelegramCliBackend,
        exit_code: i32,
        duration_ms: u64,
        stdout: &str,
        stderr: &str,
    ) -> String {
        let stdout = stdout.trim();
        let stderr = stderr.trim();
        let body = if !stdout.is_empty() {
            stdout
        } else if !stderr.is_empty() {
            stderr
        } else if exit_code == 0 {
            "CLI completed successfully with no output."
        } else {
            "CLI failed with no output."
        };

        format!(
            "`{}` finished with exit code `{}` in `{}` ms.\n\n{}",
            backend.as_str(),
            exit_code,
            duration_ms,
            Self::truncate_chars(body, 3400)
        )
    }

    async fn execute_cli_request(
        chat_id: i64,
        text: &str,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
    ) -> String {
        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        let backend = state.cli_backend;
        let started_at = Self::current_timestamp_millis();

        let invocation =
            match Self::build_cli_invocation(&state, &cli_workdir, &cli_backend_paths, text) {
                Ok(invocation) => invocation,
                Err(err) => return err,
            };

        let snapshot = match chat_states.lock() {
            Ok(mut states) => {
                let state = states.entry(chat_id).or_default();
                let usage = state
                    .usage
                    .entry(backend.as_str().to_string())
                    .or_default();
                usage.requests = usage.requests.saturating_add(1);
                usage.last_started_at_ms = Some(started_at);
                states.clone()
            }
            Err(err) => {
                return format!("State update failed before CLI execution: {}", err);
            }
        };
        Self::persist_chat_states(&state_path, &snapshot);

        let (binary, args) = invocation;
        let mut command = tokio::process::Command::new(&binary);
        command.args(&args);
        command.current_dir(cli_workdir.as_ref());
        command.env("NO_COLOR", "1");
        command.env("CLICOLOR", "0");
        command.env("TERM", "dumb");
        command.kill_on_drop(true);

        let child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state
                            .usage
                            .entry(backend.as_str().to_string())
                            .or_default();
                        usage.failures = usage.failures.saturating_add(1);
                        usage.last_exit_code = Some(-1);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }
                return format!("Failed to start `{}`: {}", backend.as_str(), err);
            }
        };

        let started = SystemTime::now();
        let timed_output =
            tokio::time::timeout(Duration::from_secs(cli_timeout_secs), child.wait_with_output())
                .await;

        match timed_output {
            Ok(Ok(output)) => {
                let duration_ms = started.elapsed().unwrap_or_default().as_millis() as u64;
                let exit_code = output.status.code().unwrap_or(-1);
                let success = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state
                            .usage
                            .entry(backend.as_str().to_string())
                            .or_default();
                        if success {
                            usage.successes = usage.successes.saturating_add(1);
                        } else {
                            usage.failures = usage.failures.saturating_add(1);
                        }
                        usage.total_duration_ms =
                            usage.total_duration_ms.saturating_add(duration_ms);
                        usage.last_exit_code = Some(exit_code);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }

                Self::format_cli_result(backend, exit_code, duration_ms, &stdout, &stderr)
            }
            Ok(Err(err)) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state
                            .usage
                            .entry(backend.as_str().to_string())
                            .or_default();
                        usage.failures = usage.failures.saturating_add(1);
                        usage.last_exit_code = Some(-1);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }
                format!("`{}` failed while waiting for output: {}", backend.as_str(), err)
            }
            Err(_) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state
                            .usage
                            .entry(backend.as_str().to_string())
                            .or_default();
                        usage.failures = usage.failures.saturating_add(1);
                        usage.last_exit_code = Some(-2);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }

                format!(
                    "`{}` timed out after `{}` seconds.",
                    backend.as_str(),
                    cli_timeout_secs
                )
            }
        }
    }

    async fn route_message(
        chat_id: i64,
        text: &str,
        agent: Option<Arc<crate::core::agent_core::AgentCore>>,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
        active_handlers: i32,
    ) -> String {
        if let Some(reply) = Self::handle_command(
            chat_id,
            text,
            &chat_states,
            &state_path,
            &cli_backend_paths,
            &cli_workdir,
            active_handlers,
        ) {
            return reply;
        }

        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        match state.interaction_mode {
            TelegramInteractionMode::Chat => {
                let Some(agent_core) = agent else {
                    return "AgentCore is not available for chat mode.".to_string();
                };
                let session_id = format!("tg_{}", chat_id);
                agent_core.process_prompt(&session_id, text, None).await
            }
            TelegramInteractionMode::Coding => {
                Self::execute_cli_request(
                    chat_id,
                    text,
                    cli_workdir,
                    cli_timeout_secs,
                    cli_backend_paths,
                    chat_states,
                    state_path,
                )
                .await
            }
        }
    }
}

impl Channel for TelegramClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }
        if self.bot_token.is_empty() || self.bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
            log::warn!("TelegramClient: invalid bot token");
            return false;
        }

        let reset_url = format!(
            "https://api.telegram.org/bot{}/deleteWebhook",
            self.bot_token
        );
        let client = crate::infra::http_client::HttpClient::new();
        let _ = client.get_sync(&reset_url);

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let bot_token = self.bot_token.clone();
        let allowed_ids = self.allowed_chat_ids.clone();
        let active_handlers = self.active_handlers.clone();
        let agent = self.agent.clone();
        let cli_workdir = self.cli_workdir.clone();
        let cli_timeout_secs = self.cli_timeout_secs;
        let cli_backend_paths = self.cli_backend_paths.clone();
        let chat_states = self.chat_states.clone();
        let chat_state_path = self.chat_state_path.clone();

        tokio::spawn(async move {
            log::debug!("TelegramClient async epoll reactor started");
            let mut offset: i64 = 0;
            let mut backoff_secs = 5u64;

            while running.load(Ordering::SeqCst) {
                let url = format!(
                    "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=50",
                    bot_token, offset
                );

                let client = crate::infra::http_client::HttpClient::new();
                let resp = match client.get(&url).await {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("Telegram polling error: {}", e);
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(60);
                        continue;
                    }
                };

                if !running.load(Ordering::SeqCst) {
                    break;
                }
                backoff_secs = 5;

                let data: Value = match serde_json::from_str(&resp.body) {
                    Ok(v) => v,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if !data["ok"].as_bool().unwrap_or(false) {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }

                if let Some(results) = data["result"].as_array() {
                    for item in results {
                        offset = item["update_id"].as_i64().unwrap_or(0) + 1;
                        let msg = match item.get("message") {
                            Some(m) => m,
                            None => continue,
                        };
                        let text = msg["text"].as_str().unwrap_or("");
                        let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);

                        if text.is_empty() || chat_id == 0 {
                            continue;
                        }
                        if !allowed_ids.is_empty() && !allowed_ids.contains(&chat_id) {
                            log::debug!("Blocked chat_id {} — not in allowlist", chat_id);
                            continue;
                        }

                        let current_handlers = active_handlers.load(Ordering::SeqCst);
                        if current_handlers >= MAX_CONCURRENT_HANDLERS {
                            log::warn!(
                                "Telegram dropping message: max concurrent handlers ({}) reached",
                                current_handlers
                            );
                            continue;
                        }

                        log::debug!("Telegram received from {}: {}", chat_id, text);

                        active_handlers.fetch_add(1, Ordering::SeqCst);
                        let text_clone = text.to_string();
                        let bot_token_clone = bot_token.clone();
                        let agent_clone = agent.clone();
                        let active_handlers_clone = active_handlers.clone();
                        let cli_workdir_clone = cli_workdir.clone();
                        let cli_backend_paths_clone = cli_backend_paths.clone();
                        let chat_states_clone = chat_states.clone();
                        let chat_state_path_clone = chat_state_path.clone();

                        tokio::spawn(async move {
                            let result = TelegramClient::route_message(
                                chat_id,
                                &text_clone,
                                agent_clone,
                                cli_workdir_clone,
                                cli_timeout_secs,
                                cli_backend_paths_clone,
                                chat_states_clone,
                                chat_state_path_clone,
                                current_handlers + 1,
                            )
                            .await;
                            TelegramClient::send_telegram_message(&bot_token_clone, chat_id, &result);
                            active_handlers_clone.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                }
            }
            log::debug!("TelegramClient async epoll reactor stopped");
        });

        log::info!("TelegramClient started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        for chat_id in self.allowed_chat_ids.iter() {
            Self::send_telegram_message(&self.bot_token, *chat_id, msg);
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TelegramCliBackend, TelegramChatState, TelegramExecutionMode, TelegramInteractionMode,
        TelegramClient,
    };

    #[test]
    fn parse_command_handles_bot_mentions() {
        let parsed = TelegramClient::parse_command("/status@tizenclaw_bot").unwrap();
        assert_eq!(parsed.0, "status");
        assert!(parsed.1.is_empty());
    }

    #[test]
    fn parse_mode_aliases_work() {
        assert_eq!(
            TelegramInteractionMode::parse("coding-agent"),
            Some(TelegramInteractionMode::Coding)
        );
        assert_eq!(TelegramExecutionMode::parse("fast"), Some(TelegramExecutionMode::Fast));
        assert_eq!(TelegramCliBackend::parse("claude-code"), Some(TelegramCliBackend::Claude));
    }

    #[test]
    fn default_chat_state_prefers_codex_plan_chat_mode() {
        let state = TelegramChatState::default();
        assert_eq!(state.interaction_mode, TelegramInteractionMode::Chat);
        assert_eq!(state.cli_backend, TelegramCliBackend::Codex);
        assert_eq!(state.execution_mode, TelegramExecutionMode::Plan);
        assert!(!state.auto_approve);
    }
}
