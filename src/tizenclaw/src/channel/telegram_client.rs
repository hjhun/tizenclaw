//! Telegram Bot API client — async long-polling channel.
//!
//! Uses `getUpdates` long-polling to receive messages. Polls natively
//! on the Tokio async reactor (epoll) avoiding expensive thread allocation.

use super::{Channel, ChannelConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncBufReadExt;

const MAX_CONCURRENT_HANDLERS: i32 = 3;
const DEFAULT_CLI_TIMEOUT_SECS: u64 = 900;
const TELEGRAM_CHAT_ACTION_UPDATE_SECS: u64 = 4;
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

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct TelegramCliBackend(String);

impl Default for TelegramCliBackend {
    fn default() -> Self {
        Self::new("codex")
    }
}

impl TelegramCliBackend {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    fn normalized(value: &str) -> String {
        value.trim().to_ascii_lowercase()
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelegramCliOutputSource {
    #[default]
    Stdout,
    Stderr,
    Combined,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelegramCliOutputFormat {
    #[default]
    Json,
    JsonLines,
    PlainText,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TelegramCliInvocationTemplate {
    args: Vec<String>,
    approval_placeholder: Option<String>,
    default_approval_value: Option<String>,
    auto_approve_value: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TelegramCliResponseExtractor {
    source: TelegramCliOutputSource,
    format: TelegramCliOutputFormat,
    match_fields: HashMap<String, String>,
    text_path: Option<String>,
    join_matches: bool,
    reject_json_input: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TelegramCliUsageExtractor {
    source: TelegramCliOutputSource,
    format: TelegramCliOutputFormat,
    match_fields: HashMap<String, String>,
    input_tokens_path: Option<String>,
    output_tokens_path: Option<String>,
    total_tokens_path: Option<String>,
    cached_input_tokens_path: Option<String>,
    cache_creation_input_tokens_path: Option<String>,
    cache_read_input_tokens_path: Option<String>,
    thought_tokens_path: Option<String>,
    tool_tokens_path: Option<String>,
    model_path: Option<String>,
    model_key_path: Option<String>,
    session_id_path: Option<String>,
    remaining_text_path: Option<String>,
    reset_at_path: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TelegramCliErrorHint {
    source: TelegramCliOutputSource,
    patterns: Vec<String>,
    message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TelegramCliModelChoice {
    value: String,
    label: Option<String>,
    description: Option<String>,
}

impl TelegramCliModelChoice {
    fn simple(value: &str) -> Self {
        Self {
            value: value.to_string(),
            label: None,
            description: None,
        }
    }

    fn detailed(value: &str, label: &str, description: &str) -> Self {
        Self {
            value: value.to_string(),
            label: Some(label.to_string()),
            description: Some(description.to_string()),
        }
    }

    fn normalized_value(&self) -> String {
        TelegramCliBackend::normalized(&self.value)
    }

    fn summary_text(&self) -> String {
        match self.label.as_deref() {
            Some(label) if label.trim() != self.value.trim() => {
                format!("{} -> {}", label.trim(), self.value.trim())
            }
            _ => self.value.trim().to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TelegramCliBackendDefinition {
    display_name: Option<String>,
    aliases: Vec<String>,
    binary_candidates: Vec<String>,
    binary_path: Option<String>,
    model: Option<String>,
    auth_hint: String,
    usage_hint: String,
    auto_approve_usage_hint: Option<String>,
    model_choices_source_label: String,
    model_choices: Vec<TelegramCliModelChoice>,
    usage_source_label: String,
    usage_refresh_hint: Option<String>,
    remaining_usage_hint: Option<String>,
    reset_usage_hint: Option<String>,
    invocation: TelegramCliInvocationTemplate,
    response_extractors: Vec<TelegramCliResponseExtractor>,
    usage_extractors: Vec<TelegramCliUsageExtractor>,
    error_hints: Vec<TelegramCliErrorHint>,
}

#[derive(Clone, Debug)]
struct TelegramCliBackendRegistry {
    order: Vec<TelegramCliBackend>,
    definitions: HashMap<TelegramCliBackend, TelegramCliBackendDefinition>,
    aliases: HashMap<String, TelegramCliBackend>,
    default_backend: TelegramCliBackend,
}

impl Default for TelegramCliBackendRegistry {
    fn default() -> Self {
        let mut registry = Self {
            order: Vec::new(),
            definitions: HashMap::new(),
            aliases: HashMap::new(),
            default_backend: TelegramCliBackend::default(),
        };

        registry.insert_builtin(
            "codex",
            TelegramCliBackendDefinition {
                display_name: Some("Codex".to_string()),
                aliases: vec!["codex".to_string()],
                binary_candidates: vec!["codex".to_string()],
                auth_hint: "Codex CLI must already be logged in on the host.".to_string(),
                usage_hint:
                    "`codex exec --json --full-auto -C <project> <prompt>`".to_string(),
                auto_approve_usage_hint: Some(
                    "`codex exec --json --dangerously-bypass-approvals-and-sandbox -C <project> <prompt>`"
                        .to_string(),
                ),
                model_choices_source_label:
                    "curated Codex-compatible model choices".to_string(),
                model_choices: vec![
                    TelegramCliModelChoice::detailed(
                        "gpt-5.4",
                        "gpt-5.4",
                        "Balanced default for strong coding quality",
                    ),
                    TelegramCliModelChoice::detailed(
                        "gpt-5.3-codex",
                        "gpt-5.3-codex",
                        "Dedicated Codex-family coding model",
                    ),
                    TelegramCliModelChoice::detailed(
                        "gpt-5-codex",
                        "gpt-5-codex",
                        "Stable Codex-family override",
                    ),
                    TelegramCliModelChoice::detailed(
                        "codex-mini-latest",
                        "codex-mini-latest",
                        "Lower-cost Codex-family option",
                    ),
                ],
                usage_source_label: "turn.completed.usage".to_string(),
                usage_refresh_hint: Some(
                    "updates after the next successful Codex run".to_string(),
                ),
                remaining_usage_hint: Some("not reported by Codex CLI".to_string()),
                reset_usage_hint: Some("not reported by Codex CLI".to_string()),
                invocation: TelegramCliInvocationTemplate {
                    args: vec![
                        "exec".to_string(),
                        "--json".to_string(),
                        "{approval_mode}".to_string(),
                        "{model_args}".to_string(),
                        "-C".to_string(),
                        "{project_dir}".to_string(),
                        "--skip-git-repo-check".to_string(),
                        "{prompt}".to_string(),
                    ],
                    approval_placeholder: Some("{approval_mode}".to_string()),
                    default_approval_value: Some("--full-auto".to_string()),
                    auto_approve_value: Some(
                        "--dangerously-bypass-approvals-and-sandbox".to_string(),
                    ),
                },
                response_extractors: vec![TelegramCliResponseExtractor {
                    source: TelegramCliOutputSource::Stdout,
                    format: TelegramCliOutputFormat::JsonLines,
                    match_fields: HashMap::from([
                        ("type".to_string(), "item.completed".to_string()),
                        ("item.type".to_string(), "agent_message".to_string()),
                    ]),
                    text_path: Some("item.text".to_string()),
                    join_matches: true,
                    reject_json_input: false,
                }],
                usage_extractors: vec![TelegramCliUsageExtractor {
                    source: TelegramCliOutputSource::Stdout,
                    format: TelegramCliOutputFormat::JsonLines,
                    match_fields: HashMap::from([(
                        "type".to_string(),
                        "turn.completed".to_string(),
                    )]),
                    input_tokens_path: Some("usage.input_tokens".to_string()),
                    output_tokens_path: Some("usage.output_tokens".to_string()),
                    cached_input_tokens_path: Some("usage.cached_input_tokens".to_string()),
                    ..TelegramCliUsageExtractor::default()
                }],
                ..TelegramCliBackendDefinition::default()
            },
        );

        registry.insert_builtin(
            "gemini",
            TelegramCliBackendDefinition {
                display_name: Some("Gemini".to_string()),
                aliases: vec!["gemini".to_string()],
                binary_candidates: vec!["gemini".to_string(), "/snap/bin/gemini".to_string()],
                model: Some(DEFAULT_GEMINI_CLI_MODEL.to_string()),
                auth_hint: "Gemini CLI must be authenticated on the host before Telegram can use it non-interactively.".to_string(),
                usage_hint: "`gemini --model <model> --prompt <prompt> --output-format json --approval-mode auto_edit`".to_string(),
                auto_approve_usage_hint: Some(
                    "`gemini --model <model> --prompt <prompt> --output-format json -y --approval-mode yolo`"
                        .to_string(),
                ),
                model_choices_source_label:
                    "Gemini CLI aliases and documented model names".to_string(),
                model_choices: vec![
                    TelegramCliModelChoice::detailed(
                        "auto",
                        "auto",
                        "Gemini CLI default routing alias",
                    ),
                    TelegramCliModelChoice::detailed(
                        "pro",
                        "pro",
                        "Alias for the stronger reasoning tier",
                    ),
                    TelegramCliModelChoice::detailed(
                        "flash",
                        "flash",
                        "Alias for the fast balanced tier",
                    ),
                    TelegramCliModelChoice::detailed(
                        "flash-lite",
                        "flash-lite",
                        "Alias for the lightest Gemini tier",
                    ),
                    TelegramCliModelChoice::simple("gemini-2.5-pro"),
                    TelegramCliModelChoice::simple("gemini-2.5-flash"),
                    TelegramCliModelChoice::simple("gemini-2.5-flash-lite"),
                    TelegramCliModelChoice::simple("gemini-3-pro-preview"),
                ],
                usage_source_label: "stats.models.<model>.tokens".to_string(),
                usage_refresh_hint: Some(
                    "updates after the next successful Gemini run".to_string(),
                ),
                remaining_usage_hint: Some("not reported by Gemini CLI".to_string()),
                reset_usage_hint: Some("not reported by Gemini CLI".to_string()),
                invocation: TelegramCliInvocationTemplate {
                    args: vec![
                        "{model_args}".to_string(),
                        "{approval_mode}".to_string(),
                        "--prompt".to_string(),
                        "{prompt}".to_string(),
                        "--output-format".to_string(),
                        "json".to_string(),
                    ],
                    approval_placeholder: Some("{approval_mode}".to_string()),
                    default_approval_value: Some("--approval-mode auto_edit".to_string()),
                    auto_approve_value: Some("-y --approval-mode yolo".to_string()),
                },
                response_extractors: vec![
                    TelegramCliResponseExtractor {
                        source: TelegramCliOutputSource::Stdout,
                        format: TelegramCliOutputFormat::Json,
                        match_fields: HashMap::new(),
                        text_path: Some("response".to_string()),
                        join_matches: false,
                        reject_json_input: false,
                    },
                    TelegramCliResponseExtractor {
                        source: TelegramCliOutputSource::Stdout,
                        format: TelegramCliOutputFormat::PlainText,
                        match_fields: HashMap::new(),
                        text_path: None,
                        join_matches: false,
                        reject_json_input: true,
                    },
                    TelegramCliResponseExtractor {
                        source: TelegramCliOutputSource::Stderr,
                        format: TelegramCliOutputFormat::PlainText,
                        match_fields: HashMap::new(),
                        text_path: None,
                        join_matches: false,
                        reject_json_input: false,
                    },
                ],
                usage_extractors: vec![TelegramCliUsageExtractor {
                    source: TelegramCliOutputSource::Stdout,
                    format: TelegramCliOutputFormat::Json,
                    match_fields: HashMap::new(),
                    input_tokens_path: Some("stats.models.@first_value.tokens.input".to_string()),
                    output_tokens_path: Some(
                        "stats.models.@first_value.tokens.candidates".to_string(),
                    ),
                    total_tokens_path: Some("stats.models.@first_value.tokens.total".to_string()),
                    cached_input_tokens_path: Some(
                        "stats.models.@first_value.tokens.cached".to_string(),
                    ),
                    thought_tokens_path: Some(
                        "stats.models.@first_value.tokens.thoughts".to_string(),
                    ),
                    tool_tokens_path: Some("stats.models.@first_value.tokens.tool".to_string()),
                    model_key_path: Some("stats.models".to_string()),
                    session_id_path: Some("session_id".to_string()),
                    ..TelegramCliUsageExtractor::default()
                }],
                error_hints: vec![
                    TelegramCliErrorHint {
                        source: TelegramCliOutputSource::Combined,
                        patterns: vec![
                            "Opening authentication page in your browser".to_string(),
                        ],
                        message: "[gemini] Login required on the host.\nRun `gemini` once, finish authentication, then retry.".to_string(),
                    },
                    TelegramCliErrorHint {
                        source: TelegramCliOutputSource::Combined,
                        patterns: vec![
                            "MODEL_CAPACITY_EXHAUSTED".to_string(),
                            "No capacity available for model".to_string(),
                            "\"status\": \"RESOURCE_EXHAUSTED\"".to_string(),
                        ],
                        message: "[gemini] Model capacity reached.\nTry a stable model such as `gemini-2.5-flash` and retry.".to_string(),
                    },
                ],
                ..TelegramCliBackendDefinition::default()
            },
        );

        registry.insert_builtin(
            "claude",
            TelegramCliBackendDefinition {
                display_name: Some("Claude".to_string()),
                aliases: vec![
                    "claude".to_string(),
                    "claude-code".to_string(),
                    "claude_code".to_string(),
                ],
                binary_candidates: vec!["claude".to_string(), "claude-code".to_string()],
                auth_hint: "Claude Code must already be authenticated on the host.".to_string(),
                usage_hint:
                    "`claude --print --output-format json --permission-mode auto <prompt>`"
                        .to_string(),
                auto_approve_usage_hint: Some(
                    "`claude --print --output-format json --permission-mode bypassPermissions <prompt>`"
                        .to_string(),
                ),
                model_choices_source_label:
                    "Claude Code aliases and common concrete model names"
                        .to_string(),
                model_choices: vec![
                    TelegramCliModelChoice::detailed(
                        "sonnet",
                        "sonnet",
                        "Claude Code alias for the latest Sonnet line",
                    ),
                    TelegramCliModelChoice::detailed(
                        "opus",
                        "opus",
                        "Claude Code alias for the latest Opus line",
                    ),
                    TelegramCliModelChoice::detailed(
                        "haiku",
                        "haiku",
                        "Claude Code alias for the lightweight tier",
                    ),
                    TelegramCliModelChoice::simple("claude-sonnet-4-6"),
                    TelegramCliModelChoice::simple("claude-opus-4-1"),
                ],
                usage_source_label: "usage + modelUsage".to_string(),
                usage_refresh_hint: Some(
                    "updates after the next successful Claude run".to_string(),
                ),
                remaining_usage_hint: Some("not reported by Claude CLI".to_string()),
                reset_usage_hint: Some("not reported by Claude CLI".to_string()),
                invocation: TelegramCliInvocationTemplate {
                    args: vec![
                        "--print".to_string(),
                        "--output-format".to_string(),
                        "json".to_string(),
                        "{model_args}".to_string(),
                        "--permission-mode".to_string(),
                        "{approval_mode}".to_string(),
                        "{prompt}".to_string(),
                    ],
                    approval_placeholder: Some("{approval_mode}".to_string()),
                    default_approval_value: Some("auto".to_string()),
                    auto_approve_value: Some("bypassPermissions".to_string()),
                },
                response_extractors: vec![
                    TelegramCliResponseExtractor {
                        source: TelegramCliOutputSource::Stdout,
                        format: TelegramCliOutputFormat::Json,
                        match_fields: HashMap::new(),
                        text_path: Some("result".to_string()),
                        join_matches: false,
                        reject_json_input: false,
                    },
                    TelegramCliResponseExtractor {
                        source: TelegramCliOutputSource::Stdout,
                        format: TelegramCliOutputFormat::PlainText,
                        match_fields: HashMap::new(),
                        text_path: None,
                        join_matches: false,
                        reject_json_input: true,
                    },
                    TelegramCliResponseExtractor {
                        source: TelegramCliOutputSource::Stderr,
                        format: TelegramCliOutputFormat::PlainText,
                        match_fields: HashMap::new(),
                        text_path: None,
                        join_matches: false,
                        reject_json_input: false,
                    },
                ],
                usage_extractors: vec![TelegramCliUsageExtractor {
                    source: TelegramCliOutputSource::Stdout,
                    format: TelegramCliOutputFormat::Json,
                    match_fields: HashMap::new(),
                    input_tokens_path: Some("usage.input_tokens".to_string()),
                    output_tokens_path: Some("usage.output_tokens".to_string()),
                    cache_creation_input_tokens_path: Some(
                        "usage.cache_creation_input_tokens".to_string(),
                    ),
                    cache_read_input_tokens_path: Some(
                        "usage.cache_read_input_tokens".to_string(),
                    ),
                    model_key_path: Some("modelUsage".to_string()),
                    session_id_path: Some("session_id".to_string()),
                    ..TelegramCliUsageExtractor::default()
                }],
                ..TelegramCliBackendDefinition::default()
            },
        );

        registry.rebuild_aliases();
        registry
    }
}

impl TelegramCliBackendRegistry {
    fn insert_builtin(&mut self, key: &str, definition: TelegramCliBackendDefinition) {
        let backend = TelegramCliBackend::new(key);
        if self.order.is_empty() {
            self.default_backend = backend.clone();
        }
        self.order.push(backend.clone());
        self.definitions.insert(backend, definition);
    }

    fn rebuild_aliases(&mut self) {
        self.aliases.clear();
        for backend in &self.order {
            self.aliases.insert(
                TelegramCliBackend::normalized(backend.as_str()),
                backend.clone(),
            );
            if let Some(definition) = self.definitions.get(backend) {
                for alias in &definition.aliases {
                    self.aliases
                        .insert(TelegramCliBackend::normalized(alias), backend.clone());
                }
            }
        }
    }

    fn parse(&self, value: &str) -> Option<TelegramCliBackend> {
        self.aliases
            .get(&TelegramCliBackend::normalized(value))
            .cloned()
    }

    fn contains(&self, backend: &TelegramCliBackend) -> bool {
        self.definitions.contains_key(backend)
    }

    fn get(&self, backend: &TelegramCliBackend) -> Option<&TelegramCliBackendDefinition> {
        self.definitions.get(backend)
    }

    fn default_backend(&self) -> TelegramCliBackend {
        self.default_backend.clone()
    }

    fn backend_choices_text(&self) -> String {
        self.order
            .iter()
            .map(|backend| backend.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }

    fn backends(&self) -> impl Iterator<Item = &TelegramCliBackend> {
        self.order.iter()
    }

    fn merge_config_value(&mut self, value: Option<&Value>) {
        let Some(value) = value else {
            return;
        };
        let Some(object) = value.as_object() else {
            return;
        };

        if let Some(backends) = object.get("backends") {
            self.merge_backend_map(backends);
        } else {
            self.merge_legacy_backend_map(value);
        }

        self.rebuild_aliases();

        if let Some(default_backend) = object
            .get("default_backend")
            .and_then(Value::as_str)
            .and_then(|value| self.parse(value))
        {
            self.default_backend = default_backend;
        }
    }

    fn merge_backend_map(&mut self, value: &Value) {
        let Some(backends) = value.as_object() else {
            return;
        };

        for (key, entry) in backends {
            let backend = TelegramCliBackend::new(TelegramCliBackend::normalized(key));
            let mut definition = self
                .definitions
                .get(&backend)
                .cloned()
                .unwrap_or_else(TelegramCliBackendDefinition::default);
            let Some(entry_object) = entry.as_object() else {
                continue;
            };

            if let Some(display_name) = entry_object.get("display_name").and_then(Value::as_str) {
                let trimmed = display_name.trim();
                if !trimmed.is_empty() {
                    definition.display_name = Some(trimmed.to_string());
                }
            }
            if let Some(aliases) = entry_object.get("aliases").and_then(Value::as_array) {
                definition.aliases = aliases
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
            if let Some(candidates) = entry_object
                .get("binary_candidates")
                .and_then(Value::as_array)
            {
                definition.binary_candidates = candidates
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
            if let Some(path) = entry_object.get("binary_path").and_then(Value::as_str) {
                let trimmed = path.trim();
                definition.binary_path = (!trimmed.is_empty()).then(|| trimmed.to_string());
            }
            if let Some(model) = entry_object.get("model").and_then(Value::as_str) {
                let trimmed = model.trim();
                definition.model = (!trimmed.is_empty()).then(|| trimmed.to_string());
            }
            if let Some(auth_hint) = entry_object.get("auth_hint").and_then(Value::as_str) {
                definition.auth_hint = auth_hint.to_string();
            }
            if let Some(usage_hint) = entry_object.get("usage_hint").and_then(Value::as_str) {
                definition.usage_hint = usage_hint.to_string();
            }
            if let Some(usage_hint) = entry_object
                .get("auto_approve_usage_hint")
                .and_then(Value::as_str)
            {
                definition.auto_approve_usage_hint = Some(usage_hint.to_string());
            }
            if let Some(label) = entry_object
                .get("model_choices_source_label")
                .and_then(Value::as_str)
            {
                definition.model_choices_source_label = label.to_string();
            }
            if let Some(choices) = entry_object.get("model_choices").and_then(Value::as_array) {
                definition.model_choices = choices
                    .iter()
                    .filter_map(|entry| match entry {
                        Value::String(value) => {
                            let trimmed = value.trim();
                            (!trimmed.is_empty()).then(|| TelegramCliModelChoice::simple(trimmed))
                        }
                        Value::Object(object) => {
                            let value = object
                                .get("value")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())?;
                            let label = object
                                .get("label")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(ToString::to_string);
                            let description = object
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .map(ToString::to_string);
                            Some(TelegramCliModelChoice {
                                value: value.to_string(),
                                label,
                                description,
                            })
                        }
                        _ => None,
                    })
                    .collect();
            }
            if let Some(label) = entry_object
                .get("usage_source_label")
                .and_then(Value::as_str)
            {
                definition.usage_source_label = label.to_string();
            }
            if let Some(hint) = entry_object
                .get("usage_refresh_hint")
                .and_then(Value::as_str)
            {
                definition.usage_refresh_hint = Some(hint.to_string());
            }
            if let Some(hint) = entry_object
                .get("remaining_usage_hint")
                .and_then(Value::as_str)
            {
                definition.remaining_usage_hint = Some(hint.to_string());
            }
            if let Some(hint) = entry_object.get("reset_usage_hint").and_then(Value::as_str) {
                definition.reset_usage_hint = Some(hint.to_string());
            }
            if let Some(invocation) = entry_object.get("invocation") {
                if let Ok(parsed) =
                    serde_json::from_value::<TelegramCliInvocationTemplate>(invocation.clone())
                {
                    definition.invocation = parsed;
                }
            }
            if let Some(extractors) = entry_object.get("response_extractors") {
                if let Ok(parsed) =
                    serde_json::from_value::<Vec<TelegramCliResponseExtractor>>(extractors.clone())
                {
                    definition.response_extractors = parsed;
                }
            }
            if let Some(extractors) = entry_object.get("usage_extractors") {
                if let Ok(parsed) =
                    serde_json::from_value::<Vec<TelegramCliUsageExtractor>>(extractors.clone())
                {
                    definition.usage_extractors = parsed;
                }
            }
            if let Some(error_hints) = entry_object.get("error_hints") {
                if let Ok(parsed) =
                    serde_json::from_value::<Vec<TelegramCliErrorHint>>(error_hints.clone())
                {
                    definition.error_hints = parsed;
                }
            }

            if !self.order.contains(&backend) {
                self.order.push(backend.clone());
            }
            self.definitions.insert(backend, definition);
        }
    }

    fn merge_legacy_backend_map(&mut self, value: &Value) {
        let Some(backends) = value.as_object() else {
            return;
        };

        for (key, entry) in backends {
            if key == "default_backend" || key == "backends" {
                continue;
            }

            let backend = TelegramCliBackend::new(TelegramCliBackend::normalized(key));
            let mut definition = self
                .definitions
                .get(&backend)
                .cloned()
                .unwrap_or_else(TelegramCliBackendDefinition::default);

            if let Some(path) = entry.as_str() {
                let trimmed = path.trim();
                if !trimmed.is_empty() {
                    definition.binary_path = Some(trimmed.to_string());
                }
            } else if let Some(entry_object) = entry.as_object() {
                if let Some(path) = entry_object.get("binary_path").and_then(Value::as_str) {
                    let trimmed = path.trim();
                    if !trimmed.is_empty() {
                        definition.binary_path = Some(trimmed.to_string());
                    }
                }
                if let Some(model) = entry_object.get("model").and_then(Value::as_str) {
                    let trimmed = model.trim();
                    if !trimmed.is_empty() {
                        definition.model = Some(trimmed.to_string());
                    }
                }
            } else {
                continue;
            }

            if definition.aliases.is_empty() {
                definition.aliases.push(backend.as_str().to_string());
            }
            if definition.display_name.is_none() {
                definition.display_name = Some(backend.as_str().to_string());
            }

            if !self.order.contains(&backend) {
                self.order.push(backend.clone());
            }
            self.definitions.insert(backend, definition);
        }
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
struct TelegramCliActualUsage {
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    cache_creation_input_tokens: i64,
    cache_read_input_tokens: i64,
    thought_tokens: i64,
    tool_tokens: i64,
    model: Option<String>,
    session_id: Option<String>,
    remaining_text: Option<String>,
    reset_at: Option<String>,
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
    total_cli_input_tokens: i64,
    total_cli_output_tokens: i64,
    total_cli_tokens: i64,
    total_cli_cached_input_tokens: i64,
    total_cli_cache_creation_input_tokens: i64,
    total_cli_cache_read_input_tokens: i64,
    total_cli_thought_tokens: i64,
    total_cli_tool_tokens: i64,
    last_actual_usage: Option<TelegramCliActualUsage>,
    last_actual_usage_at_ms: Option<u64>,
}

impl TelegramCliUsageStats {
    fn average_duration_ms(&self) -> u64 {
        if self.requests == 0 {
            0
        } else {
            self.total_duration_ms / self.requests
        }
    }

    fn record_actual_usage(&mut self, usage: TelegramCliActualUsage, completed_at_ms: u64) {
        self.total_cli_input_tokens = self
            .total_cli_input_tokens
            .saturating_add(usage.input_tokens);
        self.total_cli_output_tokens = self
            .total_cli_output_tokens
            .saturating_add(usage.output_tokens);
        self.total_cli_tokens = self.total_cli_tokens.saturating_add(usage.total_tokens);
        self.total_cli_cached_input_tokens = self
            .total_cli_cached_input_tokens
            .saturating_add(usage.cached_input_tokens);
        self.total_cli_cache_creation_input_tokens = self
            .total_cli_cache_creation_input_tokens
            .saturating_add(usage.cache_creation_input_tokens);
        self.total_cli_cache_read_input_tokens = self
            .total_cli_cache_read_input_tokens
            .saturating_add(usage.cache_read_input_tokens);
        self.total_cli_thought_tokens = self
            .total_cli_thought_tokens
            .saturating_add(usage.thought_tokens);
        self.total_cli_tool_tokens = self.total_cli_tool_tokens.saturating_add(usage.tool_tokens);
        self.last_actual_usage = Some(usage);
        self.last_actual_usage_at_ms = Some(completed_at_ms);
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
    model_overrides: HashMap<String, String>,
    chat_session_index: u64,
    coding_session_index: u64,
    usage: HashMap<String, TelegramCliUsageStats>,
}

impl Default for TelegramChatState {
    fn default() -> Self {
        Self {
            interaction_mode: TelegramInteractionMode::Chat,
            cli_backend: TelegramCliBackend::default(),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        }
    }
}

impl TelegramChatState {
    fn usage_for(&self, backend: &TelegramCliBackend) -> TelegramCliUsageStats {
        self.usage
            .get(backend.as_str())
            .cloned()
            .unwrap_or_default()
    }

    fn effective_cli_backend(
        &self,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> TelegramCliBackend {
        if cli_backends.contains(&self.cli_backend) {
            self.cli_backend.clone()
        } else {
            cli_backends.default_backend()
        }
    }

    fn model_override_for(&self, backend: &TelegramCliBackend) -> Option<&str> {
        self.model_overrides
            .get(backend.as_str())
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn effective_cli_model(
        &self,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> Option<String> {
        self.model_override_for(backend)
            .map(ToString::to_string)
            .or_else(|| {
                cli_backends
                    .get(backend)
                    .and_then(|definition| definition.model.clone())
            })
    }

    fn effective_cli_model_source(
        &self,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> &'static str {
        if self.model_override_for(backend).is_some() {
            "chat override"
        } else if cli_backends
            .get(backend)
            .and_then(|definition| definition.model.as_deref())
            .is_some()
        {
            "backend default"
        } else {
            "backend auto"
        }
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

#[derive(Clone, Debug, Default)]
pub(crate) struct CodingAgentToolRequest {
    pub prompt: String,
    pub backend: Option<String>,
    pub project_dir: Option<String>,
    pub model: Option<String>,
    pub execution_mode: Option<String>,
    pub auto_approve: Option<bool>,
    pub timeout_secs: Option<u64>,
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

    fn with_removed_keyboard(text: impl Into<String>) -> Self {
        Self::with_markup(text, TelegramClient::remove_keyboard_markup())
    }
}

#[derive(Debug)]
enum TelegramCliStreamEvent {
    StdoutLine(String),
    StderrLine(String),
}

struct TelegramCliExecutionResult {
    response_text: String,
    send_followup: bool,
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
    cli_backends: Arc<TelegramCliBackendRegistry>,
    cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
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
        let mut cli_backends = TelegramCliBackendRegistry::default();
        cli_backends.merge_config_value(config.settings.get("cli_backends"));

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
                cli_backends.merge_config_value(json.get("cli_backends"));
            }
        }

        Self::read_backend_models_from_llm_config(&config_dir, &mut cli_backends);

        let cli_backend_paths = Arc::new(Self::resolve_cli_backend_paths(&cli_backends));
        let cli_backends = Arc::new(cli_backends);
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
            cli_backends,
            cli_backend_paths,
            chat_states,
            chat_state_path,
            last_user_input: Arc::new(AtomicU64::new(now_secs)),
        }
    }

    fn read_backend_models_from_llm_config(
        config_dir: &Path,
        cli_backends: &mut TelegramCliBackendRegistry,
    ) {
        let gemini_backend = TelegramCliBackend::new("gemini");
        if cli_backends
            .get(&gemini_backend)
            .and_then(|definition| definition.model.as_deref())
            .is_some()
        {
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

        if let Some(definition) = cli_backends.definitions.get_mut(&gemini_backend) {
            definition.model = Some(model.to_string());
        }
    }

    fn resolve_cli_backend_paths(
        cli_backends: &TelegramCliBackendRegistry,
    ) -> HashMap<TelegramCliBackend, String> {
        let mut resolved = HashMap::new();

        for backend in cli_backends.backends() {
            let Some(definition) = cli_backends.get(backend) else {
                continue;
            };

            if let Some(path) = definition.binary_path.as_deref().map(str::trim) {
                if !path.is_empty() {
                    resolved.insert(backend.clone(), path.to_string());
                    continue;
                }
            }

            let candidates = definition
                .binary_candidates
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            if let Some(path) = Self::lookup_binary_on_path(&candidates) {
                resolved.insert(backend.clone(), path);
                continue;
            }
        }

        resolved
    }

    fn load_coding_agent_runtime(
        config_dir: &Path,
    ) -> (
        PathBuf,
        u64,
        TelegramCliBackendRegistry,
        HashMap<TelegramCliBackend, String>,
    ) {
        let default_workdir = std::env::current_dir()
            .unwrap_or_else(|_| crate::core::runtime_paths::default_data_dir());
        let mut cli_workdir = default_workdir;
        let mut cli_timeout_secs = DEFAULT_CLI_TIMEOUT_SECS;
        let mut cli_backends = TelegramCliBackendRegistry::default();

        let telegram_config = config_dir.join("telegram_config.json");
        if let Ok(content) = std::fs::read_to_string(&telegram_config) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                if let Some(path) = json.get("cli_workdir").and_then(|v| v.as_str()) {
                    if !path.trim().is_empty() {
                        cli_workdir = PathBuf::from(path);
                    }
                }
                if let Some(timeout) = json.get("cli_timeout_secs").and_then(|v| v.as_u64()) {
                    cli_timeout_secs = timeout;
                }
                cli_backends.merge_config_value(json.get("cli_backends"));
            }
        }

        Self::read_backend_models_from_llm_config(config_dir, &mut cli_backends);
        let cli_backend_paths = Self::resolve_cli_backend_paths(&cli_backends);
        (cli_workdir, cli_timeout_secs, cli_backends, cli_backend_paths)
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

    fn build_edit_message_payload(
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<Value>,
    ) -> String {
        let mut payload = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text
        });

        if let Some(reply_markup) = reply_markup {
            payload["reply_markup"] = reply_markup;
        }

        payload.to_string()
    }

    fn build_chat_action_payload(chat_id: i64, action: &str) -> String {
        json!({
            "chat_id": chat_id,
            "action": action
        })
        .to_string()
    }

    fn command_menu_entries() -> Vec<(&'static str, &'static str)> {
        vec![
            ("select", "Switch mode"),
            ("coding_agent", "Choose backend"),
            ("model", "Choose model"),
            ("project", "Set project path"),
            ("new_session", "Start new session"),
            ("usage", "Show usage"),
            ("mode", "Choose plan or fast"),
            ("status", "Show current state"),
            ("auto_approve", "Toggle auto approve"),
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

    fn build_owned_reply_keyboard(rows: &[Vec<String>]) -> Value {
        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| row.iter().cloned().map(Value::String).collect())
            .collect();

        json!({
            "keyboard": keyboard,
            "resize_keyboard": true,
            "one_time_keyboard": true
        })
    }

    fn remove_keyboard_markup() -> Value {
        json!({
            "remove_keyboard": true
        })
    }

    fn select_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/select chat", "/select coding"]])
    }

    fn cli_backend_keyboard(cli_backends: &TelegramCliBackendRegistry) -> Value {
        let rows = cli_backends
            .backends()
            .map(|backend| vec![format!("/coding_agent {}", backend.as_str())])
            .collect::<Vec<_>>();
        let row_refs = rows
            .iter()
            .map(|row| row.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let borrowed = row_refs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Self::build_reply_keyboard(&borrowed)
    }

    fn available_model_choices(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> (Vec<TelegramCliModelChoice>, String) {
        let definition = cli_backends.get(backend);
        let mut choices = Vec::new();
        let mut seen = HashSet::new();

        if let Some(current) = state.effective_cli_model(backend, cli_backends) {
            Self::push_model_choice(
                &mut choices,
                &mut seen,
                TelegramCliModelChoice::simple(&current),
            );
        }

        if let Some(definition) = definition {
            for choice in definition.model_choices.iter().cloned() {
                Self::push_model_choice(&mut choices, &mut seen, choice);
            }
        }

        if choices.is_empty() {
            Self::push_model_choice(
                &mut choices,
                &mut seen,
                TelegramCliModelChoice::simple("auto"),
            );
        }

        let source = definition
            .map(|definition| definition.model_choices_source_label.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("configured backend model choices")
            .to_string();

        (choices, source)
    }

    fn push_model_choice(
        choices: &mut Vec<TelegramCliModelChoice>,
        seen: &mut HashSet<String>,
        choice: TelegramCliModelChoice,
    ) {
        let trimmed = choice.value.trim();
        if trimmed.is_empty() {
            return;
        }

        let normalized = choice.normalized_value();
        if seen.insert(normalized) {
            choices.push(TelegramCliModelChoice {
                value: trimmed.to_string(),
                label: choice.label,
                description: choice.description,
            });
        }
    }

    fn model_keyboard(choices: &[TelegramCliModelChoice]) -> Value {
        let mut rows = Vec::new();
        let mut current_row = Vec::new();

        for choice in choices {
            current_row.push(format!("/model {}", choice.value.trim()));
            if current_row.len() == 2 {
                rows.push(std::mem::take(&mut current_row));
            }
        }

        if !current_row.is_empty() {
            rows.push(current_row);
        }

        rows.push(vec!["/model reset".to_string()]);
        Self::build_owned_reply_keyboard(&rows)
    }

    fn format_model_menu_text(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> String {
        let model = state
            .effective_cli_model(backend, cli_backends)
            .unwrap_or_else(|| "auto".to_string());
        let source = state.effective_cli_model_source(backend, cli_backends);
        let (choices, catalog_source) = Self::available_model_choices(state, backend, cli_backends);
        let choices_text = choices
            .iter()
            .map(TelegramCliModelChoice::summary_text)
            .collect::<Vec<_>>()
            .join(" | ");

        format!(
            "CodingAgent: {}\nModel: {}\nSource: {}\nCatalog: {}\nChoices: {}\nUse: /model [name] | /model reset",
            Self::backend_label(backend),
            Self::value_label(model),
            Self::value_label(source),
            Self::value_label(catalog_source),
            Self::value_label(choices_text)
        )
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

    async fn post_telegram_api(
        bot_token: &str,
        method: &str,
        payload: String,
    ) -> Result<Value, String> {
        if bot_token.is_empty() {
            return Err("Telegram bot token is empty.".to_string());
        }

        let url = format!("https://api.telegram.org/bot{}/{}", bot_token, method);
        let client = crate::infra::http_client::HttpClient::new();
        let response = client
            .post(&url, &payload)
            .await
            .map_err(|err| format!("Telegram {} failed: {}", method, err))?;
        let value = serde_json::from_str::<Value>(&response.body)
            .map_err(|err| format!("Telegram {} returned invalid JSON: {}", method, err))?;

        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            let description = value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram API request failed.");
            return Err(description.to_string());
        }

        Ok(value)
    }

    fn extract_telegram_message_id(body: &str) -> Option<i64> {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| Self::telegram_message_id_from_value(&value))
    }

    fn telegram_message_id_from_value(value: &Value) -> Option<i64> {
        value
            .get("result")
            .and_then(|result| result.get("message_id"))
            .and_then(Value::as_i64)
    }

    async fn send_telegram_message_and_get_id(
        bot_token: &str,
        chat_id: i64,
        message: &TelegramOutgoingMessage,
    ) -> Result<i64, String> {
        let safe_text = Self::truncate_chars(&message.text, 4000);
        let payload =
            Self::build_send_message_payload(chat_id, &safe_text, message.reply_markup.clone());
        let value = Self::post_telegram_api(bot_token, "sendMessage", payload).await?;

        Self::telegram_message_id_from_value(&value)
            .or_else(|| Self::extract_telegram_message_id(&value.to_string()))
            .ok_or_else(|| "Telegram sendMessage response did not include message_id.".to_string())
    }

    async fn edit_telegram_message(
        bot_token: &str,
        chat_id: i64,
        message_id: i64,
        message: &TelegramOutgoingMessage,
    ) -> Result<(), String> {
        let safe_text = Self::truncate_chars(&message.text, 4000);
        let payload = Self::build_edit_message_payload(
            chat_id,
            message_id,
            &safe_text,
            message.reply_markup.clone(),
        );

        match Self::post_telegram_api(bot_token, "editMessageText", payload).await {
            Ok(_) => Ok(()),
            Err(err) if err.contains("message is not modified") => Ok(()),
            Err(err) => Err(err),
        }
    }

    async fn send_telegram_chat_action(
        bot_token: &str,
        chat_id: i64,
        action: &str,
    ) -> Result<(), String> {
        let payload = Self::build_chat_action_payload(chat_id, action);
        Self::post_telegram_api(bot_token, "sendChatAction", payload)
            .await
            .map(|_| ())
    }

    async fn wait_with_typing_indicator<F>(
        bot_token: &str,
        chat_id: i64,
        response_future: F,
    ) -> String
    where
        F: Future<Output = String>,
    {
        let _ = Self::send_telegram_chat_action(bot_token, chat_id, "typing").await;

        tokio::pin!(response_future);
        let mut typing_heartbeat =
            tokio::time::interval(Duration::from_secs(TELEGRAM_CHAT_ACTION_UPDATE_SECS));
        typing_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        typing_heartbeat.tick().await;

        loop {
            tokio::select! {
                response = &mut response_future => return response,
                _ = typing_heartbeat.tick() => {
                    let _ = Self::send_telegram_chat_action(bot_token, chat_id, "typing").await;
                }
            }
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

    fn supported_commands_text(cli_backends: &TelegramCliBackendRegistry) -> String {
        let backend_choices = cli_backends.backend_choices_text();
        [
            "Commands",
            "Development requests can be sent directly in normal chat.",
            "/select [chat|coding]",
            &format!("/coding_agent [{}]", backend_choices),
            "/model [name|list|reset]",
            "/project [path]",
            "/project reset",
            "/new_session",
            "/usage",
            "/mode [plan|fast]",
            "/status",
            "/auto_approve [on|off]",
        ]
        .join("\n")
    }

    fn value_label(value: impl AsRef<str>) -> String {
        format!("[{}]", value.as_ref())
    }

    fn backend_label(backend: &TelegramCliBackend) -> String {
        Self::value_label(backend.as_str())
    }

    fn session_number(session_label: &str) -> &str {
        session_label
            .rsplit('-')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(session_label)
    }

    fn session_value_label(session_label: &str) -> String {
        Self::value_label(Self::session_number(session_label))
    }

    fn active_session_value_label(state: &TelegramChatState) -> String {
        Self::session_value_label(&state.active_session_label())
    }

    fn usage_capture_label(captured_at_ms: Option<u64>) -> String {
        captured_at_ms
            .map(|captured_at_ms| {
                let age_secs =
                    Self::current_timestamp_millis().saturating_sub(captured_at_ms) / 1000;
                format!("captured {}s ago", age_secs)
            })
            .unwrap_or_else(|| "not captured yet".to_string())
    }

    fn session_value_label_for_mode(
        state: &TelegramChatState,
        mode: TelegramInteractionMode,
    ) -> String {
        Self::session_value_label(&state.session_label_for(mode))
    }

    fn backend_choices_labels_text(cli_backends: &TelegramCliBackendRegistry) -> String {
        cli_backends
            .backends()
            .map(Self::backend_label)
            .collect::<Vec<_>>()
            .join(", ")
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
            return TelegramOutgoingMessage::with_markup("Select Mode.", Self::select_keyboard());
        };
        let Some(mode) = TelegramInteractionMode::parse(mode_raw) else {
            return TelegramOutgoingMessage::with_markup(
                "Choose [chat] or [coding].",
                Self::select_keyboard(),
            );
        };

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.interaction_mode = mode;
                format!(
                    "Mode: {}\nCodingAgent: {}",
                    Self::value_label(mode.as_str()),
                    Self::backend_label(&state.cli_backend)
                )
            },
        ))
    }

    fn set_cli_backend(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
    ) -> TelegramOutgoingMessage {
        let Some(backend_raw) = args.first() else {
            return TelegramOutgoingMessage::with_markup(
                "Select CodingAgent.",
                Self::cli_backend_keyboard(cli_backends),
            );
        };
        let Some(backend) = cli_backends.parse(backend_raw) else {
            return TelegramOutgoingMessage::with_markup(
                format!(
                    "Choose CodingAgent: {}.",
                    Self::backend_choices_labels_text(cli_backends)
                ),
                Self::cli_backend_keyboard(cli_backends),
            );
        };

        let availability = cli_backend_paths
            .get(&backend)
            .map(|path| format!("Binary: {}", Self::value_label(path)))
            .unwrap_or_else(|| format!("Binary: {}", Self::value_label("not found")));

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.cli_backend = backend.clone();
                let availability = availability.replace('`', "");
                format!(
                    "CodingAgent: {}\n{}",
                    Self::backend_label(&backend),
                    availability
                )
            },
        ))
    }

    fn set_model(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        cli_backends: &TelegramCliBackendRegistry,
    ) -> TelegramOutgoingMessage {
        if args.is_empty() {
            let state = Self::load_chat_state_snapshot(chat_states, chat_id);
            let backend = state.effective_cli_backend(cli_backends);
            let (choices, _) = Self::available_model_choices(&state, &backend, cli_backends);
            return TelegramOutgoingMessage::with_markup(
                Self::format_model_menu_text(&state, &backend, cli_backends),
                Self::model_keyboard(&choices),
            );
        }

        let requested = args.join(" ").trim().to_string();
        if requested.is_empty() {
            return TelegramOutgoingMessage::plain("Model name cannot be empty.");
        }

        match requested.to_ascii_lowercase().as_str() {
            "list" | "menu" | "show" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                let backend = state.effective_cli_backend(cli_backends);
                let (choices, _) = Self::available_model_choices(&state, &backend, cli_backends);
                TelegramOutgoingMessage::with_markup(
                    Self::format_model_menu_text(&state, &backend, cli_backends),
                    Self::model_keyboard(&choices),
                )
            }
            "reset" | "clear" | "default" => TelegramOutgoingMessage::with_removed_keyboard(
                Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
                    let backend = state.effective_cli_backend(cli_backends);
                    state.model_overrides.remove(backend.as_str());
                    let model = state
                        .effective_cli_model(&backend, cli_backends)
                        .unwrap_or_else(|| "auto".to_string());
                    let source = state.effective_cli_model_source(&backend, cli_backends);
                    format!(
                        "CodingAgent: {}\nModel: {}\nSource: {}",
                        Self::backend_label(&backend),
                        Self::value_label(model),
                        Self::value_label(source)
                    )
                }),
            ),
            _ => TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
                chat_states,
                state_path,
                chat_id,
                move |state| {
                    let backend = state.effective_cli_backend(cli_backends);
                    state
                        .model_overrides
                        .insert(backend.as_str().to_string(), requested.clone());
                    format!(
                        "CodingAgent: {}\nModel: {}\nSource: {}",
                        Self::backend_label(&backend),
                        Self::value_label(requested.clone()),
                        Self::value_label("chat override")
                    )
                },
            )),
        }
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
                "Project: {}\nUse: /project [path] | /project reset",
                Self::value_label(effective.display().to_string())
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
                            "Project: {}\nPath: {}",
                            Self::value_label("default"),
                            Self::value_label(&default_display)
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
                format!("Project: {}", Self::value_label(&project_dir_text))
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
                "Select CodingMode.",
                Self::mode_keyboard(),
            );
        };
        let Some(mode) = TelegramExecutionMode::parse(mode_raw) else {
            return TelegramOutgoingMessage::with_markup(
                "Choose [plan] or [fast].",
                Self::mode_keyboard(),
            );
        };

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.execution_mode = mode;
                format!("CodingMode: {}", Self::value_label(mode.as_str()))
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
                "Select AutoApprove.",
                Self::auto_approve_keyboard(),
            );
        };
        let enabled = match value_raw.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => {
                return TelegramOutgoingMessage::with_markup(
                    "Choose [on] or [off].",
                    Self::auto_approve_keyboard(),
                )
            }
        };

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.auto_approve = enabled;
                format!(
                    "AutoApprove: {}\nCodingAgent: {}",
                    Self::value_label(if enabled { "on" } else { "off" }),
                    Self::backend_label(&state.cli_backend)
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
                "Session: {}",
                Self::session_value_label(&state.session_label_for(mode))
            )
        });

        if let Some(state) = prepared_state {
            Self::ensure_session_file(chat_id, state.interaction_mode, &state);
        }

        TelegramOutgoingMessage::plain(reply)
    }

    fn chat_session_id(chat_id: i64, state: &TelegramChatState) -> String {
        format!(
            "tg_{}_{}",
            chat_id,
            state.session_label_for(TelegramInteractionMode::Chat)
        )
    }

    fn format_chat_usage_report(state: &TelegramChatState, usage: &Value) -> String {
        let read = |name: &str| usage.get(name).and_then(Value::as_i64).unwrap_or(0);
        format!(
            "Mode: {}\n\
Session: {}\n\
Prompt: {}\n\
Completion: {}\n\
CacheWrite: {}\n\
CacheRead: {}\n\
Requests: {}\n\
Refresh: {}\n\
Remaining: {}\n\
Reset: {}",
            Self::value_label(TelegramInteractionMode::Chat.as_str()),
            Self::session_value_label_for_mode(state, TelegramInteractionMode::Chat),
            Self::value_label(read("prompt_tokens").to_string()),
            Self::value_label(read("completion_tokens").to_string()),
            Self::value_label(read("cache_creation_input_tokens").to_string()),
            Self::value_label(read("cache_read_input_tokens").to_string()),
            Self::value_label(read("total_requests").to_string()),
            Self::value_label("updates after the next chat response"),
            Self::value_label("not tracked by daemon session store"),
            Self::value_label("not tracked by daemon session store")
        )
    }

    fn format_coding_usage_report(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> String {
        let usage = state.usage_for(backend);
        let backend_definition = cli_backends.get(backend);
        let effective_model = state
            .effective_cli_model(backend, cli_backends)
            .unwrap_or_else(|| "auto".to_string());
        let model_source = state.effective_cli_model_source(backend, cli_backends);
        let usage_source = backend_definition
            .map(|definition| definition.usage_source_label.as_str())
            .filter(|label| !label.trim().is_empty())
            .unwrap_or("backend-specific usage payload");
        let refresh_hint = backend_definition
            .and_then(|definition| definition.usage_refresh_hint.as_deref())
            .unwrap_or("updates after the next successful backend run");
        let mut lines = vec![
            format!(
                "Mode: {}",
                Self::value_label(TelegramInteractionMode::Coding.as_str())
            ),
            format!(
                "Session: {}",
                Self::session_value_label_for_mode(state, TelegramInteractionMode::Coding)
            ),
            format!("CodingAgent: {}", Self::backend_label(backend)),
            format!("Model: {}", Self::value_label(effective_model)),
            format!("ModelSource: {}", Self::value_label(model_source)),
            format!("Source: {}", Self::value_label(usage_source)),
            format!(
                "Updated: {}",
                Self::value_label(Self::usage_capture_label(usage.last_actual_usage_at_ms))
            ),
            format!("Refresh: {}", Self::value_label(refresh_hint)),
        ];

        if let Some(actual) = &usage.last_actual_usage {
            lines.push(format!(
                "LatestCLI: {}",
                Self::value_label(actual.session_id.as_deref().unwrap_or("-"))
            ));
            lines.push(format!(
                "ReportedModel: {}",
                Self::value_label(actual.model.as_deref().unwrap_or("-"))
            ));
            lines.push(format!(
                "Latest: {}",
                Self::value_label(format!(
                    "in {} | out {} | total {}",
                    actual.input_tokens, actual.output_tokens, actual.total_tokens
                ))
            ));
            if actual.cached_input_tokens > 0 {
                lines.push(format!(
                    "Cached: {}",
                    Self::value_label(actual.cached_input_tokens.to_string())
                ));
            }
            if actual.cache_creation_input_tokens > 0 {
                lines.push(format!(
                    "CacheWrite: {}",
                    Self::value_label(actual.cache_creation_input_tokens.to_string())
                ));
            }
            if actual.cache_read_input_tokens > 0 {
                lines.push(format!(
                    "CacheRead: {}",
                    Self::value_label(actual.cache_read_input_tokens.to_string())
                ));
            }
            if actual.thought_tokens > 0 {
                lines.push(format!(
                    "Thought: {}",
                    Self::value_label(actual.thought_tokens.to_string())
                ));
            }
            if actual.tool_tokens > 0 {
                lines.push(format!(
                    "Tool: {}",
                    Self::value_label(actual.tool_tokens.to_string())
                ));
            }
            lines.push(format!(
                "Remaining: {}",
                Self::value_label(
                    actual
                        .remaining_text
                        .as_deref()
                        .or_else(|| {
                            backend_definition
                                .and_then(|definition| definition.remaining_usage_hint.as_deref())
                        })
                        .unwrap_or("pending first successful run")
                )
            ));
            lines.push(format!(
                "Reset: {}",
                Self::value_label(
                    actual
                        .reset_at
                        .as_deref()
                        .or_else(|| {
                            backend_definition
                                .and_then(|definition| definition.reset_usage_hint.as_deref())
                        })
                        .unwrap_or("pending first successful run")
                )
            ));
        } else {
            lines.push(format!("Latest: {}", Self::value_label("not reported yet")));
            lines.push(format!(
                "Remaining: {}",
                Self::value_label(
                    backend_definition
                        .and_then(|definition| definition.remaining_usage_hint.as_deref())
                        .unwrap_or("pending first successful run")
                )
            ));
            lines.push(format!(
                "Reset: {}",
                Self::value_label(
                    backend_definition
                        .and_then(|definition| definition.reset_usage_hint.as_deref())
                        .unwrap_or("pending first successful run")
                )
            ));
        }

        lines.push(format!(
            "Total: {}",
            Self::value_label(format!(
                "in {} | out {} | total {}",
                usage.total_cli_input_tokens, usage.total_cli_output_tokens, usage.total_cli_tokens
            ))
        ));
        if usage.total_cli_cached_input_tokens > 0 {
            lines.push(format!(
                "TotalCached: {}",
                Self::value_label(usage.total_cli_cached_input_tokens.to_string())
            ));
        }
        if usage.total_cli_cache_creation_input_tokens > 0 {
            lines.push(format!(
                "TotalCacheWrite: {}",
                Self::value_label(usage.total_cli_cache_creation_input_tokens.to_string())
            ));
        }
        if usage.total_cli_cache_read_input_tokens > 0 {
            lines.push(format!(
                "TotalCacheRead: {}",
                Self::value_label(usage.total_cli_cache_read_input_tokens.to_string())
            ));
        }
        if usage.total_cli_thought_tokens > 0 {
            lines.push(format!(
                "TotalThought: {}",
                Self::value_label(usage.total_cli_thought_tokens.to_string())
            ));
        }
        if usage.total_cli_tool_tokens > 0 {
            lines.push(format!(
                "TotalTool: {}",
                Self::value_label(usage.total_cli_tool_tokens.to_string())
            ));
        }

        lines.push(format!(
            "Runs: {}",
            Self::value_label(format!(
                "req {} | ok {} | fail {}",
                usage.requests, usage.successes, usage.failures
            ))
        ));
        lines.push(format!(
            "Last: {}",
            Self::value_label(format!(
                "avg {}ms | exit {}",
                usage.average_duration_ms(),
                usage
                    .last_exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ))
        ));

        lines.join("\n")
    }

    fn format_usage_text(
        chat_id: i64,
        state: &TelegramChatState,
        cli_backends: &TelegramCliBackendRegistry,
        agent: Option<&crate::core::agent_core::AgentCore>,
    ) -> String {
        match state.interaction_mode {
            TelegramInteractionMode::Chat => {
                let Some(agent) = agent else {
                    return format!(
                        "Mode: {}\nSession: {}\nStatus: {}",
                        Self::value_label(TelegramInteractionMode::Chat.as_str()),
                        Self::session_value_label_for_mode(state, TelegramInteractionMode::Chat),
                        Self::value_label("usage unavailable")
                    );
                };
                let Some(session_store) = agent.get_session_store() else {
                    return format!(
                        "Mode: {}\nSession: {}\nStatus: {}",
                        Self::value_label(TelegramInteractionMode::Chat.as_str()),
                        Self::session_value_label_for_mode(state, TelegramInteractionMode::Chat),
                        Self::value_label("usage unavailable")
                    );
                };
                let session_id = Self::chat_session_id(chat_id, state);
                let usage = session_store
                    .store()
                    .load_token_usage(&session_id)
                    .to_json();
                Self::format_chat_usage_report(state, &usage)
            }
            TelegramInteractionMode::Coding => {
                let backend = state.effective_cli_backend(cli_backends);
                Self::format_coding_usage_report(state, &backend, cli_backends)
            }
        }
    }

    fn format_status_text(
        chat_id: i64,
        state: &TelegramChatState,
        cli_workdir: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        active_handlers: i32,
    ) -> String {
        let effective_workdir = state.effective_cli_workdir(cli_workdir);
        let backend = state.effective_cli_backend(cli_backends);
        let backend_path = cli_backend_paths
            .get(&backend)
            .map(|path| path.as_str())
            .unwrap_or("not found");
        let usage = state.usage_for(&backend);
        let model = state
            .effective_cli_model(&backend, cli_backends)
            .unwrap_or_else(|| "auto".to_string());

        format!(
            "TizenClaw: {}\n\
Mode: {}\n\
Session: {}\n\
CodingAgent: {}\n\
Model: {}\n\
CodingMode: {}\n\
AutoApprove: {}\n\
Project: {}\n\
Binary: {}\n\
Handlers: {}\n\
Runs: {}",
            Self::value_label("online"),
            Self::value_label(state.interaction_mode.as_str()),
            Self::active_session_value_label(state),
            Self::backend_label(&backend),
            Self::value_label(model),
            Self::value_label(state.execution_mode.as_str()),
            Self::value_label(if state.auto_approve { "on" } else { "off" }),
            Self::value_label(effective_workdir.display().to_string()),
            Self::value_label(backend_path),
            Self::value_label(active_handlers.to_string()),
            Self::value_label(format!(
                "req {} | ok {} | fail {}",
                usage.requests, usage.successes, usage.failures
            ))
        )
    }

    fn handle_command(
        chat_id: i64,
        text: &str,
        agent: Option<&crate::core::agent_core::AgentCore>,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        cli_workdir: &Path,
        active_handlers: i32,
    ) -> Option<TelegramOutgoingMessage> {
        let (command, args) = Self::parse_command(text)?;

        let reply = match command.as_str() {
            "start" | "help" => {
                TelegramOutgoingMessage::plain(Self::supported_commands_text(cli_backends))
            }
            "select" => Self::set_interaction_mode(chat_states, state_path, chat_id, &args),
            "coding-agent" | "coding_agent" | "agent-cli" | "agent_cli" | "cli-backend"
            | "cli_backend" => Self::set_cli_backend(
                chat_states,
                state_path,
                chat_id,
                &args,
                cli_backends,
                cli_backend_paths,
            ),
            "model" => Self::set_model(chat_states, state_path, chat_id, &args, cli_backends),
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
                TelegramOutgoingMessage::plain(Self::format_usage_text(
                    chat_id,
                    &state,
                    cli_backends,
                    agent,
                ))
            }
            "status" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                TelegramOutgoingMessage::plain(Self::format_status_text(
                    chat_id,
                    &state,
                    cli_workdir,
                    cli_backends,
                    cli_backend_paths,
                    active_handlers,
                ))
            }
            _ => TelegramOutgoingMessage::with_markup(
                format!(
                    "Unknown: {}\nUse: {}",
                    Self::value_label(format!("/{}", command)),
                    Self::value_label("/help")
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
                log::warn!(
                    "TelegramClient: state lock poisoned while ensuring chat state: {}",
                    err
                );
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
            "Telegram: {}\nMode: {}\nSession: {}\nCodingAgent: {}",
            Self::value_label("connected"),
            Self::value_label(state.interaction_mode.as_str()),
            Self::active_session_value_label(state),
            Self::backend_label(&state.cli_backend)
        ))
    }

    fn build_startup_message(state: &TelegramChatState) -> TelegramOutgoingMessage {
        TelegramOutgoingMessage::plain(format!(
            "TizenClaw: {}\nMode: {}\nSession: {}\nCodingAgent: {}",
            Self::value_label("online"),
            Self::value_label(state.interaction_mode.as_str()),
            Self::active_session_value_label(state),
            Self::backend_label(&state.cli_backend)
        ))
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
        backend: &TelegramCliBackend,
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
        let recent_context = Self::read_recent_session_excerpt(
            chat_id,
            TelegramInteractionMode::Coding,
            state,
            5000,
        );
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

    fn build_tool_cli_prompt(
        state: &TelegramChatState,
        effective_cli_workdir: &Path,
        backend: &TelegramCliBackend,
        prompt: &str,
    ) -> String {
        let mode_prefix = match state.execution_mode {
            TelegramExecutionMode::Plan => {
                "You are operating as a local coding agent invoked by TizenClaw. Start with a short plan, then perform the work carefully. Keep the final response concise and actionable."
            }
            TelegramExecutionMode::Fast => {
                "You are operating as a local coding agent invoked by TizenClaw. Optimize for speed, keep the response concise, and take the fastest reasonable path."
            }
        };

        format!(
            "{}\n\nSelected backend: {}\nProject directory: {}\nAuto approve: {}\n\nUser request:\n{}",
            mode_prefix,
            backend.as_str(),
            effective_cli_workdir.display(),
            if state.auto_approve { "on" } else { "off" },
            prompt.trim()
        )
    }

    fn build_unified_agent_prompt(
        state: &TelegramChatState,
        default_cli_workdir: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        text: &str,
    ) -> String {
        let backend = state.effective_cli_backend(cli_backends);
        let project_dir = state.effective_cli_workdir(default_cli_workdir);
        let model = state
            .effective_cli_model(&backend, cli_backends)
            .unwrap_or_else(|| "backend auto".to_string());

        format!(
            "You are handling a Telegram request through TizenClaw.\n\
\n\
Telegram development preferences:\n\
- Coding backend: {}\n\
- Coding model: {}\n\
- Project directory: {}\n\
- Coding execution mode: {}\n\
- Coding auto approve: {}\n\
\n\
Ordinary Telegram messages must be handled by TizenClaw first. If the user requests repository work, implementation, refactoring, debugging, testing, or other development work, prefer the run_coding_agent tool instead of replying with prose only.\n\
If the user requests periodic follow-up development work, use create_task and preserve the same coding defaults with project_dir, coding_backend, coding_model, execution_mode, and auto_approve.\n\
\n\
Telegram user request:\n{}",
            backend.as_str(),
            model,
            project_dir.display(),
            state.execution_mode.as_str(),
            if state.auto_approve { "on" } else { "off" },
            text.trim()
        )
    }

    fn build_cli_invocation(
        chat_id: i64,
        state: &TelegramChatState,
        effective_cli_workdir: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        text: &str,
    ) -> Result<(String, Vec<String>), String> {
        let backend = state.effective_cli_backend(cli_backends);
        let definition = cli_backends.get(&backend).ok_or_else(|| {
            format!(
                "Selected backend `{}` is not defined in Telegram config.",
                backend.as_str()
            )
        })?;
        let binary = cli_backend_paths.get(&backend).cloned().ok_or_else(|| {
            format!(
                "Selected backend `{}` is not available on PATH.",
                backend.as_str()
            )
        })?;

        let prompt = Self::build_cli_prompt(
            chat_id,
            state,
            state.execution_mode,
            &backend,
            effective_cli_workdir,
            text,
        );
        let effective_model = state.effective_cli_model(&backend, cli_backends);
        let approval_value = if state.auto_approve {
            definition
                .invocation
                .auto_approve_value
                .as_deref()
                .or(definition.invocation.default_approval_value.as_deref())
                .unwrap_or("")
        } else {
            definition
                .invocation
                .default_approval_value
                .as_deref()
                .unwrap_or("")
        };
        let mut args = Vec::new();
        for template in &definition.invocation.args {
            args.extend(Self::render_cli_arg_template(
                template,
                &prompt,
                effective_cli_workdir,
                effective_model.as_deref(),
                definition.invocation.approval_placeholder.as_deref(),
                approval_value,
            ));
        }

        Ok((binary, args))
    }

    fn render_cli_arg_template(
        template: &str,
        prompt: &str,
        project_dir: &Path,
        model: Option<&str>,
        approval_placeholder: Option<&str>,
        approval_value: &str,
    ) -> Vec<String> {
        let trimmed = template.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if trimmed == "{model_args}" {
            return model
                .map(|model| vec!["--model".to_string(), model.to_string()])
                .unwrap_or_default();
        }

        if trimmed == "{model}" && model.is_none() {
            return Vec::new();
        }

        if let Some(placeholder) = approval_placeholder {
            if trimmed == placeholder {
                return approval_value
                    .split_whitespace()
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
        }

        let mut rendered = trimmed.replace("{prompt}", prompt);
        rendered = rendered.replace("{project_dir}", project_dir.to_string_lossy().as_ref());
        if rendered.contains("{model}") {
            let Some(model) = model else {
                return Vec::new();
            };
            rendered = rendered.replace("{model}", model);
        }
        if let Some(placeholder) = approval_placeholder {
            rendered = rendered.replace(placeholder, approval_value);
        }
        vec![rendered]
    }

    pub(crate) async fn run_coding_agent_tool(
        config_dir: &Path,
        request: &CodingAgentToolRequest,
    ) -> Result<Value, String> {
        let (default_cli_workdir, default_timeout_secs, cli_backends, cli_backend_paths) =
            Self::load_coding_agent_runtime(config_dir);
        let mut state = TelegramChatState::default();
        state.auto_approve = request.auto_approve.unwrap_or(false);
        state.execution_mode = request
            .execution_mode
            .as_deref()
            .and_then(TelegramExecutionMode::parse)
            .unwrap_or(TelegramExecutionMode::Plan);
        state.project_dir = request
            .project_dir
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        if let Some(backend) = request
            .backend
            .as_deref()
            .and_then(|value| cli_backends.parse(value))
        {
            state.cli_backend = backend;
        }

        let backend = state.effective_cli_backend(&cli_backends);
        if let Some(model) = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            state
                .model_overrides
                .insert(backend.as_str().to_string(), model.to_string());
        }

        let effective_cli_workdir = state.effective_cli_workdir(&default_cli_workdir);
        if !effective_cli_workdir.is_dir() {
            return Err(format!(
                "Coding agent project directory '{}' is not available",
                effective_cli_workdir.display()
            ));
        }

        let definition = cli_backends.get(&backend).ok_or_else(|| {
            format!(
                "Selected backend '{}' is not defined in Telegram config.",
                backend.as_str()
            )
        })?;
        let binary = cli_backend_paths.get(&backend).cloned().ok_or_else(|| {
            format!(
                "Selected backend '{}' is not available on PATH.",
                backend.as_str()
            )
        })?;

        let prompt = Self::build_tool_cli_prompt(
            &state,
            &effective_cli_workdir,
            &backend,
            &request.prompt,
        );
        let effective_model = state.effective_cli_model(&backend, &cli_backends);
        let approval_value = if state.auto_approve {
            definition
                .invocation
                .auto_approve_value
                .as_deref()
                .or(definition.invocation.default_approval_value.as_deref())
                .unwrap_or("")
        } else {
            definition
                .invocation
                .default_approval_value
                .as_deref()
                .unwrap_or("")
        };
        let mut args = Vec::new();
        for template in &definition.invocation.args {
            args.extend(Self::render_cli_arg_template(
                template,
                &prompt,
                &effective_cli_workdir,
                effective_model.as_deref(),
                definition.invocation.approval_placeholder.as_deref(),
                approval_value,
            ));
        }

        let timeout_secs = request.timeout_secs.unwrap_or(default_timeout_secs);
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

        let started = Instant::now();
        let output = tokio::time::timeout(Duration::from_secs(timeout_secs), command.output())
            .await
            .map_err(|_| {
                format!(
                    "Coding agent '{}' timed out after {}s",
                    backend.as_str(),
                    timeout_secs
                )
            })?
            .map_err(|err| format!("Failed to start '{}': {}", backend.as_str(), err))?;

        let duration_ms = started.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let response_text = Self::format_cli_result(
            &cli_backends,
            &backend,
            exit_code,
            duration_ms,
            &stdout,
            &stderr,
        );
        let actual_usage = Self::extract_cli_actual_usage(&cli_backends, &backend, &stdout, &stderr);

        Ok(json!({
            "status": if output.status.success() { "success" } else { "error" },
            "backend": backend.as_str(),
            "project_dir": effective_cli_workdir.display().to_string(),
            "model": effective_model,
            "execution_mode": state.execution_mode.as_str(),
            "auto_approve": state.auto_approve,
            "duration_ms": duration_ms,
            "exit_code": exit_code,
            "response_text": response_text,
            "stdout_tail": Self::truncate_chars(stdout.trim(), 4000),
            "stderr_tail": Self::truncate_chars(stderr.trim(), 4000),
            "usage": actual_usage,
        }))
    }

    fn build_cli_streaming_message(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        effective_cli_workdir: &Path,
        phase: &str,
        elapsed_secs: u64,
        last_output_secs: Option<u64>,
        output_text: Option<&str>,
    ) -> String {
        let phase_text = match phase {
            "running" => "running".to_string(),
            "completed" | "failed" => phase.to_string(),
            other if other.starts_with("timed out") => other.to_string(),
            other => other.to_string(),
        };
        let activity =
            last_output_secs.map_or_else(|| "waiting".to_string(), |secs| format!("{}s ago", secs));
        let latest_output = output_text
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| Self::truncate_chars(text, 2600))
            .unwrap_or_else(|| "waiting...".to_string());

        format!(
            "CodingAgent: {}\nStatus: {}\nSession: {}\nProject: {}\nElapsed: {}\nLastOutput: {}\n\nOutput:\n{}",
            Self::backend_label(backend),
            Self::value_label(phase_text),
            Self::session_value_label_for_mode(state, TelegramInteractionMode::Coding),
            Self::value_label(effective_cli_workdir.display().to_string()),
            Self::value_label(format!("{}s", elapsed_secs)),
            Self::value_label(activity),
            latest_output
        )
    }

    fn extract_json_value(text: &str) -> Option<Value> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            serde_json::from_str::<Value>(trimmed).ok()
        }
    }

    fn extract_plain_text(text: &str, reject_json_input: bool) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if reject_json_input && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn output_text_by_source<'a>(
        source: TelegramCliOutputSource,
        stdout: &'a str,
        stderr: &'a str,
    ) -> std::borrow::Cow<'a, str> {
        match source {
            TelegramCliOutputSource::Stdout => std::borrow::Cow::Borrowed(stdout),
            TelegramCliOutputSource::Stderr => std::borrow::Cow::Borrowed(stderr),
            TelegramCliOutputSource::Combined => {
                std::borrow::Cow::Owned(format!("{}\n{}", stdout, stderr))
            }
        }
    }

    fn json_documents(text: &str, format: TelegramCliOutputFormat) -> Vec<Value> {
        match format {
            TelegramCliOutputFormat::Json => Self::extract_json_value(text).into_iter().collect(),
            TelegramCliOutputFormat::JsonLines => {
                text.lines().filter_map(Self::extract_json_value).collect()
            }
            TelegramCliOutputFormat::PlainText => Vec::new(),
        }
    }

    fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
        let mut current = value;
        for part in path
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            if part == "@first_value" {
                current = current.as_object()?.values().next()?;
                continue;
            }
            current = current.get(part)?;
        }
        Some(current)
    }

    fn string_at_path(value: &Value, path: &str) -> Option<String> {
        let value = Self::value_at_path(value, path)?;
        value
            .as_str()
            .map(ToString::to_string)
            .or_else(|| value.as_i64().map(|value| value.to_string()))
            .or_else(|| value.as_u64().map(|value| value.to_string()))
            .or_else(|| value.as_bool().map(|value| value.to_string()))
    }

    fn i64_at_path(value: &Value, path: Option<&str>) -> Option<i64> {
        let path = path?;
        let value = Self::value_at_path(value, path)?;
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    }

    fn document_matches(document: &Value, match_fields: &HashMap<String, String>) -> bool {
        match_fields.iter().all(|(path, expected)| {
            Self::string_at_path(document, path)
                .map(|actual| actual == *expected)
                .unwrap_or(false)
        })
    }

    fn extract_response_from_extractor(
        extractor: &TelegramCliResponseExtractor,
        stdout: &str,
        stderr: &str,
    ) -> Option<String> {
        match extractor.format {
            TelegramCliOutputFormat::PlainText => {
                let text = Self::output_text_by_source(extractor.source, stdout, stderr);
                Self::extract_plain_text(&text, extractor.reject_json_input)
            }
            TelegramCliOutputFormat::Json | TelegramCliOutputFormat::JsonLines => {
                let text = Self::output_text_by_source(extractor.source, stdout, stderr);
                let mut matches = Vec::new();
                for document in Self::json_documents(&text, extractor.format) {
                    if !Self::document_matches(&document, &extractor.match_fields) {
                        continue;
                    }
                    let Some(path) = extractor.text_path.as_deref() else {
                        continue;
                    };
                    let Some(text) = Self::string_at_path(&document, path) else {
                        continue;
                    };
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        matches.push(trimmed.to_string());
                    }
                }

                if matches.is_empty() {
                    None
                } else if extractor.join_matches {
                    Some(matches.join("\n\n"))
                } else {
                    matches.pop()
                }
            }
        }
    }

    fn extract_usage_from_extractor(
        extractor: &TelegramCliUsageExtractor,
        stdout: &str,
        stderr: &str,
    ) -> Option<TelegramCliActualUsage> {
        let text = Self::output_text_by_source(extractor.source, stdout, stderr);
        let document = Self::json_documents(&text, extractor.format)
            .into_iter()
            .filter(|document| Self::document_matches(document, &extractor.match_fields))
            .last()?;
        let input_tokens =
            Self::i64_at_path(&document, extractor.input_tokens_path.as_deref()).unwrap_or(0);
        let output_tokens =
            Self::i64_at_path(&document, extractor.output_tokens_path.as_deref()).unwrap_or(0);

        Some(TelegramCliActualUsage {
            input_tokens,
            output_tokens,
            total_tokens: Self::i64_at_path(&document, extractor.total_tokens_path.as_deref())
                .unwrap_or_else(|| input_tokens.saturating_add(output_tokens)),
            cached_input_tokens: Self::i64_at_path(
                &document,
                extractor.cached_input_tokens_path.as_deref(),
            )
            .unwrap_or(0),
            cache_creation_input_tokens: Self::i64_at_path(
                &document,
                extractor.cache_creation_input_tokens_path.as_deref(),
            )
            .unwrap_or(0),
            cache_read_input_tokens: Self::i64_at_path(
                &document,
                extractor.cache_read_input_tokens_path.as_deref(),
            )
            .unwrap_or(0),
            thought_tokens: Self::i64_at_path(&document, extractor.thought_tokens_path.as_deref())
                .unwrap_or(0),
            tool_tokens: Self::i64_at_path(&document, extractor.tool_tokens_path.as_deref())
                .unwrap_or(0),
            model: extractor
                .model_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path))
                .or_else(|| {
                    extractor.model_key_path.as_deref().and_then(|path| {
                        Self::value_at_path(&document, path)?
                            .as_object()?
                            .keys()
                            .next()
                            .cloned()
                    })
                }),
            session_id: extractor
                .session_id_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path)),
            remaining_text: extractor
                .remaining_text_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path)),
            reset_at: extractor
                .reset_at_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path)),
        })
    }

    fn extract_cli_actual_usage(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        stdout: &str,
        stderr: &str,
    ) -> Option<TelegramCliActualUsage> {
        let definition = cli_backends.get(backend)?;
        for extractor in &definition.usage_extractors {
            if let Some(usage) = Self::extract_usage_from_extractor(extractor, stdout, stderr) {
                return Some(usage);
            }
        }
        None
    }

    fn extract_cli_response_text(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        stdout: &str,
        stderr: &str,
    ) -> Option<String> {
        let definition = cli_backends.get(backend)?;
        for extractor in &definition.response_extractors {
            if let Some(text) = Self::extract_response_from_extractor(extractor, stdout, stderr) {
                return Some(text);
            }
        }
        None
    }

    fn extract_incremental_cli_response(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        stdout: &str,
        stderr: &str,
        last_sent_text: &str,
    ) -> Option<String> {
        let current = Self::extract_cli_response_text(cli_backends, backend, stdout, stderr)?;
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
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        exit_code: i32,
        duration_ms: u64,
        stdout: &str,
        stderr: &str,
    ) -> String {
        if let Some(definition) = cli_backends.get(backend) {
            for hint in &definition.error_hints {
                let haystack = Self::output_text_by_source(hint.source, stdout, stderr);
                if hint
                    .patterns
                    .iter()
                    .any(|pattern| haystack.contains(pattern))
                {
                    return hint.message.clone();
                }
            }
        }

        if exit_code == 0 {
            if let Some(text) =
                Self::extract_cli_response_text(cli_backends, backend, stdout, stderr)
            {
                return Self::truncate_chars(text.trim(), 3400);
            }

            return format!(
                "CodingAgent: {}\nStatus: {}\nElapsed: {}\nOutput: {}",
                Self::backend_label(backend),
                Self::value_label("done"),
                Self::value_label(format!("{}ms", duration_ms)),
                Self::value_label("not captured")
            );
        }

        let body = Self::extract_cli_response_text(cli_backends, backend, stdout, stderr)
            .unwrap_or_else(|| "CLI failed with no output.".to_string());

        format!(
            "CodingAgent: {}\nStatus: {}\nElapsed: {}\nExitCode: {}\n\n{}",
            Self::backend_label(backend),
            Self::value_label("failed"),
            Self::value_label(format!("{}ms", duration_ms)),
            Self::value_label(exit_code.to_string()),
            Self::truncate_chars(body.trim(), 3400)
        )
    }

    async fn execute_cli_request(
        bot_token: &str,
        chat_id: i64,
        text: &str,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backends: Arc<TelegramCliBackendRegistry>,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
    ) -> TelegramCliExecutionResult {
        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        let backend = state.effective_cli_backend(&cli_backends);
        let started_at = Self::current_timestamp_millis();
        let effective_cli_workdir = state.effective_cli_workdir(&cli_workdir);

        let invocation = match Self::build_cli_invocation(
            chat_id,
            &state,
            &effective_cli_workdir,
            &cli_backends,
            &cli_backend_paths,
            text,
        ) {
            Ok(invocation) => invocation,
            Err(err) => {
                return TelegramCliExecutionResult {
                    response_text: err,
                    send_followup: true,
                }
            }
        };

        let snapshot = match chat_states.lock() {
            Ok(mut states) => {
                let state = states.entry(chat_id).or_default();
                let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                usage.requests = usage.requests.saturating_add(1);
                usage.last_started_at_ms = Some(started_at);
                states.clone()
            }
            Err(err) => {
                return TelegramCliExecutionResult {
                    response_text: format!("State update failed before CLI execution: {}", err),
                    send_followup: true,
                };
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
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
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
                return TelegramCliExecutionResult {
                    response_text: format!("Failed to start `{}`: {}", backend.as_str(), err),
                    send_followup: true,
                };
            }
        };

        let initial_progress = TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
            &state,
            &backend,
            &effective_cli_workdir,
            "running",
            0,
            None,
            None,
        ));
        let initial_message_id =
            match Self::send_telegram_message_and_get_id(bot_token, chat_id, &initial_progress)
                .await
            {
                Ok(message_id) => Some(message_id),
                Err(err) => {
                    log::warn!(
                        "TelegramClient: failed to create streaming progress message: {}",
                        err
                    );
                    None
                }
            };
        if initial_message_id.is_some() {
            let _ = Self::send_telegram_chat_action(bot_token, chat_id, "typing").await;
        }

        let Some(stdout_reader) = child.stdout.take() else {
            let response_text = format!("Failed to capture `{}` stdout.", backend.as_str());
            if let Some(message_id) = initial_message_id {
                let message = TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                    &state,
                    &backend,
                    &effective_cli_workdir,
                    "failed",
                    0,
                    None,
                    Some(&response_text),
                ));
                let _ = Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await;
            }
            return TelegramCliExecutionResult {
                response_text,
                send_followup: initial_message_id.is_none(),
            };
        };
        let Some(stderr_reader) = child.stderr.take() else {
            let response_text = format!("Failed to capture `{}` stderr.", backend.as_str());
            if let Some(message_id) = initial_message_id {
                let message = TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                    &state,
                    &backend,
                    &effective_cli_workdir,
                    "failed",
                    0,
                    None,
                    Some(&response_text),
                ));
                let _ = Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await;
            }
            return TelegramCliExecutionResult {
                response_text,
                send_followup: initial_message_id.is_none(),
            };
        };

        let started = Instant::now();
        let progress_state = state.clone();
        let progress_workdir = effective_cli_workdir.clone();
        let execution_backend = backend.clone();
        let execution_cli_backends = cli_backends.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(Self::read_cli_stream(stdout_reader, true, tx.clone()));
        tokio::spawn(Self::read_cli_stream(stderr_reader, false, tx));

        let execution = async move {
            let mut child = child;
            let wait_fut = child.wait();
            tokio::pin!(wait_fut);
            let mut progress_heartbeat =
                tokio::time::interval(Duration::from_secs(CLI_PROGRESS_UPDATE_SECS));
            progress_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            progress_heartbeat.tick().await;
            let mut typing_heartbeat =
                tokio::time::interval(Duration::from_secs(TELEGRAM_CHAT_ACTION_UPDATE_SECS));
            typing_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            typing_heartbeat.tick().await;

            let mut stdout = String::new();
            let mut stderr = String::new();
            let mut last_partial_text = String::new();
            let mut latest_output_text = None::<String>;
            let mut last_output_at = None::<Instant>;
            let mut child_status = None;
            let mut streaming_message_id = initial_message_id;
            let mut stream_message_usable = streaming_message_id.is_some();

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

                        if let Some(current_text) = Self::extract_cli_response_text(
                            &execution_cli_backends,
                            &execution_backend,
                            &stdout,
                            &stderr,
                        )
                        {
                            let current_text = current_text.trim().to_string();
                            if !current_text.is_empty() {
                                latest_output_text = Some(current_text);
                            }
                        }

                        if stream_message_usable
                            && Self::extract_incremental_cli_response(
                                &execution_cli_backends,
                                &execution_backend,
                                &stdout,
                                &stderr,
                                &last_partial_text,
                            )
                            .is_some()
                        {
                            if let (Some(message_id), Some(current_text)) =
                                (streaming_message_id, latest_output_text.as_deref())
                            {
                                let elapsed_secs = started.elapsed().as_secs();
                                let last_output_secs =
                                    last_output_at.map(|instant| instant.elapsed().as_secs());
                                let message = TelegramOutgoingMessage::plain(
                                    Self::build_cli_streaming_message(
                                        &progress_state,
                                        &execution_backend,
                                        &progress_workdir,
                                        "running",
                                        elapsed_secs,
                                        last_output_secs,
                                        Some(current_text),
                                    ),
                                );
                                match Self::edit_telegram_message(
                                    bot_token,
                                    chat_id,
                                    message_id,
                                    &message,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        last_partial_text = current_text.to_string();
                                    }
                                    Err(err) => {
                                        log::warn!(
                                            "TelegramClient: failed to edit streaming message: {}",
                                            err
                                        );
                                        streaming_message_id = None;
                                        stream_message_usable = false;
                                    }
                                }
                            }
                        }
                    }
                    _ = progress_heartbeat.tick() => {
                        if child_status.is_none() && stream_message_usable {
                            if let Some(message_id) = streaming_message_id {
                                let elapsed_secs = started.elapsed().as_secs();
                                let last_output_secs =
                                    last_output_at.map(|instant| instant.elapsed().as_secs());
                                let message = TelegramOutgoingMessage::plain(
                                    Self::build_cli_streaming_message(
                                        &progress_state,
                                        &execution_backend,
                                        &progress_workdir,
                                        "running",
                                        elapsed_secs,
                                        last_output_secs,
                                        latest_output_text.as_deref(),
                                    ),
                                );
                                if let Err(err) = Self::edit_telegram_message(
                                    bot_token,
                                    chat_id,
                                    message_id,
                                    &message,
                                )
                                .await
                                {
                                    log::warn!(
                                        "TelegramClient: failed to refresh streaming message: {}",
                                        err
                                    );
                                    streaming_message_id = None;
                                    stream_message_usable = false;
                                }
                            }
                        }
                    }
                    _ = typing_heartbeat.tick() => {
                        if child_status.is_none() {
                            let _ = Self::send_telegram_chat_action(
                                bot_token,
                                chat_id,
                                "typing",
                            )
                            .await;
                        }
                    }
                }
            }

            let output = match child_status {
                Some(status) => status?,
                None => wait_fut.await?,
            };
            Ok::<_, std::io::Error>((
                output,
                stdout,
                stderr,
                last_output_at,
                latest_output_text,
                streaming_message_id,
                stream_message_usable,
            ))
        };

        let timed_output =
            tokio::time::timeout(Duration::from_secs(cli_timeout_secs), execution).await;

        match timed_output {
            Ok(Ok((
                status,
                stdout,
                stderr,
                last_output_at,
                latest_output_text,
                streaming_message_id,
                stream_message_usable,
            ))) => {
                let duration_ms = started.elapsed().as_millis() as u64;
                let exit_code = status.code().unwrap_or(-1);
                let success = status.success();
                let completed_at_ms = Self::current_timestamp_millis();
                let actual_cli_usage =
                    Self::extract_cli_actual_usage(&cli_backends, &backend, &stdout, &stderr);

                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                        if success {
                            usage.successes = usage.successes.saturating_add(1);
                        } else {
                            usage.failures = usage.failures.saturating_add(1);
                        }
                        usage.total_duration_ms =
                            usage.total_duration_ms.saturating_add(duration_ms);
                        usage.last_exit_code = Some(exit_code);
                        usage.last_completed_at_ms = Some(completed_at_ms);
                        if let Some(actual_cli_usage) = actual_cli_usage {
                            usage.record_actual_usage(actual_cli_usage, completed_at_ms);
                        }
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }

                let response_text = Self::format_cli_result(
                    &cli_backends,
                    &backend,
                    exit_code,
                    duration_ms,
                    &stdout,
                    &stderr,
                );
                let mut send_followup = streaming_message_id.is_none() || !stream_message_usable;

                if let Some(message_id) = streaming_message_id {
                    let phase = if success { "completed" } else { "failed" };
                    let last_output_secs =
                        last_output_at.map(|instant| instant.elapsed().as_secs());
                    let final_output = latest_output_text
                        .as_deref()
                        .filter(|text| !text.trim().is_empty())
                        .unwrap_or(response_text.as_str());
                    let message =
                        TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                            &state,
                            &backend,
                            &effective_cli_workdir,
                            phase,
                            started.elapsed().as_secs(),
                            last_output_secs,
                            Some(final_output),
                        ));
                    if let Err(err) =
                        Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await
                    {
                        log::warn!(
                            "TelegramClient: failed to finalize streaming message: {}",
                            err
                        );
                        send_followup = true;
                    }
                }

                TelegramCliExecutionResult {
                    response_text,
                    send_followup,
                }
            }
            Ok(Err(err)) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
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

                let response_text = format!(
                    "`{}` failed while waiting for output: {}",
                    backend.as_str(),
                    err
                );
                if let Some(message_id) = initial_message_id {
                    let message =
                        TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                            &state,
                            &backend,
                            &effective_cli_workdir,
                            "failed",
                            started.elapsed().as_secs(),
                            None,
                            Some(&response_text),
                        ));
                    if let Err(edit_err) =
                        Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await
                    {
                        log::warn!(
                            "TelegramClient: failed to report execution error in streaming message: {}",
                            edit_err
                        );
                        return TelegramCliExecutionResult {
                            response_text,
                            send_followup: true,
                        };
                    }
                    return TelegramCliExecutionResult {
                        response_text,
                        send_followup: false,
                    };
                }

                TelegramCliExecutionResult {
                    response_text,
                    send_followup: true,
                }
            }
            Err(_) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
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

                let response_text = format!(
                    "`{}` timed out after `{}` seconds.",
                    backend.as_str(),
                    cli_timeout_secs
                );
                if let Some(message_id) = initial_message_id {
                    let message =
                        TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                            &state,
                            &backend,
                            &effective_cli_workdir,
                            &format!("timed out after `{}` second(s)", cli_timeout_secs),
                            cli_timeout_secs,
                            None,
                            Some(&response_text),
                        ));
                    if let Err(err) =
                        Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await
                    {
                        log::warn!(
                            "TelegramClient: failed to report timeout in streaming message: {}",
                            err
                        );
                        return TelegramCliExecutionResult {
                            response_text,
                            send_followup: true,
                        };
                    }
                    return TelegramCliExecutionResult {
                        response_text,
                        send_followup: false,
                    };
                }

                TelegramCliExecutionResult {
                    response_text,
                    send_followup: true,
                }
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
        cli_backends: Arc<TelegramCliBackendRegistry>,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
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
            agent.as_deref(),
            &chat_states,
            &state_path,
            &cli_backends,
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
                let prompt = Self::build_unified_agent_prompt(
                    &state,
                    &cli_workdir,
                    &cli_backends,
                    text,
                );
                let response = Self::wait_with_typing_indicator(
                    bot_token,
                    chat_id,
                    agent_core.process_prompt(&session_id, &prompt, None),
                )
                .await;
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
                let Some(agent_core) = agent else {
                    replies.push(TelegramOutgoingMessage::plain(
                        "AgentCore is not available for coding mode.",
                    ));
                    return replies;
                };
                let session_id = format!(
                    "tg_{}_{}",
                    chat_id,
                    state.session_label_for(TelegramInteractionMode::Coding)
                );
                let prompt = Self::build_unified_agent_prompt(
                    &state,
                    &cli_workdir,
                    &cli_backends,
                    text,
                );
                let response = Self::wait_with_typing_indicator(
                    bot_token,
                    chat_id,
                    agent_core.process_prompt(&session_id, &prompt, None),
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
        let cli_backends = self.cli_backends.clone();
        let cli_backend_paths = self.cli_backend_paths.clone();
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
                        let cli_backends_clone = cli_backends.clone();
                        let cli_backend_paths_clone = cli_backend_paths.clone();
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
                                cli_backends_clone,
                                cli_backend_paths_clone,
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
        TelegramChatState, TelegramCliActualUsage, TelegramCliBackend, TelegramCliBackendRegistry,
        TelegramCliUsageStats, TelegramClient, TelegramExecutionMode, TelegramInteractionMode,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn backend(value: &str) -> TelegramCliBackend {
        TelegramCliBackend::new(value)
    }

    fn default_registry() -> TelegramCliBackendRegistry {
        TelegramCliBackendRegistry::default()
    }

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
        assert_eq!(
            TelegramExecutionMode::parse("fast"),
            Some(TelegramExecutionMode::Fast)
        );
        assert_eq!(
            default_registry().parse("claude-code"),
            Some(backend("claude"))
        );
    }

    #[test]
    fn default_chat_state_prefers_codex_plan_chat_mode() {
        let state = TelegramChatState::default();
        assert_eq!(state.interaction_mode, TelegramInteractionMode::Chat);
        assert_eq!(state.cli_backend, backend("codex"));
        assert_eq!(state.execution_mode, TelegramExecutionMode::Plan);
        assert!(!state.auto_approve);
        assert_eq!(
            state.session_label_for(TelegramInteractionMode::Chat),
            "chat-0001"
        );
        assert_eq!(
            state.session_label_for(TelegramInteractionMode::Coding),
            "coding-0001"
        );
    }

    #[test]
    fn send_message_payload_is_plain_text_json() {
        let payload = TelegramClient::build_send_message_payload(123, "value_with`markdown`", None);
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["chat_id"], 123);
        assert_eq!(json["text"], "value_with`markdown`");
        assert!(json.get("parse_mode").is_none());
        assert!(json.get("reply_markup").is_none());
    }

    #[test]
    fn supported_commands_text_uses_coding_agent_name() {
        let help = TelegramClient::supported_commands_text(&default_registry());
        assert!(help.contains("/coding_agent [codex|gemini|claude]"));
        assert!(help.contains("/model [name|list|reset]"));
        assert!(help.contains("/usage"));
        assert!(help.contains("/auto_approve [on|off]"));
        assert!(help.contains("/project [path]"));
        assert!(help.contains("/new_session"));
        assert!(!help.contains("/agent_cli [codex|gemini|claude]"));
        assert!(!help.contains("/cli_backend [codex|gemini|claude]"));
        assert!(!help.contains("/cli-backend [codex|gemini|claude]"));
        assert!(!help.contains("/auto-approve [on|off]"));
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
                "coding_agent",
                "model",
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
    fn removed_keyboard_markup_is_serialized() {
        let payload = TelegramClient::build_send_message_payload(
            7,
            "done",
            Some(TelegramClient::remove_keyboard_markup()),
        );
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["reply_markup"]["remove_keyboard"], true);
    }

    #[test]
    fn coding_agent_keyboard_uses_new_command_name() {
        let keyboard = TelegramClient::cli_backend_keyboard(&default_registry());
        assert_eq!(keyboard["keyboard"][0][0], "/coding_agent codex");
        assert_eq!(keyboard["keyboard"][1][0], "/coding_agent gemini");
        assert_eq!(keyboard["keyboard"][2][0], "/coding_agent claude");
    }

    #[test]
    fn model_keyboard_exposes_curated_choices_and_reset() {
        let state = TelegramChatState::default();
        let backend = backend("gemini");
        let (choices, source) =
            TelegramClient::available_model_choices(&state, &backend, &default_registry());
        let keyboard = TelegramClient::model_keyboard(&choices);

        assert_eq!(source, "Gemini CLI aliases and documented model names");
        assert_eq!(keyboard["keyboard"][0][0], "/model gemini-2.5-flash");
        assert_eq!(keyboard["keyboard"][0][1], "/model auto");
        assert_eq!(keyboard["keyboard"][4][0], "/model reset");
    }

    #[test]
    fn custom_backend_from_config_is_exposed_in_help_and_keyboard() {
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "default_backend": "custom_agent",
            "backends": {
                "custom_agent": {
                    "aliases": ["custom"],
                    "binary_path": "/usr/bin/custom-agent",
                    "usage_hint": "`custom-agent run --json <prompt>`",
                    "invocation": {
                        "args": ["run", "--json", "{prompt}"]
                    },
                    "response_extractors": [
                        { "source": "stdout", "format": "json", "text_path": "result" }
                    ],
                    "usage_extractors": [
                        {
                            "source": "stdout",
                            "format": "json",
                            "input_tokens_path": "usage.input_tokens",
                            "output_tokens_path": "usage.output_tokens"
                        }
                    ]
                }
            }
        })));

        let help = TelegramClient::supported_commands_text(&registry);
        let keyboard = TelegramClient::cli_backend_keyboard(&registry);

        assert!(help.contains("/coding_agent [codex|gemini|claude|custom_agent]"));
        assert_eq!(keyboard["keyboard"][3][0], "/coding_agent custom_agent");
        assert_eq!(registry.parse("custom"), Some(backend("custom_agent")));
        assert_eq!(registry.default_backend(), backend("custom_agent"));
    }

    #[test]
    fn connected_message_mentions_current_mode() {
        let message = TelegramClient::build_connected_message(&TelegramChatState::default());
        assert!(message.text.contains("Telegram: [connected]"));
        assert!(message.text.contains("Mode: [chat]"));
        assert!(message.text.contains("Session: [0001]"));
        assert!(message.text.contains("CodingAgent: [codex]"));
        assert!(message.reply_markup.is_none());
    }

    #[test]
    fn startup_message_mentions_current_mode() {
        let message = TelegramClient::build_startup_message(&TelegramChatState::default());
        assert!(message.text.contains("TizenClaw: [online]"));
        assert!(message.text.contains("Mode: [chat]"));
        assert!(message.text.contains("Session: [0001]"));
        assert!(message.text.contains("CodingAgent: [codex]"));
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
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Select Mode."));
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][0],
            "/select chat"
        );
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][1],
            "/select coding"
        );
    }

    #[test]
    fn select_with_valid_arg_removes_reply_keyboard() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_select_success_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let reply = TelegramClient::handle_command(
            77,
            "/select coding",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Mode: [coding]"));
        assert!(reply.text.contains("CodingAgent: [codex]"));
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );
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
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Project: [/tmp]"));
        assert!(reply.text.contains("Use: /project [path] | /project reset"));
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
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/work"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Project: ["));
        let state = TelegramClient::load_chat_state_snapshot(&chat_states, 77);
        let expected = project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(state.project_dir.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn coding_agent_command_and_legacy_aliases_route_to_backend_selection() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_coding_agent_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let backend_paths = HashMap::from([(backend("claude"), "/usr/bin/claude".to_string())]);
        let registry = default_registry();

        let new_reply = TelegramClient::handle_command(
            77,
            "/coding_agent claude",
            None,
            &chat_states,
            &state_path,
            &registry,
            &backend_paths,
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(new_reply.text.contains("CodingAgent: [claude]"));
        assert!(new_reply.text.contains("Binary: [/usr/bin/claude]"));
        assert_eq!(
            new_reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );

        let legacy_reply = TelegramClient::handle_command(
            77,
            "/cli_backend codex",
            None,
            &chat_states,
            &state_path,
            &registry,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(legacy_reply.text.contains("CodingAgent: [codex]"));
        assert!(legacy_reply.text.contains("Binary: [not found]"));
        assert_eq!(
            legacy_reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );

        let older_alias_reply = TelegramClient::handle_command(
            77,
            "/agent_cli claude",
            None,
            &chat_states,
            &state_path,
            &registry,
            &backend_paths,
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(older_alias_reply.text.contains("CodingAgent: [claude]"));
        assert!(older_alias_reply.text.contains("Binary: [/usr/bin/claude]"));
        assert_eq!(
            older_alias_reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );
    }

    #[test]
    fn model_command_sets_shows_and_resets_backend_override() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_model_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));

        let set_reply = TelegramClient::handle_command(
            77,
            "/model claude-sonnet-4-6",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(set_reply.text.contains("CodingAgent: [codex]"));
        assert!(set_reply.text.contains("Model: [claude-sonnet-4-6]"));
        assert!(set_reply.text.contains("Source: [chat override]"));

        let show_reply = TelegramClient::handle_command(
            77,
            "/model",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(show_reply.text.contains("Model: [claude-sonnet-4-6]"));
        assert!(show_reply
            .text
            .contains("Catalog: [curated Codex-compatible model choices]"));
        assert!(show_reply.text.contains("Choices: [claude-sonnet-4-6"));
        assert_eq!(
            show_reply.reply_markup.as_ref().unwrap()["keyboard"][0][0],
            "/model claude-sonnet-4-6"
        );

        let reset_reply = TelegramClient::handle_command(
            77,
            "/model reset",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(reset_reply.text.contains("Model: [auto]"));
        assert!(reset_reply.text.contains("Source: [backend auto]"));
        assert_eq!(
            reset_reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );
    }

    #[test]
    fn custom_backend_model_choices_are_shown_in_model_menu() {
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "backends": {
                "custom_agent": {
                    "model_choices_source_label": "custom backend menu",
                    "model_choices": [
                        "alpha",
                        { "value": "beta-fast", "label": "beta", "description": "fast tier" }
                    ]
                }
            }
        })));

        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_model_custom_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));

        let _ = TelegramClient::handle_command(
            77,
            "/coding_agent custom_agent",
            None,
            &chat_states,
            &state_path,
            &registry,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        let reply = TelegramClient::handle_command(
            77,
            "/model",
            None,
            &chat_states,
            &state_path,
            &registry,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Catalog: [custom backend menu]"));
        assert!(reply.text.contains("Choices: [alpha | beta -> beta-fast]"));
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][0],
            "/model alpha"
        );
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][1],
            "/model beta-fast"
        );
    }

    #[test]
    fn config_driven_codex_response_and_usage_are_parsed() {
        let output = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\",\"type\":\"agent_message\",\"text\":\"HELLO\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1}}\n"
        );
        let registry = default_registry();
        let codex = backend("codex");

        assert_eq!(
            TelegramClient::extract_cli_response_text(&registry, &codex, output, "").as_deref(),
            Some("HELLO")
        );
        let output = "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":12,\"cached_input_tokens\":3,\"output_tokens\":4}}\n";
        let usage =
            TelegramClient::extract_cli_actual_usage(&registry, &codex, output, "").unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.cached_input_tokens, 3);
        assert_eq!(usage.output_tokens, 4);
        assert_eq!(usage.total_tokens, 16);
    }

    #[test]
    fn telegram_message_id_is_extracted_from_send_message_response() {
        let body = r#"{"ok":true,"result":{"message_id":77,"text":"hello"}}"#;

        assert_eq!(TelegramClient::extract_telegram_message_id(body), Some(77));
    }

    #[test]
    fn chat_action_payload_contains_chat_id_and_action() {
        let payload = TelegramClient::build_chat_action_payload(77, "typing");
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(value["chat_id"].as_i64(), Some(77));
        assert_eq!(value["action"].as_str(), Some("typing"));
    }

    #[tokio::test]
    async fn typing_indicator_helper_returns_response_even_without_token() {
        let response = TelegramClient::wait_with_typing_indicator("", 77, async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            "done".to_string()
        })
        .await;

        assert_eq!(response, "done");
    }

    #[test]
    fn cli_streaming_message_mentions_progress_and_project() {
        let state = TelegramChatState::default();
        let message = TelegramClient::build_cli_streaming_message(
            &state,
            &backend("codex"),
            std::path::Path::new("/tmp/project"),
            "running",
            15,
            None,
            None,
        );

        assert!(message.contains("CodingAgent: [codex]"));
        assert!(message.contains("Status: [running]"));
        assert!(message.contains("Session: [0001]"));
        assert!(message.contains("Project: [/tmp/project]"));
        assert!(message.contains("Elapsed: [15s]"));
        assert!(message.contains("LastOutput: [waiting]"));
        assert!(message.contains("waiting..."));
    }

    #[test]
    fn cli_streaming_message_includes_latest_output_summary() {
        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            ..TelegramChatState::default()
        };
        let message = TelegramClient::build_cli_streaming_message(
            &state,
            &backend("claude"),
            std::path::Path::new("/tmp/project"),
            "completed",
            22,
            Some(3),
            Some("Third line extends the response"),
        );

        assert!(message.contains("CodingAgent: [claude]"));
        assert!(message.contains("Status: [completed]"));
        assert!(message.contains("LastOutput: [3s ago]"));
        assert!(message.contains("Output:"));
        assert!(message.contains("Third line extends the response"));
    }

    #[test]
    fn incremental_cli_response_uses_new_text_delta() {
        let registry = default_registry();
        let stdout = "First line of output\nSecond line of output with enough detail";
        let partial = TelegramClient::extract_incremental_cli_response(
            &registry,
            &backend("claude"),
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
            &registry,
            &backend("claude"),
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
            cli_backend: backend("codex"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let backend_paths = HashMap::from([(backend("codex"), "/usr/bin/codex".to_string())]);
        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &backend_paths,
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
            cli_backend: backend("gemini"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let backend_paths = HashMap::from([(backend("gemini"), "/snap/bin/gemini".to_string())]);

        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &backend_paths,
            "hello",
        )
        .unwrap();

        assert_eq!(binary, "/snap/bin/gemini");
        let model_index = args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(args[model_index + 1], "gemini-2.5-flash");
        assert!(args.iter().any(|arg| arg == "--prompt"));
        assert!(args.iter().any(|arg| arg == "--output-format"));
        let output_index = args
            .iter()
            .position(|arg| arg == "--output-format")
            .unwrap();
        assert_eq!(args[output_index + 1], "json");
    }

    #[test]
    fn codex_and_claude_invocations_include_model_override_when_set() {
        let codex_state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("codex"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::from([("codex".to_string(), "gpt-5-codex".to_string())]),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let codex_paths = HashMap::from([(backend("codex"), "/usr/bin/codex".to_string())]);
        let (_, codex_args) = TelegramClient::build_cli_invocation(
            77,
            &codex_state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &codex_paths,
            "hello",
        )
        .unwrap();
        let codex_model_index = codex_args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(codex_args[codex_model_index + 1], "gpt-5-codex");

        let claude_state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("claude"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::from([(
                "claude".to_string(),
                "claude-sonnet-4-6".to_string(),
            )]),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let claude_paths = HashMap::from([(backend("claude"), "/usr/bin/claude".to_string())]);
        let (_, claude_args) = TelegramClient::build_cli_invocation(
            77,
            &claude_state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &claude_paths,
            "hello",
        )
        .unwrap();
        let claude_model_index = claude_args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(claude_args[claude_model_index + 1], "claude-sonnet-4-6");
    }

    #[test]
    fn gemini_json_response_and_usage_are_parsed() {
        let output = r#"{
  "session_id": "gemini-session",
  "response": "OK",
  "stats": {
    "models": {
      "gemini-2.5-flash": {
        "tokens": {
          "input": 10,
          "prompt": 10,
          "candidates": 2,
          "total": 15,
          "cached": 1,
          "thoughts": 3,
          "tool": 4
        }
      }
    }
  }
}"#;

        let registry = default_registry();
        let gemini = backend("gemini");
        assert_eq!(
            TelegramClient::extract_cli_response_text(&registry, &gemini, output, "").as_deref(),
            Some("OK")
        );
        let usage =
            TelegramClient::extract_cli_actual_usage(&registry, &gemini, output, "").unwrap();
        assert_eq!(usage.session_id.as_deref(), Some("gemini-session"));
        assert_eq!(usage.model.as_deref(), Some("gemini-2.5-flash"));
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 2);
        assert_eq!(usage.total_tokens, 15);
        assert_eq!(usage.cached_input_tokens, 1);
        assert_eq!(usage.thought_tokens, 3);
        assert_eq!(usage.tool_tokens, 4);
    }

    #[test]
    fn claude_json_response_and_usage_are_parsed() {
        let output = r#"{
  "result": "DONE",
  "session_id": "claude-session",
  "usage": {
    "input_tokens": 5,
    "output_tokens": 7,
    "cache_creation_input_tokens": 11,
    "cache_read_input_tokens": 13
  },
  "modelUsage": {
    "claude-sonnet-4-6": {
      "inputTokens": 5
    }
  }
}"#;

        let registry = default_registry();
        let claude = backend("claude");
        assert_eq!(
            TelegramClient::extract_cli_response_text(&registry, &claude, output, "").as_deref(),
            Some("DONE")
        );
        let usage =
            TelegramClient::extract_cli_actual_usage(&registry, &claude, output, "").unwrap();
        assert_eq!(usage.session_id.as_deref(), Some("claude-session"));
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(usage.input_tokens, 5);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.total_tokens, 12);
        assert_eq!(usage.cache_creation_input_tokens, 11);
        assert_eq!(usage.cache_read_input_tokens, 13);
    }

    #[test]
    fn coding_usage_report_includes_actual_cli_tokens() {
        let mut state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("gemini"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 2,
            usage: HashMap::new(),
        };
        let mut usage = TelegramCliUsageStats::default();
        usage.requests = 2;
        usage.successes = 2;
        usage.total_duration_ms = 120;
        usage.record_actual_usage(
            TelegramCliActualUsage {
                input_tokens: 10,
                output_tokens: 2,
                total_tokens: 15,
                cached_input_tokens: 1,
                thought_tokens: 3,
                model: Some("gemini-2.5-flash".to_string()),
                session_id: Some("gemini-session".to_string()),
                ..TelegramCliActualUsage::default()
            },
            123456,
        );
        state
            .usage
            .insert(backend("gemini").as_str().to_string(), usage);

        let report = TelegramClient::format_coding_usage_report(
            &state,
            &backend("gemini"),
            &default_registry(),
        );
        assert!(report.contains("Mode: [coding]"));
        assert!(report.contains("Session: [0002]"));
        assert!(report.contains("CodingAgent: [gemini]"));
        assert!(report.contains("ModelSource: [backend default]"));
        assert!(report.contains("Source: [stats.models.<model>.tokens]"));
        assert!(report.contains("Refresh: [updates after the next successful Gemini run]"));
        assert!(report.contains("LatestCLI: [gemini-session]"));
        assert!(report.contains("Model: [gemini-2.5-flash]"));
        assert!(report.contains("Latest: [in 10 | out 2 | total 15]"));
        assert!(report.contains("Cached: [1]"));
        assert!(report.contains("Thought: [3]"));
        assert!(report.contains("Remaining: [not reported by Gemini CLI]"));
        assert!(report.contains("Reset: [not reported by Gemini CLI]"));
        assert!(report.contains("TotalThought: [3]"));
    }

    #[test]
    fn gemini_capacity_errors_are_summarized() {
        let registry = default_registry();
        let message = TelegramClient::format_cli_result(
            &registry,
            &backend("gemini"),
            1,
            100,
            "",
            "No capacity available for model gemini-3-flash-preview",
        );

        assert!(message.contains("[gemini] Model capacity reached."));
        assert!(message.contains("gemini-2.5-flash"));
    }

    #[test]
    fn custom_backend_invocation_and_usage_can_be_loaded_from_config() {
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "backends": {
                "custom_agent": {
                    "binary_path": "/usr/bin/custom-agent",
                    "usage_hint": "`custom-agent run --cwd <project> --prompt <prompt>`",
                    "invocation": {
                        "args": ["run", "--cwd", "{project_dir}", "--prompt", "{prompt}"]
                    },
                    "response_extractors": [
                        { "source": "stdout", "format": "json", "text_path": "reply" }
                    ],
                    "usage_extractors": [
                        {
                            "source": "stdout",
                            "format": "json",
                            "input_tokens_path": "usage.prompt",
                            "output_tokens_path": "usage.completion",
                            "total_tokens_path": "usage.total",
                            "session_id_path": "session"
                        }
                    ]
                }
            }
        })));

        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("custom_agent"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
        };
        let backend_paths =
            HashMap::from([(backend("custom_agent"), "/usr/bin/custom-agent".to_string())]);

        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &registry,
            &backend_paths,
            "hello",
        )
        .unwrap();
        assert_eq!(binary, "/usr/bin/custom-agent");
        assert_eq!(args[0], "run");
        assert!(args.iter().any(|arg| arg == "/tmp/project"));

        let stdout =
            r#"{"reply":"DONE","session":"sess-1","usage":{"prompt":4,"completion":6,"total":10}}"#;
        assert_eq!(
            TelegramClient::extract_cli_response_text(
                &registry,
                &backend("custom_agent"),
                stdout,
                ""
            )
            .as_deref(),
            Some("DONE")
        );
        let usage = TelegramClient::extract_cli_actual_usage(
            &registry,
            &backend("custom_agent"),
            stdout,
            "",
        )
        .unwrap();
        assert_eq!(usage.input_tokens, 4);
        assert_eq!(usage.output_tokens, 6);
        assert_eq!(usage.total_tokens, 10);
        assert_eq!(usage.session_id.as_deref(), Some("sess-1"));
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

        let mut cli_backends = default_registry();
        if let Some(definition) = cli_backends.definitions.get_mut(&backend("gemini")) {
            definition.model = None;
        }
        TelegramClient::read_backend_models_from_llm_config(&temp_root, &mut cli_backends);

        assert_eq!(
            cli_backends
                .get(&backend("gemini"))
                .and_then(|definition| definition.model.as_deref()),
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
            targets[0]
                .1
                .session_label_for(TelegramInteractionMode::Chat),
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
        assert!(first.text.contains("Session: [0002]"));

        {
            let mut states = chat_states.lock().unwrap();
            let state = states.entry(77).or_default();
            state.interaction_mode = TelegramInteractionMode::Coding;
        }

        let second = TelegramClient::start_new_session(&chat_states, &state_path, 77);
        assert!(second.text.contains("Session: [0002]"));
    }
}
