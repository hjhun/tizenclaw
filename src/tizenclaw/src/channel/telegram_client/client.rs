pub struct TelegramClient {
    name: String,
    bot_token: String,
    allowed_chat_ids: Arc<HashSet<i64>>,
    max_message_chars: usize,
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
