//! Telegram Bot API client — async long-polling channel.
//!
//! Uses `getUpdates` long-polling to receive messages. Polls natively
//! on the Tokio async reactor (epoll) avoiding expensive thread allocation.

use super::{Channel, ChannelConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncBufReadExt;

const MAX_CONCURRENT_HANDLERS: i32 = 3;
const DEFAULT_CLI_TIMEOUT_SECS: u64 = 900;
const CLI_PROGRESS_UPDATE_SECS: u64 = 15;
const CLI_PROGRESS_MIN_PARTIAL_CHARS: usize = 80;
const DEFAULT_GEMINI_CLI_MODEL: &str = "gemini-2.5-flash";

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
            Self::Gemini => &["gemini", "/snap/bin/gemini"],
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
#[serde(default)]
struct TelegramChatState {
    interaction_mode: TelegramInteractionMode,
    cli_backend: TelegramCliBackend,
    execution_mode: TelegramExecutionMode,
    auto_approve: bool,
    project_dir: Option<String>,
    chat_session_index: u64,
    coding_session_index: u64,
    usage: HashMap<String, TelegramCliUsageStats>,
}

impl Default for TelegramChatState {
    fn default() -> Self {
        Self {
            interaction_mode: TelegramInteractionMode::Chat,
            cli_backend: TelegramCliBackend::Codex,
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            chat_session_index: 1,
            coding_session_index: 1,
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

    fn session_index_for(&self, mode: TelegramInteractionMode) -> u64 {
        match mode {
            TelegramInteractionMode::Chat => self.chat_session_index,
            TelegramInteractionMode::Coding => self.coding_session_index,
        }
    }

    fn session_label_for(&self, mode: TelegramInteractionMode) -> String {
        format!("{}-{:04}", mode.as_str(), self.session_index_for(mode))
    }

    fn active_session_label(&self) -> String {
        self.session_label_for(self.interaction_mode)
    }

    fn effective_cli_workdir(&self, default_cli_workdir: &Path) -> PathBuf {
        self.project_dir
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_cli_workdir.to_path_buf())
    }
}

#[derive(Clone, Debug)]
struct TelegramOutgoingMessage {
    text: String,
    reply_markup: Option<Value>,
}

impl TelegramOutgoingMessage {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            reply_markup: None,
        }
    }

    fn with_markup(text: impl Into<String>, reply_markup: Value) -> Self {
        Self {
            text: text.into(),
            reply_markup: Some(reply_markup),
        }
    }
}

#[derive(Debug)]
enum TelegramCliStreamEvent {
    StdoutLine(String),
    StderrLine(String),
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
    cli_backend_models: Arc<HashMap<TelegramCliBackend, String>>,
    chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
    chat_state_path: Arc<PathBuf>,
    /// UNIX seconds of the last user message; used for idle-trim scheduling.
    last_user_input: Arc<AtomicU64>,
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
        let mut backend_models = HashMap::new();
        Self::read_backend_overrides(config.settings.get("cli_backends"), &mut backend_overrides);
        Self::read_backend_models(config.settings.get("cli_backends"), &mut backend_models);

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
                Self::read_backend_models(json.get("cli_backends"), &mut backend_models);
            }
        }

        Self::read_backend_models_from_llm_config(&config_dir, &mut backend_models);
        backend_models
            .entry(TelegramCliBackend::Gemini)
            .or_insert_with(|| DEFAULT_GEMINI_CLI_MODEL.to_string());

        let cli_backend_paths = Arc::new(Self::resolve_cli_backend_paths(&backend_overrides));
        let cli_backend_models = Arc::new(backend_models);
        let chat_state_path = Arc::new(config_dir.join("telegram_channel_state.json"));
        let chat_states = Arc::new(Mutex::new(Self::load_chat_states(&chat_state_path)));

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
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
            cli_backend_models,
            chat_states,
            chat_state_path,
            last_user_input: Arc::new(AtomicU64::new(now_secs)),
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

    fn read_backend_models(
        value: Option<&Value>,
        models: &mut HashMap<TelegramCliBackend, String>,
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
            let Some(model) = entry.get("model").and_then(|v| v.as_str()) else {
                continue;
            };
            let trimmed = model.trim();
            if !trimmed.is_empty() {
                models.insert(backend, trimmed.to_string());
            }
        }
    }

    fn read_backend_models_from_llm_config(
        config_dir: &Path,
        models: &mut HashMap<TelegramCliBackend, String>,
    ) {
        if models.contains_key(&TelegramCliBackend::Gemini) {
            return;
        }

        let llm_config = config_dir.join("llm_config.json");
        let Ok(content) = std::fs::read_to_string(&llm_config) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&content) else {
            return;
        };
        let Some(model) = json
            .get("backends")
            .and_then(|v| v.get("gemini"))
            .and_then(|v| v.get("model"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return;
        };

        models.insert(TelegramCliBackend::Gemini, model.to_string());
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

    fn telegram_session_root() -> PathBuf {
        crate::core::runtime_paths::default_data_dir().join("telegram_sessions")
    }

    fn session_file_path(
        chat_id: i64,
        mode: TelegramInteractionMode,
        state: &TelegramChatState,
    ) -> PathBuf {
        let session_label = state.session_label_for(mode);
        Self::telegram_session_root()
            .join(chat_id.to_string())
            .join(mode.as_str())
            .join(format!("{}.md", session_label))
    }

    fn ensure_session_file(
        chat_id: i64,
        mode: TelegramInteractionMode,
        state: &TelegramChatState,
    ) -> PathBuf {
        let path = Self::session_file_path(chat_id, mode, state);
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                log::warn!(
                    "TelegramClient: failed to create session dir '{}': {}",
                    parent.display(),
                    err
                );
                return path;
            }
        }

        if !path.exists() {
            let header = format!(
                "# Telegram {} session {}\n\nChat ID: `{}`\nMode: `{}`\n\n",
                mode.as_str(),
                state.session_label_for(mode),
                chat_id,
                mode.as_str()
            );
            if let Err(err) = std::fs::write(&path, header) {
                log::warn!(
                    "TelegramClient: failed to initialize session file '{}': {}",
                    path.display(),
                    err
                );
            }
        }

        path
    }

    fn append_session_transcript(
        chat_id: i64,
        mode: TelegramInteractionMode,
        state: &TelegramChatState,
        user_text: &str,
        assistant_text: &str,
    ) {
        let path = Self::ensure_session_file(chat_id, mode, state);
        let entry = format!(
            "## User\n\n{}\n\n## Assistant\n\n{}\n\n",
            user_text.trim(),
            assistant_text.trim()
        );

        if let Err(err) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| {
                use std::io::Write;
                file.write_all(entry.as_bytes())
            })
        {
            log::warn!(
                "TelegramClient: failed to append session transcript '{}': {}",
                path.display(),
                err
            );
        }
    }

    fn read_recent_session_excerpt(
        chat_id: i64,
        mode: TelegramInteractionMode,
        state: &TelegramChatState,
        max_chars: usize,
    ) -> String {
        let path = Self::session_file_path(chat_id, mode, state);
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(_) => return String::new(),
        };

        let char_count = content.chars().count();
        if char_count <= max_chars {
            return content;
        }

        let excerpt = content
            .chars()
            .skip(char_count.saturating_sub(max_chars))
            .collect::<String>();
        format!("...(recent excerpt)\n{}", excerpt)
    }

    fn truncate_chars(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }

        let truncated = text.chars().take(max_chars).collect::<String>();
        format!("{}\n...(truncated)", truncated)
    }

    fn build_send_message_payload(chat_id: i64, text: &str, reply_markup: Option<Value>) -> String {
        let mut payload = json!({
            "chat_id": chat_id,
            "text": text
        });

        if let Some(reply_markup) = reply_markup {
            payload["reply_markup"] = reply_markup;
        }

        payload.to_string()
    }

    fn command_menu_entries() -> Vec<(&'static str, &'static str)> {
        vec![
            ("select", "Switch between chat and coding mode"),
            ("agent_cli", "Choose codex, gemini, or claude"),
            ("project", "Set the project directory for coding mode"),
            ("new_session", "Start a fresh chat or coding session"),
            ("usage", "Show local usage for the selected CLI"),
            ("mode", "Set coding mode to plan or fast"),
            ("status", "Show the current Telegram channel state"),
            ("auto_approve", "Toggle automatic approval when supported"),
        ]
    }

    fn build_set_my_commands_payload() -> String {
        let commands: Vec<Value> = Self::command_menu_entries()
            .into_iter()
            .map(|(command, description)| {
                json!({
                    "command": command,
                    "description": description
                })
            })
            .collect();

        json!({
            "commands": commands
        })
        .to_string()
    }

    fn build_reply_keyboard(rows: &[&[&str]]) -> Value {
        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|entry| Value::String((*entry).to_string()))
                    .collect()
            })
            .collect();

        json!({
            "keyboard": keyboard,
            "resize_keyboard": true,
            "one_time_keyboard": true
        })
    }

    fn select_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/select chat", "/select coding"]])
    }

    fn cli_backend_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/agent_cli codex"], &["/agent_cli gemini"], &["/agent_cli claude"]])
    }

    fn mode_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/mode plan", "/mode fast"]])
    }

    fn auto_approve_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/auto_approve on", "/auto_approve off"]])
    }

    fn register_bot_commands(bot_token: &str) {
        if bot_token.is_empty() {
            return;
        }

        let url = format!("https://api.telegram.org/bot{}/setMyCommands", bot_token);
        let payload = Self::build_set_my_commands_payload();
        let client = crate::infra::http_client::HttpClient::new();

        match client.post_sync(&url, &payload) {
            Ok(_) => log::info!("Telegram bot commands registered"),
            Err(err) => log::warn!("Telegram setMyCommands failed: {}", err),
        }
    }

    // Static so it can be called inside spawned async tasks easily
    fn send_telegram_message(bot_token: &str, chat_id: i64, message: &TelegramOutgoingMessage) {
        if bot_token.is_empty() {
            return;
        }

        let safe_text = Self::truncate_chars(&message.text, 4000);

        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let payload =
            Self::build_send_message_payload(chat_id, &safe_text, message.reply_markup.clone());

        let client = crate::infra::http_client::HttpClient::new();
        tokio::spawn(async move {
            if let Err(e) = client.post(&url, &payload).await {
                log::error!("Telegram sendMessage failed: {}", e);
            }
        });
    }

    fn supported_commands_text() -> String {
        [
            "Telegram coding-agent commands:",
            "/select <chat|coding> - switch between normal chat and local CLI coding mode",
            "/agent_cli <codex|gemini|claude> - choose the coding-agent backend",
            "/project <path> - set the project directory used by coding mode",
            "/project reset - clear the per-chat project override",
            "/new_session - start a fresh session for the current mode",
            "/usage - show locally tracked usage for the selected CLI backend",
            "/mode <plan|fast> - switch planning style for coding mode prompts",
            "/status - show the current Telegram channel control state",
            "/auto_approve <on|off> - toggle backend auto approval when supported",
        ]
        .join("\n")
    }

    fn backend_usage_template(backend: TelegramCliBackend, auto_approve: bool) -> &'static str {
        match (backend, auto_approve) {
            (TelegramCliBackend::Codex, false) => {
                "`codex exec --json --full-auto -C <project> <prompt>`"
            }
            (TelegramCliBackend::Codex, true) => {
                "`codex exec --json --dangerously-bypass-approvals-and-sandbox -C <project> <prompt>`"
            }
            (TelegramCliBackend::Gemini, false) => {
                "`gemini --model <model> --prompt <prompt> --output-format text --approval-mode auto_edit`"
            }
            (TelegramCliBackend::Gemini, true) => {
                "`gemini --model <model> --prompt <prompt> --output-format text -y --approval-mode yolo`"
            }
            (TelegramCliBackend::Claude, false) => {
                "`claude --print --output-format text --permission-mode auto <prompt>`"
            }
            (TelegramCliBackend::Claude, true) => {
                "`claude --print --output-format text --permission-mode bypassPermissions <prompt>`"
            }
        }
    }

    fn backend_auth_hint(backend: TelegramCliBackend) -> &'static str {
        match backend {
            TelegramCliBackend::Codex => {
                "Codex CLI must already be logged in on the host."
            }
            TelegramCliBackend::Gemini => {
                "Gemini CLI must be authenticated on the host before Telegram can use it non-interactively."
            }
            TelegramCliBackend::Claude => {
                "Claude Code must already be authenticated on the host."
            }
        }
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
    ) -> TelegramOutgoingMessage {
        let Some(mode_raw) = args.first() else {
            return TelegramOutgoingMessage::with_markup(
                "Choose the interaction mode for this chat.",
                Self::select_keyboard(),
            );
        };
        let Some(mode) = TelegramInteractionMode::parse(mode_raw) else {
            return TelegramOutgoingMessage::with_markup(
                "Invalid mode. Choose `chat` or `coding` from the menu.",
                Self::select_keyboard(),
            );
        };

        TelegramOutgoingMessage::plain(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.interaction_mode = mode;
                format!(
                    "Interaction mode set to `{}`.\nSelected CLI backend remains `{}`.",
                    mode.as_str(),
                    state.cli_backend.as_str()
                )
            },
        ))
    }

    fn set_cli_backend(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
    ) -> TelegramOutgoingMessage {
        let Some(backend_raw) = args.first() else {
            return TelegramOutgoingMessage::with_markup(
                "Choose the CLI backend for coding mode.",
                Self::cli_backend_keyboard(),
            );
        };
        let Some(backend) = TelegramCliBackend::parse(backend_raw) else {
            return TelegramOutgoingMessage::with_markup(
                "Invalid CLI backend. Choose `codex`, `gemini`, or `claude`.",
                Self::cli_backend_keyboard(),
            );
        };

        let availability = cli_backend_paths
            .get(&backend)
            .map(|path| format!("Resolved binary: `{}`", path))
            .unwrap_or_else(|| {
                "Warning: backend binary was not found on PATH. You can still keep it selected, but execution will fail until the binary is installed or configured.".to_string()
            });

        TelegramOutgoingMessage::plain(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.cli_backend = backend;
                format!(
                    "CLI backend set to `{}`.\n{}\nUsage: {}\n{}",
                    backend.as_str(),
                    availability,
                    Self::backend_usage_template(backend, state.auto_approve),
                    Self::backend_auth_hint(backend)
                )
            },
        ))
    }

    fn resolve_project_directory(
        requested: &str,
        default_cli_workdir: &Path,
        state: &TelegramChatState,
    ) -> Result<PathBuf, String> {
        let trimmed = requested.trim();
        if trimmed.is_empty() {
            return Err("Project path cannot be empty.".to_string());
        }

        let effective_base = state.effective_cli_workdir(default_cli_workdir);
        let candidate = PathBuf::from(trimmed);
        let resolved = if candidate.is_absolute() {
            candidate
        } else {
            effective_base.join(candidate)
        };

        let canonical = std::fs::canonicalize(&resolved).map_err(|err| {
            format!(
                "Project directory '{}' could not be resolved: {}",
                resolved.display(),
                err
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "Project directory '{}' is not a directory.",
                canonical.display()
            ));
        }

        Ok(canonical)
    }

    fn set_project_directory(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        default_cli_workdir: &Path,
    ) -> TelegramOutgoingMessage {
        if args.is_empty() {
            let state = Self::load_chat_state_snapshot(chat_states, chat_id);
            let effective = state.effective_cli_workdir(default_cli_workdir);
            return TelegramOutgoingMessage::plain(format!(
                "Current project directory: `{}`.\nUse `/project <path>` to change it or `/project reset` to return to the default directory.",
                effective.display()
            ));
        }

        let requested = args.join(" ");
        match requested.trim().to_ascii_lowercase().as_str() {
            "reset" | "clear" | "default" => {
                let default_display = default_cli_workdir.display().to_string();
                return TelegramOutgoingMessage::plain(Self::mutate_chat_state(
                    chat_states,
                    state_path,
                    chat_id,
                    move |state| {
                        state.project_dir = None;
                        format!(
                            "Project directory reset.\nCoding mode will use the default CLI workdir: `{}`.",
                            default_display
                        )
                    },
                ));
            }
            _ => {}
        }

        let state = Self::load_chat_state_snapshot(chat_states, chat_id);
        let project_dir =
            match Self::resolve_project_directory(&requested, default_cli_workdir, &state) {
                Ok(path) => path,
                Err(err) => return TelegramOutgoingMessage::plain(err),
            };
        let project_dir_text = project_dir.display().to_string();

        TelegramOutgoingMessage::plain(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.project_dir = Some(project_dir_text.clone());
                format!(
                    "Project directory set to `{}`.\nCoding mode will run inside this directory for this chat.",
                    project_dir_text
                )
            },
        ))
    }

    fn set_execution_mode(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> TelegramOutgoingMessage {
        let Some(mode_raw) = args.first() else {
            return TelegramOutgoingMessage::with_markup(
                "Choose the coding execution mode.",
                Self::mode_keyboard(),
            );
        };
        let Some(mode) = TelegramExecutionMode::parse(mode_raw) else {
            return TelegramOutgoingMessage::with_markup(
                "Invalid execution mode. Choose `plan` or `fast`.",
                Self::mode_keyboard(),
            );
        };

        TelegramOutgoingMessage::plain(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.execution_mode = mode;
                format!(
                    "Execution mode set to `{}` for Telegram coding-agent prompts.",
                    mode.as_str()
                )
            },
        ))
    }

    fn set_auto_approve(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> TelegramOutgoingMessage {
        let Some(value_raw) = args.first() else {
            return TelegramOutgoingMessage::with_markup(
                "Choose whether approvals should be automatic.",
                Self::auto_approve_keyboard(),
            );
        };
        let enabled = match value_raw.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => {
                return TelegramOutgoingMessage::with_markup(
                    "Invalid value. Choose `on` or `off`.",
                    Self::auto_approve_keyboard(),
                )
            }
        };

        TelegramOutgoingMessage::plain(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.auto_approve = enabled;
                format!(
                    "Auto-approve is now `{}` for the `{}` backend.",
                    if enabled { "on" } else { "off" },
                    state.cli_backend.as_str()
                )
            },
        ))
    }

    fn start_new_session(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
    ) -> TelegramOutgoingMessage {
        let mut prepared_state = None;
        let reply = Self::mutate_chat_state(chat_states, state_path, chat_id, |state| {
            let mode = state.interaction_mode;
            match mode {
                TelegramInteractionMode::Chat => {
                    state.chat_session_index = state.chat_session_index.saturating_add(1);
                }
                TelegramInteractionMode::Coding => {
                    state.coding_session_index = state.coding_session_index.saturating_add(1);
                }
            }

            prepared_state = Some(state.clone());
            format!(
                "Started a new `{}` session: `{}`.",
                mode.as_str(),
                state.session_label_for(mode)
            )
        });

        if let Some(state) = prepared_state {
            Self::ensure_session_file(chat_id, state.interaction_mode, &state);
        }

        TelegramOutgoingMessage::plain(reply)
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
        let effective_workdir = state.effective_cli_workdir(cli_workdir);
        let backend_path = cli_backend_paths
            .get(&state.cli_backend)
            .map(|path| path.as_str())
            .unwrap_or("not found");
        let usage = state.usage_for(state.cli_backend);

        format!(
            "Telegram channel status:\n\
chat_id: `{}`\n\
interaction mode: `{}`\n\
active session: `{}`\n\
chat session: `{}`\n\
coding session: `{}`\n\
cli backend: `{}`\n\
execution mode: `{}`\n\
auto approve: `{}`\n\
cli binary: `{}`\n\
project directory: `{}`\n\
backend usage: {}\n\
backend auth: {}\n\
active handlers: `{}`\n\
backend requests: `{}`\n\
backend successes: `{}`\n\
backend failures: `{}`",
            chat_id,
            state.interaction_mode.as_str(),
            state.active_session_label(),
            state.session_label_for(TelegramInteractionMode::Chat),
            state.session_label_for(TelegramInteractionMode::Coding),
            state.cli_backend.as_str(),
            state.execution_mode.as_str(),
            if state.auto_approve { "on" } else { "off" },
            backend_path,
            effective_workdir.display(),
            Self::backend_usage_template(state.cli_backend, state.auto_approve),
            Self::backend_auth_hint(state.cli_backend),
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
    ) -> Option<TelegramOutgoingMessage> {
        let (command, args) = Self::parse_command(text)?;

        let reply = match command.as_str() {
            "start" | "help" => TelegramOutgoingMessage::plain(Self::supported_commands_text()),
            "select" => Self::set_interaction_mode(chat_states, state_path, chat_id, &args),
            "agent-cli" | "agent_cli" | "cli-backend" | "cli_backend" => {
                Self::set_cli_backend(chat_states, state_path, chat_id, &args, cli_backend_paths)
            }
            "project" => {
                Self::set_project_directory(chat_states, state_path, chat_id, &args, cli_workdir)
            }
            "new_session" => Self::start_new_session(chat_states, state_path, chat_id),
            "mode" => Self::set_execution_mode(chat_states, state_path, chat_id, &args),
            "auto-approve" | "auto_approve" => {
                Self::set_auto_approve(chat_states, state_path, chat_id, &args)
            }
            "usage" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                TelegramOutgoingMessage::plain(Self::format_usage_text(&state, state.cli_backend))
            }
            "status" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                TelegramOutgoingMessage::plain(Self::format_status_text(
                    chat_id,
                    &state,
                    cli_workdir,
                    cli_backend_paths,
                    active_handlers,
                ))
            }
            _ => TelegramOutgoingMessage::with_markup(
                format!(
                    "Unknown command `/{}`.\n\n{}",
                    command,
                    Self::supported_commands_text()
                ),
                Self::build_reply_keyboard(&[&["/help"]]),
            ),
        };

        Some(reply)
    }

    fn ensure_chat_state(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
    ) -> (TelegramChatState, bool) {
        let (state, snapshot, is_new) = match chat_states.lock() {
            Ok(mut states) => {
                if let Some(state) = states.get(&chat_id).cloned() {
                    (state, None, false)
                } else {
                    let state = TelegramChatState::default();
                    states.insert(chat_id, state.clone());
                    (state, Some(states.clone()), true)
                }
            }
            Err(err) => {
                log::warn!("TelegramClient: state lock poisoned while ensuring chat state: {}", err);
                (TelegramChatState::default(), None, false)
            }
        };

        if let Some(snapshot) = snapshot {
            Self::persist_chat_states(state_path, &snapshot);
        }

        if is_new {
            Self::ensure_session_file(chat_id, TelegramInteractionMode::Chat, &state);
            Self::ensure_session_file(chat_id, TelegramInteractionMode::Coding, &state);
        }

        (state, is_new)
    }

    fn build_connected_message(state: &TelegramChatState) -> TelegramOutgoingMessage {
        TelegramOutgoingMessage::plain(format!(
            "Telegram mobile connected.\nCurrent interaction mode: `{}`.\nCurrent session: `{}`.\nSelected CLI backend: `{}`.",
            state.interaction_mode.as_str(),
            state.active_session_label(),
            state.cli_backend.as_str()
        )
        )
    }

    fn build_startup_message(state: &TelegramChatState) -> TelegramOutgoingMessage {
        TelegramOutgoingMessage::plain(format!(
            "TizenClaw device started and Telegram channel is online.\nCurrent interaction mode: `{}`.\nCurrent session: `{}`.\nSelected CLI backend: `{}`.",
            state.interaction_mode.as_str(),
            state.active_session_label(),
            state.cli_backend.as_str()
        )
        )
    }

    fn startup_notification_targets(
        allowed_chat_ids: &Arc<HashSet<i64>>,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
    ) -> Vec<(i64, TelegramChatState)> {
        let mut snapshot = match chat_states.lock() {
            Ok(states) => states.clone(),
            Err(err) => {
                log::warn!(
                    "TelegramClient: state lock poisoned while gathering startup targets: {}",
                    err
                );
                HashMap::new()
            }
        };

        for chat_id in allowed_chat_ids.iter() {
            snapshot.entry(*chat_id).or_default();
        }

        let mut targets = snapshot.into_iter().collect::<Vec<_>>();
        targets.sort_by_key(|(chat_id, _)| *chat_id);
        targets
    }

    fn broadcast_startup_status(
        bot_token: &str,
        allowed_chat_ids: &Arc<HashSet<i64>>,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
    ) {
        for (chat_id, state) in Self::startup_notification_targets(allowed_chat_ids, chat_states) {
            let message = Self::build_startup_message(&state);
            Self::send_telegram_message(bot_token, chat_id, &message);
        }
    }

    fn build_cli_prompt(
        chat_id: i64,
        state: &TelegramChatState,
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
        let session_label = state.session_label_for(TelegramInteractionMode::Coding);
        let recent_context =
            Self::read_recent_session_excerpt(chat_id, TelegramInteractionMode::Coding, state, 5000);
        let history_block = if recent_context.trim().is_empty() {
            String::new()
        } else {
            format!(
                "\nCurrent Telegram coding session history ({})\n{}\n",
                session_label, recent_context
            )
        };

        format!(
            "{}\n\
\n\
Selected backend: {}\n\
Session: {}\n\
Project directory: {}\n\
\n\
{}\
User request:\n{}",
            mode_prefix,
            backend.as_str(),
            session_label,
            cli_workdir.display(),
            history_block,
            text.trim()
        )
    }

    fn build_cli_invocation(
        chat_id: i64,
        state: &TelegramChatState,
        effective_cli_workdir: &Path,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        cli_backend_models: &HashMap<TelegramCliBackend, String>,
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
            chat_id,
            &state,
            state.execution_mode,
            state.cli_backend,
            effective_cli_workdir,
            text,
        );
        let mut args = Vec::new();

        match state.cli_backend {
            TelegramCliBackend::Codex => {
                args.push("exec".to_string());
                args.push("--json".to_string());
                if state.auto_approve {
                    args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                } else {
                    args.push("--full-auto".to_string());
                }
                args.push("-C".to_string());
                args.push(effective_cli_workdir.to_string_lossy().to_string());
                args.push("--skip-git-repo-check".to_string());
                args.push(prompt);
            }
            TelegramCliBackend::Gemini => {
                if let Some(model) = cli_backend_models.get(&TelegramCliBackend::Gemini) {
                    args.push("--model".to_string());
                    args.push(model.clone());
                }
                if state.auto_approve {
                    args.push("-y".to_string());
                    args.push("--approval-mode".to_string());
                    args.push("yolo".to_string());
                } else {
                    args.push("--approval-mode".to_string());
                    args.push("auto_edit".to_string());
                }
                args.push("--prompt".to_string());
                args.push(prompt);
                args.push("--output-format".to_string());
                args.push("text".to_string());
            }
            TelegramCliBackend::Claude => {
                args.push("--print".to_string());
                args.push("--output-format".to_string());
                args.push("text".to_string());
                args.push("--permission-mode".to_string());
                args.push(if state.auto_approve {
                    "bypassPermissions".to_string()
                } else {
                    "auto".to_string()
                });
                args.push(prompt);
            }
        }

        Ok((binary, args))
    }

    fn build_cli_started_message(
        state: &TelegramChatState,
        backend: TelegramCliBackend,
        effective_cli_workdir: &Path,
    ) -> String {
        format!(
            "Started `{}` coding request.\nSession: `{}`.\nProject directory: `{}`.\nI will post progress updates while it runs.",
            backend.as_str(),
            state.session_label_for(TelegramInteractionMode::Coding),
            effective_cli_workdir.display()
        )
    }

    fn build_cli_progress_message(
        backend: TelegramCliBackend,
        effective_cli_workdir: &Path,
        elapsed_secs: u64,
        last_output_secs: Option<u64>,
    ) -> String {
        let activity = last_output_secs.map_or_else(
            || "No CLI output has been observed yet.".to_string(),
            |secs| format!("Last CLI output observed `{}` second(s) ago.", secs),
        );

        format!(
            "`{}` is still running.\nElapsed: `{}` second(s).\nProject directory: `{}`.\n{}",
            backend.as_str(),
            elapsed_secs,
            effective_cli_workdir.display(),
            activity
        )
    }

    fn build_cli_partial_response_message(
        backend: TelegramCliBackend,
        partial_text: &str,
    ) -> String {
        format!(
            "Partial `{}` response:\n\n{}",
            backend.as_str(),
            Self::truncate_chars(partial_text.trim(), 2800)
        )
    }

    fn extract_codex_json_response(stdout: &str) -> Option<String> {
        let mut messages = Vec::new();

        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if value.get("type").and_then(Value::as_str) != Some("item.completed") {
                continue;
            }
            let Some(item) = value.get("item") else {
                continue;
            };
            if item.get("type").and_then(Value::as_str) != Some("agent_message") {
                continue;
            }
            let Some(text) = item.get("text").and_then(Value::as_str) else {
                continue;
            };
            let text = text.trim();
            if !text.is_empty() {
                messages.push(text.to_string());
            }
        }

        if messages.is_empty() {
            None
        } else {
            Some(messages.join("\n\n"))
        }
    }

    fn extract_cli_response_text(
        backend: TelegramCliBackend,
        stdout: &str,
        stderr: &str,
    ) -> Option<String> {
        match backend {
            TelegramCliBackend::Codex => Self::extract_codex_json_response(stdout)
                .or_else(|| {
                    let text = stdout.trim();
                    (!text.is_empty()).then(|| text.to_string())
                })
                .or_else(|| {
                    let text = stderr.trim();
                    (!text.is_empty()).then(|| text.to_string())
                }),
            TelegramCliBackend::Gemini | TelegramCliBackend::Claude => {
                let text = stdout.trim();
                if !text.is_empty() {
                    Some(text.to_string())
                } else {
                    let text = stderr.trim();
                    (!text.is_empty()).then(|| text.to_string())
                }
            }
        }
    }

    fn extract_incremental_cli_response(
        backend: TelegramCliBackend,
        stdout: &str,
        stderr: &str,
        last_sent_text: &str,
    ) -> Option<String> {
        let current = Self::extract_cli_response_text(backend, stdout, stderr)?;
        let current = current.trim();
        if current.is_empty() || current == last_sent_text {
            return None;
        }

        let candidate = current
            .strip_prefix(last_sent_text)
            .map(str::trim)
            .filter(|delta| !delta.is_empty())
            .unwrap_or(current);
        let candidate_len = candidate.chars().count();

        let should_send = if last_sent_text.is_empty() {
            candidate_len >= CLI_PROGRESS_MIN_PARTIAL_CHARS
                || (candidate.contains('\n') && candidate_len >= 20)
        } else {
            candidate_len >= 40
                || (candidate.contains('\n') && candidate_len >= 20)
                || candidate.matches('\n').count() >= 2
        };

        should_send.then(|| candidate.to_string())
    }

    async fn read_cli_stream<R>(
        reader: R,
        is_stdout: bool,
        tx: tokio::sync::mpsc::UnboundedSender<TelegramCliStreamEvent>,
    ) where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let mut lines = tokio::io::BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let event = if is_stdout {
                        TelegramCliStreamEvent::StdoutLine(line)
                    } else {
                        TelegramCliStreamEvent::StderrLine(line)
                    };
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    let line = format!("stream read failed: {}", err);
                    let event = if is_stdout {
                        TelegramCliStreamEvent::StderrLine(line)
                    } else {
                        TelegramCliStreamEvent::StderrLine(line)
                    };
                    let _ = tx.send(event);
                    break;
                }
            }
        }
    }

    fn format_cli_result(
        backend: TelegramCliBackend,
        exit_code: i32,
        duration_ms: u64,
        stdout: &str,
        stderr: &str,
    ) -> String {
        if backend == TelegramCliBackend::Gemini {
            let combined = format!("{}\n{}", stdout, stderr);
            if combined.contains("Opening authentication page in your browser") {
                return "Gemini CLI requires interactive host login before Telegram can use it. Run `gemini` once on the host and finish authentication, then retry.".to_string();
            }
            if combined.contains("MODEL_CAPACITY_EXHAUSTED")
                || combined.contains("No capacity available for model")
                || combined.contains("\"status\": \"RESOURCE_EXHAUSTED\"")
            {
                return "Gemini CLI hit a model-capacity limit on the host. Telegram now supports an explicit Gemini model; use a stable model such as `gemini-2.5-flash` in the host config and retry.".to_string();
            }
        }

        if exit_code == 0 {
            if let Some(text) = Self::extract_cli_response_text(backend, stdout, stderr) {
                return Self::truncate_chars(text.trim(), 3400);
            }

            return format!(
                "`{}` completed successfully in `{}` ms, but no response text was captured.",
                backend.as_str(),
                duration_ms
            );
        }

        let body = Self::extract_cli_response_text(backend, stdout, stderr)
            .unwrap_or_else(|| "CLI failed with no output.".to_string());

        format!(
            "`{}` finished with exit code `{}` in `{}` ms.\n\n{}",
            backend.as_str(),
            exit_code,
            duration_ms,
            Self::truncate_chars(body.trim(), 3400)
        )
    }

    async fn execute_cli_request(
        bot_token: &str,
        chat_id: i64,
        text: &str,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        cli_backend_models: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
    ) -> String {
        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        let backend = state.cli_backend;
        let started_at = Self::current_timestamp_millis();
        let effective_cli_workdir = state.effective_cli_workdir(&cli_workdir);

        let invocation =
            match Self::build_cli_invocation(
                chat_id,
                &state,
                &effective_cli_workdir,
                &cli_backend_paths,
                &cli_backend_models,
                text,
            ) {
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
        command.current_dir(&effective_cli_workdir);
        command.env("NO_COLOR", "1");
        command.env("CLICOLOR", "0");
        command.env("TERM", "dumb");
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.kill_on_drop(true);

        let mut child = match command.spawn() {
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

        Self::send_telegram_message(
            bot_token,
            chat_id,
            &TelegramOutgoingMessage::plain(Self::build_cli_started_message(
                &state,
                backend,
                &effective_cli_workdir,
            )),
        );

        let Some(stdout_reader) = child.stdout.take() else {
            return format!("Failed to capture `{}` stdout.", backend.as_str());
        };
        let Some(stderr_reader) = child.stderr.take() else {
            return format!("Failed to capture `{}` stderr.", backend.as_str());
        };

        let started = Instant::now();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(Self::read_cli_stream(stdout_reader, true, tx.clone()));
        tokio::spawn(Self::read_cli_stream(stderr_reader, false, tx));

        let execution = async {
            let mut child = child;
            let wait_fut = child.wait();
            tokio::pin!(wait_fut);
            let mut heartbeat =
                tokio::time::interval(Duration::from_secs(CLI_PROGRESS_UPDATE_SECS));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            heartbeat.tick().await;

            let mut stdout = String::new();
            let mut stderr = String::new();
            let mut last_partial_text = String::new();
            let mut last_output_at = None::<Instant>;
            let mut child_status = None;

            loop {
                tokio::select! {
                    status = &mut wait_fut, if child_status.is_none() => {
                        child_status = Some(status);
                    }
                    maybe_event = rx.recv() => {
                        match maybe_event {
                            Some(TelegramCliStreamEvent::StdoutLine(line)) => {
                                if !stdout.is_empty() {
                                    stdout.push('\n');
                                }
                                stdout.push_str(&line);
                                last_output_at = Some(Instant::now());
                            }
                            Some(TelegramCliStreamEvent::StderrLine(line)) => {
                                if !stderr.is_empty() {
                                    stderr.push('\n');
                                }
                                stderr.push_str(&line);
                                last_output_at = Some(Instant::now());
                            }
                            None => {
                                if child_status.is_some() {
                                    break;
                                }
                            }
                        }

                        if let Some(partial) = Self::extract_incremental_cli_response(
                            backend,
                            &stdout,
                            &stderr,
                            &last_partial_text,
                        ) {
                            Self::send_telegram_message(
                                bot_token,
                                chat_id,
                                &TelegramOutgoingMessage::plain(
                                    Self::build_cli_partial_response_message(
                                        backend,
                                        &partial,
                                    ),
                                ),
                            );
                            if let Some(current_text) =
                                Self::extract_cli_response_text(backend, &stdout, &stderr)
                            {
                                last_partial_text = current_text.trim().to_string();
                            }
                        }
                    }
                    _ = heartbeat.tick() => {
                        if child_status.is_none() {
                            let elapsed_secs = started.elapsed().as_secs();
                            let last_output_secs = last_output_at.map(|instant| instant.elapsed().as_secs());
                            Self::send_telegram_message(
                                bot_token,
                                chat_id,
                                &TelegramOutgoingMessage::plain(
                                    Self::build_cli_progress_message(
                                        backend,
                                        &effective_cli_workdir,
                                        elapsed_secs,
                                        last_output_secs,
                                    ),
                                ),
                            );
                        }
                    }
                }
            }

            let output = match child_status {
                Some(status) => status?,
                None => wait_fut.await?,
            };
            Ok::<_, std::io::Error>((output, stdout, stderr))
        };

        let timed_output = tokio::time::timeout(Duration::from_secs(cli_timeout_secs), execution).await;

        match timed_output {
            Ok(Ok((status, stdout, stderr))) => {
                let duration_ms = started.elapsed().as_millis() as u64;
                let exit_code = status.code().unwrap_or(-1);
                let success = status.success();

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
        bot_token: &str,
        chat_id: i64,
        text: &str,
        agent: Option<Arc<crate::core::agent_core::AgentCore>>,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        cli_backend_models: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
        active_handlers: i32,
    ) -> Vec<TelegramOutgoingMessage> {
        let (state, is_new_chat) = Self::ensure_chat_state(&chat_states, &state_path, chat_id);
        let mut replies = Vec::new();

        if is_new_chat {
            replies.push(Self::build_connected_message(&state));
        }

        if let Some(reply) = Self::handle_command(
            chat_id,
            text,
            &chat_states,
            &state_path,
            &cli_backend_paths,
            &cli_workdir,
            active_handlers,
        ) {
            replies.push(reply);
            return replies;
        }

        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        match state.interaction_mode {
            TelegramInteractionMode::Chat => {
                let Some(agent_core) = agent else {
                    replies.push(TelegramOutgoingMessage::plain(
                        "AgentCore is not available for chat mode.",
                    ));
                    return replies;
                };
                let session_id = format!(
                    "tg_{}_{}",
                    chat_id,
                    state.session_label_for(TelegramInteractionMode::Chat)
                );
                let response = agent_core.process_prompt(&session_id, text, None).await;
                Self::append_session_transcript(
                    chat_id,
                    TelegramInteractionMode::Chat,
                    &state,
                    text,
                    &response,
                );
                replies.push(TelegramOutgoingMessage::plain(response));
                replies
            }
            TelegramInteractionMode::Coding => {
                let response = Self::execute_cli_request(
                    bot_token,
                    chat_id,
                    text,
                    cli_workdir,
                    cli_timeout_secs,
                    cli_backend_paths,
                    cli_backend_models,
                    chat_states,
                    state_path,
                )
                .await;
                Self::append_session_transcript(
                    chat_id,
                    TelegramInteractionMode::Coding,
                    &state,
                    text,
                    &response,
                );
                replies.push(TelegramOutgoingMessage::plain(response));
                replies
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
        Self::register_bot_commands(&self.bot_token);

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let bot_token = self.bot_token.clone();
        let allowed_ids = self.allowed_chat_ids.clone();
        let active_handlers = self.active_handlers.clone();
        let agent = self.agent.clone();
        let cli_workdir = self.cli_workdir.clone();
        let cli_timeout_secs = self.cli_timeout_secs;
        let cli_backend_paths = self.cli_backend_paths.clone();
        let cli_backend_models = self.cli_backend_models.clone();
        let chat_states = self.chat_states.clone();
        let chat_state_path = self.chat_state_path.clone();
        let last_user_input = self.last_user_input.clone();

        Self::broadcast_startup_status(&self.bot_token, &self.allowed_chat_ids, &self.chat_states);

        // Idle-trim background task: when no user input for 3 minutes, release
        // free heap pages back to the OS via malloc_trim(0).
        {
            const IDLE_TRIM_SECS: u64 = 180;
            const CHECK_INTERVAL_SECS: u64 = 30;
            let running_trim = running.clone();
            let last_input_trim = last_user_input.clone();
            tokio::spawn(async move {
                let mut trimmed_at: u64 = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
                    if !running_trim.load(Ordering::SeqCst) {
                        break;
                    }
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let last = last_input_trim.load(Ordering::Relaxed);
                    // Trim once per idle window; don't repeat until next message arrives.
                    if now.saturating_sub(last) >= IDLE_TRIM_SECS && last != trimmed_at {
                        unsafe { libc::malloc_trim(0) };
                        trimmed_at = last;
                        log::info!(
                            "TelegramClient: idle {}s — malloc_trim(0) executed",
                            now.saturating_sub(last)
                        );
                    }
                }
            });
        }

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

                        // Record activity time to reset the idle-trim window.
                        let now_secs = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        last_user_input.store(now_secs, Ordering::Relaxed);

                        active_handlers.fetch_add(1, Ordering::SeqCst);
                        let text_clone = text.to_string();
                        let bot_token_clone = bot_token.clone();
                        let agent_clone = agent.clone();
                        let active_handlers_clone = active_handlers.clone();
                        let cli_workdir_clone = cli_workdir.clone();
                        let cli_backend_paths_clone = cli_backend_paths.clone();
                        let cli_backend_models_clone = cli_backend_models.clone();
                        let chat_states_clone = chat_states.clone();
                        let chat_state_path_clone = chat_state_path.clone();

                        tokio::spawn(async move {
                            let results = TelegramClient::route_message(
                                &bot_token_clone,
                                chat_id,
                                &text_clone,
                                agent_clone,
                                cli_workdir_clone,
                                cli_timeout_secs,
                                cli_backend_paths_clone,
                                cli_backend_models_clone,
                                chat_states_clone,
                                chat_state_path_clone,
                                current_handlers + 1,
                            )
                            .await;
                            for result in results {
                                TelegramClient::send_telegram_message(
                                    &bot_token_clone,
                                    chat_id,
                                    &result,
                                );
                            }
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
            Self::send_telegram_message(
                &self.bot_token,
                *chat_id,
                &TelegramOutgoingMessage::plain(msg),
            );
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
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};

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
        assert_eq!(state.session_label_for(TelegramInteractionMode::Chat), "chat-0001");
        assert_eq!(
            state.session_label_for(TelegramInteractionMode::Coding),
            "coding-0001"
        );
    }

    #[test]
    fn send_message_payload_is_plain_text_json() {
        let payload =
            TelegramClient::build_send_message_payload(123, "value_with`markdown`", None);
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["chat_id"], 123);
        assert_eq!(json["text"], "value_with`markdown`");
        assert!(json.get("parse_mode").is_none());
        assert!(json.get("reply_markup").is_none());
    }

    #[test]
    fn supported_commands_text_uses_agent_cli_name() {
        let help = TelegramClient::supported_commands_text();
        assert!(help.contains("/agent_cli <codex|gemini|claude>"));
        assert!(help.contains("/auto_approve <on|off>"));
        assert!(help.contains("/project <path>"));
        assert!(help.contains("/new_session - start a fresh session for the current mode"));
        assert!(!help.contains("/cli_backend <codex|gemini|claude>"));
        assert!(!help.contains("/cli-backend <codex|gemini|claude>"));
        assert!(!help.contains("/auto-approve <on|off>"));
    }

    #[test]
    fn set_my_commands_payload_contains_expected_commands() {
        let payload = TelegramClient::build_set_my_commands_payload();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let commands = json["commands"].as_array().unwrap();
        let names: Vec<&str> = commands
            .iter()
            .filter_map(|entry| entry["command"].as_str())
            .collect();

        assert_eq!(
            names,
            vec![
                "select",
                "agent_cli",
                "project",
                "new_session",
                "usage",
                "mode",
                "status",
                "auto_approve"
            ]
        );
    }

    #[test]
    fn build_send_message_payload_can_include_reply_markup() {
        let payload = TelegramClient::build_send_message_payload(
            123,
            "pick one",
            Some(TelegramClient::mode_keyboard()),
        );
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["chat_id"], 123);
        assert_eq!(json["text"], "pick one");
        assert_eq!(json["reply_markup"]["one_time_keyboard"], true);
        assert_eq!(json["reply_markup"]["keyboard"][0][0], "/mode plan");
        assert_eq!(json["reply_markup"]["keyboard"][0][1], "/mode fast");
    }

    #[test]
    fn agent_cli_keyboard_uses_new_command_name() {
        let keyboard = TelegramClient::cli_backend_keyboard();
        assert_eq!(keyboard["keyboard"][0][0], "/agent_cli codex");
        assert_eq!(keyboard["keyboard"][1][0], "/agent_cli gemini");
        assert_eq!(keyboard["keyboard"][2][0], "/agent_cli claude");
    }

    #[test]
    fn connected_message_mentions_current_mode() {
        let message = TelegramClient::build_connected_message(&TelegramChatState::default());
        assert!(message.text.contains("Telegram mobile connected."));
        assert!(message.text.contains("Current interaction mode: `chat`"));
        assert!(message.text.contains("Current session: `chat-0001`"));
        assert!(message.reply_markup.is_none());
    }

    #[test]
    fn startup_message_mentions_current_mode() {
        let message = TelegramClient::build_startup_message(&TelegramChatState::default());
        assert!(message.text.contains("TizenClaw device started and Telegram channel is online."));
        assert!(message.text.contains("Current session: `chat-0001`"));
        assert!(message.reply_markup.is_none());
    }

    #[test]
    fn select_without_args_shows_only_select_submenu() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_select_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let reply = TelegramClient::handle_command(
            77,
            "/select",
            &chat_states,
            &state_path,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Choose the interaction mode"));
        assert_eq!(reply.reply_markup.as_ref().unwrap()["keyboard"][0][0], "/select chat");
        assert_eq!(reply.reply_markup.as_ref().unwrap()["keyboard"][0][1], "/select coding");
    }

    #[test]
    fn project_without_args_reports_current_directory() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_project_status_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let reply = TelegramClient::handle_command(
            77,
            "/project",
            &chat_states,
            &state_path,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Current project directory: `/tmp`"));
    }

    #[test]
    fn project_command_updates_chat_state() {
        let project_dir = std::env::temp_dir();
        let project_text = project_dir.display().to_string();
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_project_set_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let command = format!("/project {}", project_text);

        let reply = TelegramClient::handle_command(
            77,
            &command,
            &chat_states,
            &state_path,
            &HashMap::new(),
            std::path::Path::new("/work"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Project directory set to"));
        let state = TelegramClient::load_chat_state_snapshot(&chat_states, 77);
        let expected = project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(
            state.project_dir.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn agent_cli_command_and_legacy_alias_both_route_to_backend_selection() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_agent_cli_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let backend_paths = HashMap::from([(
            TelegramCliBackend::Claude,
            "/usr/bin/claude".to_string(),
        )]);

        let new_reply = TelegramClient::handle_command(
            77,
            "/agent_cli claude",
            &chat_states,
            &state_path,
            &backend_paths,
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(new_reply.text.contains("CLI backend set to `claude`."));

        let legacy_reply = TelegramClient::handle_command(
            77,
            "/cli_backend codex",
            &chat_states,
            &state_path,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(legacy_reply.text.contains("CLI backend set to `codex`."));
    }

    #[test]
    fn extract_codex_json_response_reads_agent_message() {
        let output = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\",\"type\":\"agent_message\",\"text\":\"HELLO\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1}}\n"
        );

        assert_eq!(
            TelegramClient::extract_codex_json_response(output).as_deref(),
            Some("HELLO")
        );
    }

    #[test]
    fn cli_started_message_mentions_session_and_project() {
        let state = TelegramChatState::default();
        let message = TelegramClient::build_cli_started_message(
            &state,
            TelegramCliBackend::Codex,
            std::path::Path::new("/tmp/project"),
        );

        assert!(message.contains("Started `codex` coding request."));
        assert!(message.contains("Session: `coding-0001`."));
        assert!(message.contains("Project directory: `/tmp/project`."));
    }

    #[test]
    fn cli_progress_message_reports_output_state() {
        let idle = TelegramClient::build_cli_progress_message(
            TelegramCliBackend::Codex,
            std::path::Path::new("/tmp/project"),
            15,
            None,
        );
        assert!(idle.contains("`codex` is still running."));
        assert!(idle.contains("Elapsed: `15` second(s)."));
        assert!(idle.contains("No CLI output has been observed yet."));

        let active = TelegramClient::build_cli_progress_message(
            TelegramCliBackend::Claude,
            std::path::Path::new("/tmp/project"),
            22,
            Some(3),
        );
        assert!(active.contains("`claude` is still running."));
        assert!(active.contains("Last CLI output observed `3` second(s) ago."));
    }

    #[test]
    fn incremental_cli_response_uses_new_text_delta() {
        let stdout = "First line of output\nSecond line of output with enough detail";
        let partial = TelegramClient::extract_incremental_cli_response(
            TelegramCliBackend::Claude,
            stdout,
            "",
            "",
        )
        .unwrap();
        assert!(partial.contains("First line of output"));

        let next_stdout = format!(
            "{}\nThird line extends the response with more useful detail",
            stdout
        );
        let partial = TelegramClient::extract_incremental_cli_response(
            TelegramCliBackend::Claude,
            &next_stdout,
            "",
            stdout,
        )
        .unwrap();
        assert!(partial.contains("Third line extends the response"));
    }

    #[test]
    fn codex_invocation_uses_json_mode_and_project_directory() {
        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: TelegramCliBackend::Codex,
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let backend_paths = HashMap::from([(
            TelegramCliBackend::Codex,
            "/usr/bin/codex".to_string(),
        )]);
        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &backend_paths,
            &HashMap::new(),
            "hello",
        )
        .unwrap();

        assert_eq!(binary, "/usr/bin/codex");
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(args.iter().any(|arg| arg == "--full-auto"));
        assert!(args.iter().any(|arg| arg == "--skip-git-repo-check"));
        let cd_index = args.iter().position(|arg| arg == "-C").unwrap();
        assert_eq!(args[cd_index + 1], "/tmp/project");
    }

    #[test]
    fn gemini_invocation_uses_explicit_model() {
        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: TelegramCliBackend::Gemini,
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let backend_paths = HashMap::from([(
            TelegramCliBackend::Gemini,
            "/snap/bin/gemini".to_string(),
        )]);
        let backend_models = HashMap::from([(
            TelegramCliBackend::Gemini,
            "gemini-2.5-flash".to_string(),
        )]);

        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &backend_paths,
            &backend_models,
            "hello",
        )
        .unwrap();

        assert_eq!(binary, "/snap/bin/gemini");
        let model_index = args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(args[model_index + 1], "gemini-2.5-flash");
        assert!(args.iter().any(|arg| arg == "--prompt"));
        assert!(args.iter().any(|arg| arg == "--output-format"));
    }

    #[test]
    fn gemini_capacity_errors_are_summarized() {
        let message = TelegramClient::format_cli_result(
            TelegramCliBackend::Gemini,
            1,
            100,
            "",
            "No capacity available for model gemini-3-flash-preview",
        );

        assert!(message.contains("model-capacity limit"));
        assert!(message.contains("gemini-2.5-flash"));
    }

    #[test]
    fn llm_config_gemini_model_is_used_as_fallback() {
        let temp_root = std::env::temp_dir().join(format!(
            "telegram_gemini_model_{}_{}",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        std::fs::write(
            temp_root.join("llm_config.json"),
            r#"{"backends":{"gemini":{"model":"gemini-2.5-pro"}}}"#,
        )
        .unwrap();

        let mut models = HashMap::new();
        TelegramClient::read_backend_models_from_llm_config(&temp_root, &mut models);

        assert_eq!(
            models.get(&TelegramCliBackend::Gemini).map(String::as_str),
            Some("gemini-2.5-pro")
        );

        let _ = std::fs::remove_file(temp_root.join("llm_config.json"));
        let _ = std::fs::remove_dir(&temp_root);
    }

    #[test]
    fn startup_targets_include_allowed_chat_ids_without_saved_state() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let allowed = Arc::new(HashSet::from([12345_i64]));
        let targets = TelegramClient::startup_notification_targets(&allowed, &chat_states);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, 12345);
        assert_eq!(
            targets[0].1.session_label_for(TelegramInteractionMode::Chat),
            "chat-0001"
        );
    }

    #[test]
    fn new_session_increments_current_mode_counter() {
        let state_path = std::env::temp_dir().join(format!(
            "telegram_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let chat_states = Arc::new(Mutex::new(HashMap::new()));

        let first = TelegramClient::start_new_session(&chat_states, &state_path, 77);
        assert!(first.text.contains("Started a new `chat` session: `chat-0002`."));

        {
            let mut states = chat_states.lock().unwrap();
            let state = states.entry(77).or_default();
            state.interaction_mode = TelegramInteractionMode::Coding;
        }

        let second = TelegramClient::start_new_session(&chat_states, &state_path, 77);
        assert!(second
            .text
            .contains("Started a new `coding` session: `coding-0002`."));
    }
}
