//! Agent Core — the brain of TizenClaw.
//!
//! Manages LLM interaction, tool calling, session management,
//! and the agentic loop (prompt → LLM → tool call → result → LLM → ...).

use serde_json::{json, Value};

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

pub struct AgentCore {
    backend: Option<Box<dyn LlmBackend>>,
    fallback_backends: Vec<Box<dyn LlmBackend>>,
    session_store: Option<SessionStore>,
    tool_dispatcher: ToolDispatcher,
    key_store: KeyStore,
    system_prompt: String,
    backend_name: String,
    llm_config: LlmConfig,
}

impl AgentCore {
    pub fn new() -> Self {
        AgentCore {
            backend: None,
            fallback_backends: Vec::new(),
            session_store: None,
            tool_dispatcher: ToolDispatcher::new(),
            key_store: KeyStore::new(),
            system_prompt: String::new(),
            backend_name: String::new(),
            llm_config: LlmConfig::default(),
        }
    }

    pub fn initialize(&mut self) -> bool {
        log::info!("AgentCore initializing...");

        // Load API keys
        let key_path = format!("{}/config/keys.json", APP_DATA_DIR);
        self.key_store.load(&key_path);

        // Load system prompt
        let prompt_path = format!("{}/config/system_prompt.txt", APP_DATA_DIR);
        self.system_prompt = std::fs::read_to_string(&prompt_path).unwrap_or_else(|_| {
            "You are TizenClaw, an AI assistant for Tizen devices. \
             You can execute tools to help users interact with the device."
                .into()
        });

        // Load LLM config (supports multi-backend + fallback)
        let llm_config_path = format!("{}/config/llm_config.json", APP_DATA_DIR);
        self.llm_config = LlmConfig::load(&llm_config_path);
        self.backend_name = self.llm_config.active_backend.clone();

        // Initialize primary backend
        self.backend = self.create_and_init_backend(&self.backend_name.clone());
        if self.backend.is_some() {
            log::info!("Primary LLM backend '{}' initialized", self.backend_name);
        } else {
            log::error!("Primary LLM backend '{}' failed to initialize", self.backend_name);
        }

        // Initialize fallback backends
        let fallback_names = self.llm_config.fallback_backends.clone();
        for name in &fallback_names {
            if let Some(be) = self.create_and_init_backend(name) {
                log::info!("Fallback LLM backend '{}' initialized", name);
                self.fallback_backends.push(be);
            }
        }

        // Initialize session store
        let db_path = format!("{}/sessions.db", APP_DATA_DIR);
        match SessionStore::new(&db_path) {
            Ok(store) => {
                log::info!("Session store initialized");
                self.session_store = Some(store);
            }
            Err(e) => log::error!("Session store failed: {}", e),
        }

        // Load tools from all subdirectories under /opt/usr/share/tizen-tools
        self.tool_dispatcher.load_tools_from_root("/opt/usr/share/tizen-tools");
        log::info!("Tools loaded");

        true
    }

    /// Create and initialize an LLM backend by name using the loaded config.
    fn create_and_init_backend(&self, name: &str) -> Option<Box<dyn LlmBackend>> {
        let mut be = backend::create_backend(name)?;
        let config = self.llm_config.backend_config(name);
        if be.initialize(&config) {
            Some(be)
        } else {
            log::warn!("Backend '{}' created but failed to initialize", name);
            None
        }
    }

    /// Execute a chat request against the primary backend, falling back on failure.
    fn chat_with_fallback(
        &self,
        messages: &[LlmMessage],
        tools: &[crate::llm::backend::LlmToolDecl],
        on_chunk: Option<&dyn Fn(&str)>,
    ) -> LlmResponse {
        // Try primary backend
        if let Some(be) = &self.backend {
            let resp = be.chat(messages, tools, on_chunk, &self.system_prompt);
            if resp.success {
                return resp;
            }
            log::warn!(
                "Primary backend '{}' failed (HTTP {}): {}",
                self.backend_name, resp.http_status, resp.error_message
            );
        }

        // Try fallback backends in order
        for fb in &self.fallback_backends {
            log::info!("Trying fallback backend '{}'", fb.get_name());
            let resp = fb.chat(messages, tools, on_chunk, &self.system_prompt);
            if resp.success {
                return resp;
            }
            log::warn!("Fallback '{}' also failed: {}", fb.get_name(), resp.error_message);
        }

        LlmResponse {
            error_message: "All LLM backends failed".into(),
            ..Default::default()
        }
    }

    /// Process a user prompt through the agentic loop.
    pub fn process_prompt(
        &self,
        session_id: &str,
        prompt: &str,
        on_chunk: Option<&dyn Fn(&str)>,
    ) -> String {
        log::info!("Processing prompt for session '{}' ({} chars)", session_id, prompt.len());

        if self.backend.is_none() && self.fallback_backends.is_empty() {
            return "Error: No LLM backend configured".into();
        }

        // Store user message
        if let Some(store) = &self.session_store {
            store.add_message(session_id, "user", prompt);
        }

        // Build conversation history
        let history = self
            .session_store
            .as_ref()
            .map(|s| s.get_messages(session_id, MAX_CONTEXT_MESSAGES))
            .unwrap_or_default();

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

        let tools = self.tool_dispatcher.get_tool_declarations();

        // Agentic loop
        for round in 0..MAX_TOOL_ROUNDS {
            let response = self.chat_with_fallback(&messages, &tools, on_chunk);

            if !response.success {
                let err = format!(
                    "LLM error (HTTP {}): {}",
                    response.http_status, response.error_message
                );
                log::error!("{}", err);
                return err;
            }

            // Record token usage
            if let Some(store) = &self.session_store {
                let backend_name = self
                    .backend
                    .as_ref()
                    .map(|b| b.get_name())
                    .unwrap_or("unknown");
                store.record_usage(
                    session_id,
                    response.prompt_tokens,
                    response.completion_tokens,
                    backend_name,
                );
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

                // Execute each tool call and add results
                for tc in &response.tool_calls {
                    log::info!("Executing tool: {} (id: {})", tc.name, tc.id);
                    let result = self.tool_dispatcher.execute(&tc.name, &tc.args);
                    messages.push(LlmMessage::tool_result(&tc.id, &tc.name, result));
                }
                // Continue loop for next LLM response
            } else {
                // Final text response
                let text = response.text;
                if let Some(store) = &self.session_store {
                    store.add_message(session_id, "assistant", &text);
                }
                return text;
            }
        }

        "Error: Maximum tool call rounds exceeded".into()
    }

    pub fn shutdown(&mut self) {
        log::info!("AgentCore shutting down");
        if let Some(be) = &mut self.backend {
            be.shutdown();
        }
        for fb in &mut self.fallback_backends {
            fb.shutdown();
        }
    }

    pub fn get_session_store(&self) -> &Option<SessionStore> {
        &self.session_store
    }

    pub fn reload_tools(&mut self) {
        self.tool_dispatcher = ToolDispatcher::new();
        self.tool_dispatcher.load_tools_from_root("/opt/usr/share/tizen-tools");
        log::info!("Tools reloaded");
    }
}
