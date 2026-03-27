//! Agent Core — the brain of TizenClaw.
//!
//! Manages LLM interaction, tool calling, session management,
//! and the agentic loop (prompt → LLM → tool call → result → LLM → ...).
//!
//! Thread-safety: uses fine-grained internal locking so callers can
//! share `Arc<AgentCore>` without an outer Mutex.

use serde_json::{json, Value};
use std::sync::{Mutex, RwLock};

use crate::infra::key_store::KeyStore;
use crate::llm::backend::{self, LlmBackend, LlmMessage, LlmResponse};
use crate::storage::session_store::SessionStore;
use crate::core::tool_dispatcher::ToolDispatcher;

const APP_DATA_DIR: &str = "/opt/usr/share/tizenclaw";
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

/// Thread-safe AgentCore with fine-grained internal locking.
///
/// Callers share `Arc<AgentCore>` — no outer Mutex needed.
/// Each field that requires mutation is individually protected:
/// - `backend` + `fallback_backends`: Mutex (used during LLM calls)
/// - `session_store`: Mutex (SQLite is not Sync)
/// - `tool_dispatcher`: RwLock (reads are frequent, writes are rare)
pub struct AgentCore {
    backend: tokio::sync::RwLock<Option<Box<dyn LlmBackend>>>,
    fallback_backends: tokio::sync::RwLock<Vec<Box<dyn LlmBackend>>>,
    session_store: Mutex<Option<SessionStore>>,
    tool_dispatcher: RwLock<ToolDispatcher>,
    key_store: Mutex<KeyStore>,
    system_prompt: RwLock<String>,
    backend_name: RwLock<String>,
    llm_config: Mutex<LlmConfig>,
}

impl Default for AgentCore {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentCore {
    pub fn new() -> Self {
        AgentCore {
            backend: tokio::sync::RwLock::new(None),
            fallback_backends: tokio::sync::RwLock::new(Vec::new()),
            session_store: Mutex::new(None),
            tool_dispatcher: RwLock::new(ToolDispatcher::new()),
            key_store: Mutex::new(KeyStore::new()),
            system_prompt: RwLock::new(String::new()),
            backend_name: RwLock::new(String::new()),
            llm_config: Mutex::new(LlmConfig::default()),
        }
    }

    pub async fn initialize(&self) -> bool {
        log::info!("AgentCore initializing...");

        // Load API keys
        let key_path = format!("{}/config/keys.json", APP_DATA_DIR);
        if let Ok(mut ks) = self.key_store.lock() {
            ks.load(&key_path);
        }

        // Load system prompt
        let prompt_path = format!("{}/config/system_prompt.txt", APP_DATA_DIR);
        let prompt = std::fs::read_to_string(&prompt_path).unwrap_or_else(|_| {
            "You are TizenClaw, an AI assistant for Tizen devices. \
             You can execute tools to help users interact with the device."
                .into()
        });
        if let Ok(mut sp) = self.system_prompt.write() {
            *sp = prompt;
        }

        // Load LLM config (supports multi-backend + fallback)
        let llm_config_path = format!("{}/config/llm_config.json", APP_DATA_DIR);
        let config = LlmConfig::load(&llm_config_path);
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
        let db_path = format!("{}/sessions.db", APP_DATA_DIR);
        match SessionStore::new(&db_path) {
            Ok(store) => {
                log::info!("Session store initialized");
                if let Ok(mut ss) = self.session_store.lock() {
                    *ss = Some(store);
                }
            }
            Err(e) => log::error!("Session store failed: {}", e),
        }

        // Load tools from all subdirectories under /opt/usr/share/tizen-tools
        if let Ok(mut td) = self.tool_dispatcher.write() {
            td.load_tools_from_root("/opt/usr/share/tizen-tools");
        }
        log::info!("Tools loaded");

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

    /// Execute a chat request against the primary backend, falling back on failure.
    ///
    /// Acquires backend lock only for the duration of each `chat()` call.
    async fn chat_with_fallback(
        &self,
        messages: &[LlmMessage],
        tools: &[crate::llm::backend::LlmToolDecl],
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> LlmResponse {
        let system_prompt = self.system_prompt.read()
            .map(|sp| sp.clone())
            .unwrap_or_default();

        // Try primary backend — lock is held only during chat()
        {
            let be_guard = self.backend.read().await;
            if let Some(be) = be_guard.as_ref() {
                let resp = be.chat(messages, tools, on_chunk, &system_prompt).await;
                if resp.success {
                    return resp;
                }
                let bn = self.backend_name.read()
                    .map(|n| n.clone())
                    .unwrap_or_default();
                log::warn!(
                    "Primary backend '{}' failed (HTTP {}): {}",
                    bn, resp.http_status, resp.error_message
                );
            }
        }
        // Primary lock is released here

        // Try fallback backends in order
        {
            let fbs_guard = self.fallback_backends.read().await;
            for fb in fbs_guard.iter() {
                log::info!("Trying fallback backend '{}'", fb.get_name());
                let resp = fb.chat(messages, tools, on_chunk, &system_prompt).await;
                if resp.success {
                    return resp;
                }
                log::warn!(
                    "Fallback '{}' also failed: {}",
                    fb.get_name(), resp.error_message
                );
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
        let tools = self.tool_dispatcher.read()
            .map(|td| td.get_tool_declarations())
            .unwrap_or_default();

        // Agentic loop — no global lock held during LLM calls
        for round in 0..MAX_TOOL_ROUNDS {
            let response = self.chat_with_fallback(&messages, &tools, on_chunk).await;

            if !response.success {
                let err = format!(
                    "LLM error (HTTP {}): {}",
                    response.http_status, response.error_message
                );
                log::error!("{}", err);
                return err;
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

            if response.has_tool_calls() {
                log::info!("Round {}: {} tool call(s)", round, response.tool_calls.len());

                // Add assistant message with tool calls
                messages.push(LlmMessage {
                    role: "assistant".into(),
                    text: response.text.clone(),
                    tool_calls: response.tool_calls.clone(),
                    ..Default::default()
                });

                // Execute tool calls — parallel when multiple, sequential when single
                if response.tool_calls.len() == 1 {
                    let tc = &response.tool_calls[0];
                    log::info!("Executing tool: {} (id: {})", tc.name, tc.id);
                    let result = if let Ok(td) = self.tool_dispatcher.read() {
                        td.execute(&tc.name, &tc.args)
                    } else {
                        json!({"error": "Tool dispatcher unavailable"})
                    };
                    messages.push(LlmMessage::tool_result(&tc.id, &tc.name, result));
                } else {
                    // Parallel execution for multiple tool calls
                    let results: Vec<_> = std::thread::scope(|s| {
                        let handles: Vec<_> = response.tool_calls.iter().map(|tc| {
                            log::info!("Executing tool (parallel): {} (id: {})", tc.name, tc.id);
                            s.spawn(|| {
                                if let Ok(td) = self.tool_dispatcher.read() {
                                    td.execute(&tc.name, &tc.args)
                                } else {
                                    json!({"error": "Tool dispatcher unavailable"})
                                }
                            })
                        }).collect();
                        handles.into_iter().map(|h| h.join().unwrap_or(json!({"error": "Thread panicked"}))).collect()
                    });
                    for (tc, result) in response.tool_calls.iter().zip(results) {
                        messages.push(LlmMessage::tool_result(&tc.id, &tc.name, result));
                    }
                }
                // Continue loop for next LLM response
            } else {
                // Final text response
                let text = response.text;
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.add_message(session_id, "assistant", &text);
                    }
                }
                return text;
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

    pub fn reload_tools(&self) {
        if let Ok(mut td) = self.tool_dispatcher.write() {
            *td = ToolDispatcher::new();
            td.load_tools_from_root("/opt/usr/share/tizen-tools");
        }
        log::info!("Tools reloaded");
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
