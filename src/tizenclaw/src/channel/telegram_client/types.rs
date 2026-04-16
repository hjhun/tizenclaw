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
            "backend" => Some(Self::Coding),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Coding => "backend",
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
                    "configure via telegram_config.json cli_backends.backends.codex.model_choices"
                        .to_string(),
                model_choices: vec![],
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
                    "configure via telegram_config.json cli_backends.backends.gemini.model_choices"
                        .to_string(),
                model_choices: vec![],
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
                    "configure via telegram_config.json cli_backends.backends.claude.model_choices"
                        .to_string(),
                model_choices: vec![],
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
    pending_menu: Option<TelegramPendingMenu>,
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
            pending_menu: None,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TelegramPendingMenu {
    SelectMode,
    Model,
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
