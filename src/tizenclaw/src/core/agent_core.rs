//! Agent Core — the brain of TizenClaw.
//!
//! Manages LLM interaction, tool calling, session management,
//! and the agentic loop (prompt → LLM → tool call → result → LLM → ...).
//!
//! Thread-safety: uses fine-grained internal locking so callers can
//! share `Arc<AgentCore>` without an outer Mutex.

use serde_json::{json, Value};
use std::sync::{Arc, Mutex, RwLock};

use crate::infra::key_store::KeyStore;
use crate::llm::backend::{self, LlmBackend, LlmMessage, LlmResponse};
use crate::storage::session_store::SessionStore;
use crate::core::tool_dispatcher::ToolDispatcher;
use crate::core::fallback_parser::FallbackParser;
use crate::core::context_engine::{ContextEngine, SimpleContextEngine};

const MAX_TOOL_ROUNDS: usize = 10;
const MAX_CONTEXT_MESSAGES: usize = 20;

/// LLM backend configuration loaded from `llm_config.json`.
#[derive(Debug)]
struct LlmConfig {
    active_backend: String,
    fallback_backends: Vec<String>,
    backends: Value,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            active_backend: "gemini".into(),
            fallback_backends: vec![],
            backends: json!({}),
        }
    }
}

impl LlmConfig {
    /// Load LLM config from a JSON file.
    fn load(path: &str) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                log::warn!("LLM config not found at {}, using defaults", path);
                return Self::default();
            }
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed to parse LLM config: {}", e);
                return Self::default();
            }
        };

        LlmConfig {
            active_backend: json["active_backend"]
                .as_str()
                .unwrap_or("gemini")
                .to_string(),
            fallback_backends: json["fallback_backends"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            backends: json.get("backends").cloned().unwrap_or(json!({})),
        }
    }

    /// Get config for a specific backend.
    fn backend_config(&self, name: &str) -> Value {
        self.backends
            .get(name)
            .cloned()
            .unwrap_or(json!({}))
    }
}

#[derive(Debug, Clone)]
struct CircuitBreakerState {
    consecutive_failures: u32,
    last_failure_time: Option<std::time::Instant>,
}

/// Thread-safe AgentCore with fine-grained internal locking.
///
/// Callers share `Arc<AgentCore>` — no outer Mutex needed.
/// Each field that requires mutation is individually protected:
/// - `backend` + `fallback_backends`: Mutex (used during LLM calls)
/// - `session_store`: Mutex (SQLite is not Sync)
/// - `tool_dispatcher`: RwLock (reads are frequent, writes are rare)
pub struct AgentCore {
    platform: Arc<libtizenclaw::PlatformContext>,
    backend: tokio::sync::RwLock<Option<Box<dyn LlmBackend>>>,
    fallback_backends: tokio::sync::RwLock<Vec<Box<dyn LlmBackend>>>,
    session_store: Mutex<Option<SessionStore>>,
    tool_dispatcher: tokio::sync::RwLock<ToolDispatcher>,
    key_store: Mutex<KeyStore>,
    system_prompt: RwLock<String>,
    soul_content: RwLock<Option<String>>,
    backend_name: RwLock<String>,
    llm_config: Mutex<LlmConfig>,
    circuit_breakers: RwLock<std::collections::HashMap<String, CircuitBreakerState>>,
}

impl AgentCore {
    pub fn new(platform: Arc<libtizenclaw::PlatformContext>) -> Self {
        AgentCore {
            platform,
            backend: tokio::sync::RwLock::new(None),
            fallback_backends: tokio::sync::RwLock::new(Vec::new()),
            session_store: Mutex::new(None),
            tool_dispatcher: tokio::sync::RwLock::new(ToolDispatcher::new()),
            key_store: Mutex::new(KeyStore::new()),
            system_prompt: RwLock::new(String::new()),
            soul_content: RwLock::new(None),
            backend_name: RwLock::new(String::new()),
            llm_config: Mutex::new(LlmConfig::default()),
            circuit_breakers: RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub async fn initialize(&self) -> bool {
        log::info!("AgentCore initializing...");
        let paths = &self.platform.paths;

        // Load API keys
        let key_path = paths.config_dir.join("keys.json");
        if let Ok(mut ks) = self.key_store.lock() {
            ks.load(&key_path.to_string_lossy());
        }

        // Load system prompt
        let prompt_path = paths.config_dir.join("system_prompt.txt");
        let prompt = std::fs::read_to_string(&prompt_path).unwrap_or_else(|_| {
            "You are TizenClaw, an AI assistant that can execute tools \
             to help users interact with the system."
                .into()
        });
        if let Ok(mut sp) = self.system_prompt.write() {
            *sp = prompt;
        }

        // Load SOUL persona if present
        let soul_path = paths.config_dir.join("SOUL.md");
        if let Ok(soul) = std::fs::read_to_string(&soul_path) {
            log::info!("Loaded persona from SOUL.md");
            if let Ok(mut sc) = self.soul_content.write() {
                *sc = Some(soul);
            }
        }

        // Load LLM config (supports multi-backend + fallback)
        let llm_config_path = paths.config_dir.join("llm_config.json");
        let config = LlmConfig::load(&llm_config_path.to_string_lossy());
        let active_name = config.active_backend.clone();
        let fallback_names = config.fallback_backends.clone();

        // Initialize primary backend
        let primary = Self::create_and_init_backend_static(&config, &active_name);
        if primary.is_some() {
            log::info!("Primary LLM backend '{}' initialized", active_name);
        } else {
            log::error!("Primary LLM backend '{}' failed to initialize", active_name);
        }

        *self.backend.write().await = primary;
        if let Ok(mut bn) = self.backend_name.write() {
            *bn = active_name;
        }

        // Initialize fallback backends
        let mut fallbacks = Vec::new();
        for name in &fallback_names {
            if let Some(be) = Self::create_and_init_backend_static(&config, name) {
                log::info!("Fallback LLM backend '{}' initialized", name);
                fallbacks.push(be);
            }
        }
        *self.fallback_backends.write().await = fallbacks;

        // Store config for later use
        if let Ok(mut cfg) = self.llm_config.lock() {
            *cfg = config;
        }

        // Initialize session store
        let db_path = paths.sessions_db_path();
        match SessionStore::new(&db_path.to_string_lossy()) {
            Ok(store) => {
                log::info!("Session store initialized");
                if let Ok(mut ss) = self.session_store.lock() {
                    *ss = Some(store);
                }
            }
            Err(e) => log::error!("Session store failed: {}", e),
        }

        // Load tools from all subdirectories under the tools directory
        {
            let mut td = self.tool_dispatcher.write().await;
            td.load_tools_from_root(&paths.tools_dir.to_string_lossy());
        }
        log::info!("Tools loaded from {:?}", paths.tools_dir);

        true
    }

    /// Create and initialize an LLM backend by name using the provided config.
    fn create_and_init_backend_static(
        config: &LlmConfig,
        name: &str,
    ) -> Option<Box<dyn LlmBackend>> {
        let mut be = backend::create_backend(name)?;
        let cfg = config.backend_config(name);
        if be.initialize(&cfg) {
            Some(be)
        } else {
            log::warn!("Backend '{}' created but failed to initialize", name);
            None
        }
    }

    fn is_backend_available(&self, name: &str) -> bool {
        let cb_guard = self.circuit_breakers.read().unwrap();
        if let Some(state) = cb_guard.get(name) {
            if state.consecutive_failures >= 2 {
                if let Some(last_fail) = state.last_failure_time {
                    if last_fail.elapsed().as_secs() < 60 {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn record_success(&self, name: &str) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        let state = cb_guard.entry(name.to_string()).or_insert(CircuitBreakerState {
            consecutive_failures: 0,
            last_failure_time: None,
        });
        state.consecutive_failures = 0;
        state.last_failure_time = None;
    }

    fn record_failure(&self, name: &str) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        let state = cb_guard.entry(name.to_string()).or_insert(CircuitBreakerState {
            consecutive_failures: 0,
            last_failure_time: None,
        });
        state.consecutive_failures += 1;
        state.last_failure_time = Some(std::time::Instant::now());
    }

    /// Execute a chat request against the primary backend, falling back on failure.
    ///
    /// Acquires backend lock only for the duration of each `chat()` call.
    async fn chat_with_fallback(
        &self,
        messages: &[LlmMessage],
        tools: &[crate::llm::backend::LlmToolDecl],
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
        system_prompt: &str,
    ) -> LlmResponse {
        // Try primary backend — lock is held only during chat()
        {
            let bn = self.backend_name.read().map(|n| n.clone()).unwrap_or_default();
            if self.is_backend_available(&bn) {
                let be_guard = self.backend.read().await;
                if let Some(be) = be_guard.as_ref() {
                    let resp = be.chat(messages, tools, on_chunk, system_prompt).await;
                    if resp.success {
                        self.record_success(&bn);
                        return resp;
                    }
                    self.record_failure(&bn);
                    log::warn!(
                        "Primary backend '{}' failed (HTTP {}): {}",
                        bn, resp.http_status, resp.error_message
                    );
                }
            } else {
                log::warn!("Primary backend '{}' skipped due to Circuit Breaker", bn);
            }
        }
        // Primary lock is released here

        // Try fallback backends in order
        {
            let fbs_guard = self.fallback_backends.read().await;
            for fb in fbs_guard.iter() {
                let bn = fb.get_name().to_string();
                if self.is_backend_available(&bn) {
                    log::info!("Trying fallback backend '{}'", bn);
                    let resp = fb.chat(messages, tools, on_chunk, system_prompt).await;
                    if resp.success {
                        self.record_success(&bn);
                        return resp;
                    }
                    self.record_failure(&bn);
                    log::warn!(
                        "Fallback '{}' also failed: {}",
                        bn, resp.error_message
                    );
                } else {
                    log::warn!("Fallback backend '{}' skipped due to Circuit Breaker", bn);
                }
            }
        }

        LlmResponse {
            error_message: "All LLM backends failed".into(),
            ..Default::default()
        }
    }

    /// Process a user prompt through the agentic loop.
    ///
    /// Thread-safe: acquires fine-grained locks on individual fields
    /// rather than locking the entire AgentCore.
    pub async fn process_prompt(
        &self,
        session_id: &str,
        prompt: &str,
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> String {
        log::info!("Processing prompt for session '{}' ({} chars)", session_id, prompt.len());

        // Quick check: do we have any backend?
        {
            let has_primary = self.backend.read().await.is_some();
            let has_fallback = !self.fallback_backends.read().await.is_empty();
            if !has_primary && !has_fallback {
                return "Error: No LLM backend configured".into();
            }
        }

        // Store user message (short lock on session_store)
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                store.add_message(session_id, "user", prompt);
            }
        }

        // Build conversation history (short lock on session_store)
        let history = {
            let ss = self.session_store.lock();
            ss.ok()
                .and_then(|s| s.as_ref().map(|store| store.get_messages(session_id, MAX_CONTEXT_MESSAGES)))
                .unwrap_or_default()
        };

        let mut messages: Vec<LlmMessage> = history
            .iter()
            .map(|m| LlmMessage {
                role: m.role.clone(),
                text: m.text.clone(),
                ..Default::default()
            })
            .collect();

        // If history is empty or doesn't end with user message, add it
        if messages.is_empty() || messages.last().map(|m| m.role.as_str()) != Some("user") {
            messages.push(LlmMessage::user(prompt));
        }

        // Get tool declarations (read lock — allows concurrent reads)
        let mut tools = self.tool_dispatcher.read().await.get_tool_declarations();
        crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_builtin_tools(&mut tools);

        // Build System Prompt Dynamically
        let system_prompt = {
            let mut builder = crate::core::prompt_builder::SystemPromptBuilder::new();
            if let Ok(base) = self.system_prompt.read() {
                builder = builder.set_base_prompt(base.clone());
            }
            if let Ok(soul_lock) = self.soul_content.read() {
                if let Some(ref soul) = *soul_lock {
                    builder = builder.set_soul_content(soul.clone());
                }
            }
            let tool_names = tools.iter().map(|t| t.name.clone()).collect();
            builder = builder.add_tool_names(tool_names);
            
            let skills_dir = self.platform.paths.skills_dir.to_string_lossy().to_string();
            let textual_skills = crate::core::textual_skill_scanner::scan_textual_skills(&skills_dir);
            let formatted_skills = textual_skills.into_iter()
                .map(|s| (s.absolute_path, s.description))
                .collect();
            builder = builder.add_available_skills(formatted_skills);
            
            let model_name = self.backend_name.read().unwrap().clone();
            let platform_name = self.platform.platform_name().to_string();
            let data_dir = self.platform.paths.data_dir.to_string_lossy().to_string();
            builder = builder.set_runtime_context(
                platform_name,
                model_name,
                data_dir,
            );
            builder.build()
        };

        // --- ENHANCEMENT: Proactive Compaction (Step 1: Before loop) ---
        let context_engine = SimpleContextEngine::new();
        let budget = 32000; // Default budget (can be made configurable)
        if context_engine.should_compact(&messages, budget) {
            messages = context_engine.compact(messages, budget);
        }

        // Agentic loop — no global lock held during LLM calls
        for round in 0..MAX_TOOL_ROUNDS {
            let response = self.chat_with_fallback(&messages, &tools, on_chunk, &system_prompt).await;

            if !response.success {
                let err = format!(
                    "LLM error (HTTP {}): {}",
                    response.http_status, response.error_message
                );
                log::error!("{}", err);
                return err;
            }

            // --- ENHANCEMENT: Reasoning Extraction ---
            let mut reasoning_text = response.reasoning_text.clone();
            if reasoning_text.is_empty() {
                // Regex fallback for <think> tags
                let think_re = regex::Regex::new(r"(?s)<think>(.*?)</think>").unwrap();
                if let Some(cap) = think_re.captures(&response.text) {
                    reasoning_text = cap[1].trim().to_string();
                }
            }

            // --- ENHANCEMENT: Fallback Parsing ---
            let mut detected_tool_calls = response.tool_calls.clone();
            if detected_tool_calls.is_empty() {
                detected_tool_calls = FallbackParser::parse(&response.text);
                if !detected_tool_calls.is_empty() {
                    log::info!("FallbackParser: Detected {} tool calls from text", detected_tool_calls.len());
                }
            }

            // Record token usage (short lock on session_store)
            {
                let be_name = self.backend.read().await
                    .as_ref()
                    .map(|be| be.get_name().to_string())
                    .unwrap_or_else(|| "unknown".into());
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.record_usage(
                            session_id,
                            response.prompt_tokens,
                            response.completion_tokens,
                            &be_name,
                        );
                    }
                }
            }

            if !detected_tool_calls.is_empty() {
                log::info!("Round {}: {} tool call(s)", round, detected_tool_calls.len());

                // Add assistant message with tool calls and reasoning
                messages.push(LlmMessage {
                    role: "assistant".into(),
                    text: response.text.clone(),
                    reasoning_text,
                    tool_calls: detected_tool_calls.clone(),
                    ..Default::default()
                });

                // Execute tool calls — parallel with tokio join_all
                let td_guard = self.tool_dispatcher.read().await;
                let mut futures_list = Vec::new();
                for tc in detected_tool_calls.iter() {
                    log::info!("Executing tool (async): {} (id: {})", tc.name, tc.id);
                    let skills_dir = self.platform.paths.skills_dir.clone();
                    let td_guard_ref = &*td_guard;
                    let tc_name = tc.name.clone();
                    let tc_args = tc.args.clone();
                    let tc_id = tc.id.clone();
                    
                    futures_list.push(async move {
                        let result = if tc_name == "create_skill" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed_skill");
                            let content = tc_args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            let sanitized_name = name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "");
                            
                            if sanitized_name.is_empty() {
                                serde_json::json!({"error": "Invalid skill name"})
                            } else {
                                let skill_dir_path = skills_dir.join(&sanitized_name);
                                if let Err(e) = std::fs::create_dir_all(&skill_dir_path) {
                                    serde_json::json!({"error": format!("Failed to create skill directory: {}", e)})
                                } else {
                                    let skill_md_path = skill_dir_path.join("SKILL.md");
                                    match std::fs::write(&skill_md_path, content) {
                                        Ok(_) => serde_json::json!({"status": "success", "message": format!("Skill '{}' created successfully at {:?}", sanitized_name, skill_md_path)}),
                                        Err(e) => serde_json::json!({"error": format!("Failed to write skill content: {}", e)})
                                    }
                                }
                            }
                        } else if tc_name == "read_skill" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let sanitized_name = name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "");
                            let skill_md_path = skills_dir.join(&sanitized_name).join("SKILL.md");
                            match std::fs::read_to_string(&skill_md_path) {
                                Ok(content) => serde_json::json!({"status": "success", "content": content}),
                                Err(e) => serde_json::json!({"error": format!("Failed to read skill '{}': {}", sanitized_name, e)})
                            }
                        } else {
                            td_guard_ref.execute(&tc_name, &tc_args).await
                        };
                        LlmMessage::tool_result(&tc_id, &tc_name, result)
                    });
                }
                let results = futures_util::future::join_all(futures_list).await;
                messages.extend(results);
                // Continue loop for next LLM response
            } else {
                // Final text response
                let text = response.text;
                let mut reasoning_text = response.reasoning_text.clone();
                if reasoning_text.is_empty() {
                    let think_re = regex::Regex::new(r"(?s)<think>(.*?)</think>").unwrap();
                    if let Some(cap) = think_re.captures(&text) {
                        reasoning_text = cap[1].trim().to_string();
                    }
                }

                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        // Store reasoning in the message too
                        let mut msg = LlmMessage::assistant(&text);
                        msg.reasoning_text = reasoning_text;
                        store.add_message(session_id, "assistant", &text);
                        // TODO: Update SessionStore to optionally store reasoning_text properly
                    }
                }
                return text;
            }

            // --- ENHANCEMENT: Proactive Compaction (Step 2: During loop) ---
            if context_engine.should_compact(&messages, budget) {
                messages = context_engine.compact(messages, budget);
            }
        }

        "Error: Maximum tool call rounds exceeded".into()
    }

    pub async fn shutdown(&self) {
        log::info!("AgentCore shutting down");
        if let Some(b) = self.backend.write().await.as_mut() {
            b.shutdown();
        }
        for fb in self.fallback_backends.write().await.iter_mut() {
            fb.shutdown();
        }
    }

    pub fn get_session_store(&self) -> Option<SessionStoreRef<'_>> {
        let guard = self.session_store.lock().ok()?;
        if guard.is_some() {
            Some(SessionStoreRef { guard })
        } else {
            None
        }
    }

    pub async fn reload_tools(&self) {
        {
            let mut td = self.tool_dispatcher.write().await;
            *td = ToolDispatcher::new();
            td.load_tools_from_root(&self.platform.paths.tools_dir.to_string_lossy());
        }
        log::info!("Tools reloaded from {:?}", self.platform.paths.tools_dir);
    }

    pub async fn run_startup_indexing(&self) {
        let has_primary = self.backend.read().await.is_some();
        let has_fallback = !self.fallback_backends.read().await.is_empty();
        if !has_primary && !has_fallback {
            log::info!("[Startup Indexing] Skipped: No actively connected LLM found.");
            return;
        }

        log::info!("[Startup Indexing] LLM connected. Requesting dynamic indexing of /opt/usr/share/tizen-tools/...");
        
        let prompt = "Please check the directories under `/opt/usr/share/tizen-tools/` (specifically the `skills/` and `cli/` folders) and update the `tools.md` and `index.md` files located in `/opt/usr/share/tizen-tools/` (or its relevant documentation paths) to reflect the current state of tools exactly. Read the directories first, then overwrite the markdown index files cleanly. Do not ask for permissions, simply execute via your file manager or execution tools.";
        
        // We use a predefined system session ID so it doesn't pollute user chats.
        let session_id = "system_startup_indexer";
        let _ = self.process_prompt(session_id, prompt, None).await;
        
        log::info!("[Startup Indexing] Completed autonomous documentation updates.");
    }
}

/// RAII guard providing access to the SessionStore while holding the lock.
pub struct SessionStoreRef<'a> {
    guard: std::sync::MutexGuard<'a, Option<SessionStore>>,
}

impl<'a> SessionStoreRef<'a> {
    pub fn store(&self) -> &SessionStore {
        self.guard.as_ref().unwrap()
    }
}
