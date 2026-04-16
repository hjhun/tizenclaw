impl TelegramClient {
    pub fn from_config(config: &Value) -> Result<Self, String> {
        let bot_token = config
            .get("bot_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "telegram bot_token is required".to_string())?;
        let chat_id = config
            .get("chat_id")
            .and_then(Value::as_i64)
            .or_else(|| {
                config
                    .get("chat_id")
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse::<i64>().ok())
            });
        let allowed_chat_ids = config.get("allowed_chat_ids").and_then(Value::as_array);
        if chat_id.is_none() && allowed_chat_ids.is_none() {
            return Err("telegram chat_id or allowed_chat_ids is required".to_string());
        }

        let mut settings = serde_json::Map::new();
        settings.insert("bot_token".to_string(), Value::String(bot_token.to_string()));
        if let Some(chat_id) = chat_id {
            settings.insert(
                "allowed_chat_ids".to_string(),
                Value::Array(vec![Value::Number(chat_id.into())]),
            );
        } else if let Some(ids) = allowed_chat_ids {
            settings.insert("allowed_chat_ids".to_string(), Value::Array(ids.to_vec()));
        }
        if let Some(max_chars) = config.get("max_message_chars").cloned() {
            settings.insert("max_message_chars".to_string(), max_chars);
        }

        let channel_config = ChannelConfig {
            name: config
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("telegram")
                .to_string(),
            channel_type: "telegram".to_string(),
            enabled: true,
            settings: Value::Object(settings),
        };

        Ok(Self::new(&channel_config, None))
    }

    pub fn new(
        config: &ChannelConfig,
        agent: Option<Arc<crate::core::agent_core::AgentCore>>,
    ) -> Self {
        let explicit_bot_token = config
            .settings
            .get("bot_token")
            .and_then(|v| v.as_str())
            .is_some_and(|value| !value.trim().is_empty());
        let explicit_allowed_ids = config
            .settings
            .get("allowed_chat_ids")
            .and_then(|v| v.as_array())
            .is_some_and(|value| !value.is_empty());
        let mut bot_token = config
            .settings
            .get("bot_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let max_message_chars = config
            .settings
            .get("max_message_chars")
            .and_then(|v| v.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(TELEGRAM_MAX_MESSAGE_CHARS);

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
        // Merge llm_config.json model overrides before telegram_config.json so
        // that telegram_config.json has higher precedence (later merge wins).
        Self::read_backend_models_from_llm_config(&config_dir, &mut cli_backends);

        let telegram_config = config_dir.join("telegram_config.json");
        if let Ok(content) = std::fs::read_to_string(&telegram_config) {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                if !explicit_bot_token {
                    if let Some(token) = json.get("bot_token").and_then(|v| v.as_str()) {
                        if !token.is_empty() {
                            bot_token = token.to_string();
                            log::info!("TelegramClient: loaded bot_token override");
                        }
                    }
                }
                if !explicit_allowed_ids {
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
            max_message_chars,
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
        let llm_config = config_dir.join("llm_config.json");
        let Ok(content) = std::fs::read_to_string(&llm_config) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&content) else {
            return;
        };

        // Merge telegram-scoped cli_backends overrides (model_choices etc.)
        // from the optional `telegram.cli_backends` key in llm_config.json.
        if let Some(telegram_section) = json.get("telegram") {
            cli_backends.merge_config_value(telegram_section.get("cli_backends"));
        }

        // Gemini model fallback: if the operator has not set a model for the
        // gemini backend (neither in telegram_config.json nor above), try the
        // top-level `backends.gemini.model` key for backwards compatibility.
        let gemini_backend = TelegramCliBackend::new("gemini");
        let gemini_model_set = cli_backends
            .get(&gemini_backend)
            .and_then(|definition| definition.model.as_deref())
            .is_some();
        if !gemini_model_set {
            if let Some(model) = json
                .get("backends")
                .and_then(|v| v.get("gemini"))
                .and_then(|v| v.get("model"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if let Some(definition) = cli_backends.definitions.get_mut(&gemini_backend) {
                    definition.model = Some(model.to_string());
                }
            }
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
        (
            cli_workdir,
            cli_timeout_secs,
            cli_backends,
            cli_backend_paths,
        )
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
}
