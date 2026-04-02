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
use crate::core::context_engine::{ContextEngine, SizedContextEngine};
use crate::core::agent_loop_state::{AgentLoopState, AgentPhase, EvalVerdict};

const MAX_CONTEXT_MESSAGES: usize = 20;
const CONTEXT_TOKEN_BUDGET: usize = 256_000;
const CONTEXT_COMPACT_THRESHOLD: f32 = 0.90;
const MAX_TOOL_RETRY: usize = 3;

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

/// Merge an `api_key` from `KeyStore` into a backend config `Value`.
///
/// Priority:
///  1. Explicit `api_key` in the JSON config block (non-empty) — unchanged.
///  2. `keys.json` entry keyed by backend name (or env var via `KeyStore::get`).
///  3. Nothing found — config returned as-is (backend will fail init gracefully).
fn merge_api_key(mut cfg: Value, name: &str, ks: &crate::infra::key_store::KeyStore) -> Value {
    // If the config already contains a non-empty api_key, trust it as-is.
    if cfg.get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
    {
        return cfg;
    }
    // Fall back to KeyStore (also checks env vars internally).
    if let Some(key) = ks.get(name) {
        if !key.is_empty() {
            cfg["api_key"] = Value::String(key);
        }
    }
    cfg
}

#[derive(Debug, Clone)]
struct CircuitBreakerState {
    consecutive_failures: u32,
    last_failure_time: Option<std::time::Instant>,
}

struct BackendCandidate {
    name: String,
    priority: i64,
}

/// Thread-safe AgentCore with fine-grained internal locking.
///
/// Callers share `Arc<AgentCore>` — no outer Mutex needed.
/// Each field that requires mutation is individually protected:
/// - `backend` + `fallback_backends`: Mutex (used during LLM calls)
/// - `session_store`: Mutex (SQLite is not Sync)
/// - `tool_dispatcher`: RwLock (reads are frequent, writes are rare)
pub struct AgentCore {
    platform: Arc<libtizenclaw_core::framework::PlatformContext>,
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
    action_bridge: Mutex<crate::core::action_bridge::ActionBridge>,
    tool_policy: Mutex<crate::core::tool_policy::ToolPolicy>,
}

impl AgentCore {
    pub fn new(platform: Arc<libtizenclaw_core::framework::PlatformContext>) -> Self {
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
            action_bridge: Mutex::new(crate::core::action_bridge::ActionBridge::new()),
            tool_policy: Mutex::new(crate::core::tool_policy::ToolPolicy::new()),
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

        let policy_path = paths.config_dir.join("tool_policy.json");
        if let Ok(mut tp) = self.tool_policy.lock() {
            tp.load_config(&policy_path.to_string_lossy());
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

        // Initialize plugin manager
        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        // Plugins are exclusively scanned via PackageManager via `scan_plugins`.
        plugin_manager.scan_plugins(Some(self.platform.package_manager.as_ref()));

        // Unified priority-based selection
        let candidates = self.get_backend_candidates(&config, &plugin_manager);

        // 5. Initialize backends iteratively
        let mut primary_initialized = false;
        let mut fallbacks = Vec::new();

        for cand in candidates {
            // Acquire KeyStore briefly — clone the api_key value, then drop the guard.
            let merged_cfg = {
                let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
                let base = config.backend_config(&cand.name);
                merge_api_key(base, &cand.name, &ks_guard)
            };

            if let Some(be) = Self::create_and_init_backend_static(&plugin_manager, &cand.name, merged_cfg) {
                if !primary_initialized {
                    log::info!("Primary LLM backend '{}' initialized (priority {})", cand.name, cand.priority);
                    *self.backend.write().await = Some(be);
                    if let Ok(mut bn) = self.backend_name.write() {
                        *bn = cand.name.clone();
                    }
                    primary_initialized = true;
                } else {
                    log::info!("Fallback LLM backend '{}' initialized (priority {})", cand.name, cand.priority);
                    fallbacks.push(be);
                }
            }
        }

        if !primary_initialized {
            log::error!("Failed to initialize ANY backend from candidates list!");
            *self.backend.write().await = None;
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

        {
            let mut bridge = self.action_bridge.lock().unwrap();
            bridge.start();
        }

        true
    }

    /// Dynamically handle package manager events for plugins
    pub async fn handle_pkgmgr_event(&self, event_name: &str, pkgid: &str) {
        log::info!("Handling pkgmgr event: {} for pkgid: {}", event_name, pkgid);
        
        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        let loaded = if event_name == "install" || event_name == "recoverinstall" || event_name == "upgrade" || event_name == "recoverupgrade" {
            plugin_manager.load_plugin_from_pkg(Some(self.platform.package_manager.as_ref()), pkgid)
        } else {
            false
        };

        let unloaded = if event_name == "uninstall" || event_name == "recoveruninstall" {
            // Note: PluginManager removes from registry, but we do a full reload of backends anyway
            true
        } else {
            false
        };

        if loaded || unloaded {
            log::info!("Triggering LLM backend reload due to plugin changes...");
            self.reload_backends().await;
        }

        // --- NEW: Handle Tool Extensibility Indexing via PkgMgr ---
        // If a package is installed/uninstalled, we re-evaluate if index.md and tools.md
        // need to be rebuilt. This removes the need for periodic filesystem polling.
        if loaded || unloaded {
            self.reload_tools().await;
            self.run_startup_indexing().await;
        }
    }

    /// Reload LLM backends dynamically
    pub async fn reload_backends(&self) {
        let paths = &self.platform.paths;
        let llm_config_path = paths.config_dir.join("llm_config.json");
        let config = LlmConfig::load(&llm_config_path.to_string_lossy());
        
        // Re-scan plugins
        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        plugin_manager.scan_plugins(Some(self.platform.package_manager.as_ref()));

        let active_name = config.active_backend.clone();
        let fallback_names = config.fallback_backends.clone();
        
        // Unified priority-based selection
        let candidates = self.get_backend_candidates(&config, &plugin_manager);

        let mut primary_initialized = false;
        let mut fallbacks = Vec::new();

        for cand in candidates {
            // Acquire KeyStore briefly — merge api_key, then drop guard.
            let merged_cfg = {
                let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
                let base = config.backend_config(&cand.name);
                merge_api_key(base, &cand.name, &ks_guard)
            };

            if let Some(be) = Self::create_and_init_backend_static(&plugin_manager, &cand.name, merged_cfg) {
                if !primary_initialized {
                    log::info!("Dynamically swapped Primary LLM backend to '{}' (priority {})", cand.name, cand.priority);
                    *self.backend.write().await = Some(be);
                    if let Ok(mut bn) = self.backend_name.write() {
                        *bn = cand.name.clone();
                    }
                    primary_initialized = true;
                } else {
                    fallbacks.push(be);
                }
            }
        }

        if !primary_initialized {
            log::warn!("Failed to initialize ANY backend during reload!");
            *self.backend.write().await = None;
        }

        // Properly update fallback backends
        *self.fallback_backends.write().await = fallbacks;
    }


    /// Create and initialize an LLM backend by name using the provided merged config.
    ///
    /// The caller is responsible for merging the api_key from KeyStore into
    /// `merged_cfg` before calling this function.
    fn create_and_init_backend_static(
        plugin_manager: &crate::llm::plugin_manager::PluginManager,
        name: &str,
        merged_cfg: Value,
    ) -> Option<Box<dyn LlmBackend>> {
        let mut be = plugin_manager.create_backend(name)?;
        if be.initialize(&merged_cfg) {
            Some(be)
        } else {
            log::warn!("Backend '{}' created but failed to initialize", name);
            None
        }
    }

    /// Determine LLM backend candidates and their priorities.
    fn get_backend_candidates(
        &self,
        config: &LlmConfig,
        plugin_manager: &crate::llm::plugin_manager::PluginManager,
    ) -> Vec<BackendCandidate> {
        let mut candidates = Vec::new();
        let mut all_names: Vec<String> = Vec::new();

        // 1. Gather backend names from llm_config.json
        if let Some(obj) = config.backends.as_object() {
            for key in obj.keys() {
                all_names.push(key.clone());
            }
        }
        if !all_names.contains(&config.active_backend) {
            all_names.push(config.active_backend.clone());
        }
        for fb in &config.fallback_backends {
            if !all_names.contains(fb) {
                all_names.push(fb.clone());
            }
        }

        // 2. Append plugin backends
        for plugin_name in plugin_manager.available_plugins() {
            if !all_names.contains(&plugin_name) {
                all_names.push(plugin_name);
            }
        }

        for name in all_names {
            let mut priority = 0;
            let mut is_explicitly_in_config = false;

            // Priority 1 by default if it originates from llm_config.json
            if name == config.active_backend || config.fallback_backends.contains(&name) || config.backends.get(&name).is_some() {
                priority = 1;
                is_explicitly_in_config = true;
            }

            // Manual priority override from llm_config.json
            if let Some(p) = config.backends.get(&name).and_then(|v| v.get("priority")).and_then(|v| v.as_i64()) {
                priority = p;
                is_explicitly_in_config = true;
            }

            // Fallback to internal plugin config priority if NOT in llm_config.json
            if !is_explicitly_in_config {
                if let Some(cfg) = plugin_manager.get_plugin_config(&name) {
                    priority = cfg.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
                }
            }

            candidates.push(BackendCandidate { name, priority });
        }

        // Sort descending by priority, then by configuration precedence
        candidates.sort_by(|a, b| {
            let p_res = b.priority.cmp(&a.priority);
            if p_res != std::cmp::Ordering::Equal {
                return p_res;
            }

            // Tie-breaker: active_backend > fallback_backends (in array order) > others
            let score = |name: &str| -> i32 {
                if name == config.active_backend {
                    1000
                } else if let Some(idx) = config.fallback_backends.iter().position(|r| r == name) {
                    900 - (idx as i32)
                } else {
                    0
                }
            };
            score(&b.name).cmp(&score(&a.name))
        });

        candidates
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

    /// Process a user prompt through the 15-phase autonomous agent loop.
    ///
    /// ## Loop Phases
    /// 1. GoalParsing: Initialize AgentLoopState for this session + prompt
    /// 2. ContextLoading: Load session history, build messages + tools
    /// 3. Pre-loop Compaction: Compact if ≥90% of 256k token budget
    /// 4-13. Main loop: DecisionMaking → SafetyCheck → ToolDispatching
    ///        → ObservationCollect → Evaluating → ErrorRecovery
    ///        → StateTracking → SelfInspection → RePlanning → TerminationCheck
    /// 14. ResultReporting: Format and return final answer
    ///
    /// Thread-safe: acquires fine-grained locks on individual fields.
    pub async fn process_prompt(
        &self,
        session_id: &str,
        prompt: &str,
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> String {
        // ── Phase 1: GoalParsing ─────────────────────────────────────────
        let mut loop_state = AgentLoopState::new(session_id, prompt);

        // Load context token budget from config if available
        let (budget, threshold) = {
            let cfg = self.llm_config.lock().ok();
            let b = cfg.as_ref()
                .and_then(|c| c.backends.get("context_token_budget"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(CONTEXT_TOKEN_BUDGET);
            let t = cfg.as_ref()
                .and_then(|c| c.backends.get("context_compact_threshold"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(CONTEXT_COMPACT_THRESHOLD);
            (b, t)
        };
        loop_state.token_budget = budget;
        loop_state.compact_threshold = threshold;

        log::info!(
            "[AgentLoop] Phase=GoalParsing session='{}' goal='{}' budget={}",
            session_id, &prompt[..prompt.len().min(80)], budget
        );

        // Quick check: do we have any backend?
        {
            let has_primary = self.backend.read().await.is_some();
            let has_fallback = !self.fallback_backends.read().await.is_empty();
            if !has_primary && !has_fallback {
                return "Error: No LLM backend configured".into();
            }
        }

        // ── Phase 2: ContextLoading ──────────────────────────────────────
        loop_state.transition(AgentPhase::ContextLoading);

        // Store user message
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                store.add_message(session_id, "user", prompt);
            }
        }

        // Build conversation history
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

        if messages.is_empty() || messages.last().map(|m| m.role.as_str()) != Some("user") {
            messages.push(LlmMessage::user(prompt));
        }

        // Get tool declarations
        let mut tools = self.tool_dispatcher.read().await.get_tool_declarations();
        crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_builtin_tools(&mut tools);
        if let Ok(bridge) = self.action_bridge.lock() {
            tools.extend(bridge.get_action_declarations());
        }

        // Build System Prompt
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
            builder = builder.set_runtime_context(platform_name, model_name, data_dir);
            builder.build()
        };

        // ── Phase 3: Planning (pre-loop compaction) ──────────────────────
        loop_state.transition(AgentPhase::Planning);
        let context_engine = SizedContextEngine::new().with_threshold(loop_state.compact_threshold);

        // Update token_used estimate
        loop_state.token_used = context_engine.estimate_tokens(&messages);
        if loop_state.needs_compaction() {
            log::info!("[AgentLoop] Pre-loop compaction triggered ({}% used)",
                (loop_state.token_used as f32 / loop_state.token_budget as f32 * 100.0) as u32);
            messages = context_engine.compact(messages, loop_state.token_budget);
            loop_state.token_used = context_engine.estimate_tokens(&messages);
        }

        // ── Phases 4–13: Main agentic loop ───────────────────────────────
        loop {
            // ── Phase 4: DecisionMaking / LLM call ──────────────────────
            loop_state.transition(AgentPhase::DecisionMaking);
            log::info!(
                "[AgentLoop] Round {} | session='{}' phase=DecisionMaking msgs={}",
                loop_state.round, session_id, messages.len()
            );

            log::debug!("[System Prompt]:\n{}", system_prompt);
            for (i, msg) in messages.iter().enumerate() {
                log::info!("[Message {}] Role: {}\nText: {}", i, msg.role, msg.text);
            }

            let response = self.chat_with_fallback(&messages, &tools, on_chunk, &system_prompt).await;

            // ── Phase 6: ObservationCollect ──────────────────────────────
            loop_state.transition(AgentPhase::ObservationCollect);
            log::info!("[AgentLoop] Round {} Response: success={} text_len={}",
                loop_state.round, response.success, response.text.len());

            // ── Phase 11: SafetyCheck — handle LLM error ─────────────────
            if !response.success {
                loop_state.transition(AgentPhase::ErrorRecovery);
                loop_state.error_count += 1;
                let err = format!(
                    "LLM error (HTTP {}): {}",
                    response.http_status, response.error_message
                );
                log::error!("[AgentLoop] {}", err);

                if loop_state.error_count >= MAX_TOOL_RETRY {
                    loop_state.transition(AgentPhase::ResultReporting);
                    return err;
                }
                // Retry: continue loop
                loop_state.round += 1;
                continue;
            }

            // Extract reasoning
            let mut reasoning_text = response.reasoning_text.clone();
            if reasoning_text.is_empty() {
                let think_re = regex::Regex::new(r"(?s)<think>(.*?)</think>").unwrap();
                if let Some(cap) = think_re.captures(&response.text) {
                    reasoning_text = cap[1].trim().to_string();
                }
            }

            // Fallback parser
            let mut detected_tool_calls = response.tool_calls.clone();
            if detected_tool_calls.is_empty() {
                detected_tool_calls = FallbackParser::parse(&response.text);
                if !detected_tool_calls.is_empty() {
                    log::info!("[AgentLoop] FallbackParser detected {} tool call(s)",
                        detected_tool_calls.len());
                }
            }

            // Record token usage
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
                        let usage = store.load_token_usage(session_id);
                        log::info!("[TokenUsage] Round: P{}+C{}={} | Session cumulative: {}",
                            response.prompt_tokens, response.completion_tokens,
                            response.prompt_tokens + response.completion_tokens,
                            usage.total_prompt_tokens + usage.total_completion_tokens);
                        loop_state.token_used = usage.total_prompt_tokens as usize
                            + context_engine.estimate_tokens(&messages);
                    }
                }
            }

            if !detected_tool_calls.is_empty() {
                // ── Phase 5: ToolDispatching ─────────────────────────────
                loop_state.transition(AgentPhase::ToolDispatching);
                loop_state.total_tool_calls += detected_tool_calls.len();
                log::info!("[AgentLoop] Round {} dispatching {} tool(s)",
                    loop_state.round, detected_tool_calls.len());

                // Add assistant message
                messages.push(LlmMessage {
                    role: "assistant".into(),
                    text: response.text.clone(),
                    reasoning_text: reasoning_text.clone(),
                    tool_calls: detected_tool_calls.clone(),
                    ..Default::default()
                });

                // Parallel tool execution
                let td_guard = self.tool_dispatcher.read().await;
                let mut futures_list = Vec::new();

                for tc in detected_tool_calls.iter() {
                    let skills_dir = self.platform.paths.skills_dir.clone();
                    let td_guard_ref = &*td_guard;
                    let tc_name = tc.name.clone();
                    let tc_args = tc.args.clone();
                    let tc_id = tc.id.clone();
                    let bridge_ref = &self.action_bridge;

                    // ── Phase 11: SafetyCheck per tool ───────────────────
                    let block_reason = if let Ok(tp) = self.tool_policy.lock() {
                        match tp.check_policy(session_id, &tc_name, &tc_args) {
                            Err(reason) => Some(reason),
                            Ok(_) => None,
                        }
                    } else { None };

                    futures_list.push(async move {
                        if let Some(reason) = block_reason {
                            log::warn!("[SafetyCheck] Tool '{}' blocked: {}", tc_name, reason);
                            return LlmMessage::tool_result(&tc_id, &tc_name,
                                serde_json::json!({"error": reason}));
                        }

                        let result = if tc_name.starts_with("action_") {
                            if let Some(action_id) = tc_name.strip_prefix("action_") {
                                if let Ok(bridge) = bridge_ref.lock() {
                                    bridge.execute_action(action_id, &tc_args)
                                } else {
                                    json!({"error": "Failed to lock action bridge"})
                                }
                            } else {
                                json!({"error": "Invalid action format"})
                            }
                        } else if tc_name == "create_skill" {
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
                                        Ok(_) => serde_json::json!({"status": "success", "message": format!("Skill '{}' created at {:?}", sanitized_name, skill_md_path)}),
                                        Err(e) => serde_json::json!({"error": format!("Failed to write skill: {}", e)})
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

                        log::info!("[ObservationCollect] Tool '{}' result: {} chars",
                            tc_name, result.to_string().len());
                        LlmMessage::tool_result(&tc_id, &tc_name, result)
                    });
                }

                let results = futures_util::future::join_all(futures_list).await;
                messages.extend(results);

                // ── Phase 7: Evaluating (partial progress) ───────────────
                loop_state.transition(AgentPhase::Evaluating);
                let verdict = loop_state.observe_output(&response.text);
                log::info!("[Evaluating] Round {} verdict={}", loop_state.round, verdict.as_str());

                if verdict == EvalVerdict::Stuck {
                    log::warn!("[AgentLoop] Idle loop detected (same output {} rounds). Terminating.",
                        AgentLoopState::IDLE_WINDOW);
                    loop_state.transition(AgentPhase::TerminationCheck);
                    loop_state.transition(AgentPhase::ResultReporting);

                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            store.add_message(session_id, "assistant",
                                "Task completed (idle loop detected).");
                        }
                    }
                    return response.text;
                }

            } else {
                // ── Phase 7: Evaluating — GoalAchieved ──────────────────
                loop_state.transition(AgentPhase::Evaluating);
                loop_state.last_eval_verdict = EvalVerdict::GoalAchieved;

                log::info!("[Evaluating] Round {} verdict=GoalAchieved (no tool calls)",
                    loop_state.round);

                let text = response.text;
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.add_message(session_id, "assistant", &text);
                    }
                }

                // ── Phase 14: ResultReporting ────────────────────────────
                loop_state.transition(AgentPhase::ResultReporting);
                loop_state.transition(AgentPhase::Complete);
                loop_state.log_self_inspection();
                return text;
            }

            // ── Phase 8: RePlanning / Phase 12: StateTracking ────────────
            loop_state.transition(AgentPhase::StateTracking);

            // ── Phase 13: SelfInspection ─────────────────────────────────
            loop_state.transition(AgentPhase::SelfInspection);
            loop_state.log_self_inspection();

            // In-loop size-based compaction
            loop_state.token_used = context_engine.estimate_tokens(&messages);
            if loop_state.needs_compaction() {
                log::info!("[ContextEngine] In-loop compaction triggered (round {})", loop_state.round);
                messages = context_engine.compact(messages, loop_state.token_budget);
                loop_state.token_used = context_engine.estimate_tokens(&messages);
            }

            // ── Phase 9: TerminationCheck ─────────────────────────────────
            loop_state.round += 1;
            loop_state.transition(AgentPhase::TerminationCheck);

            if loop_state.is_round_limit_reached() {
                log::warn!("[AgentLoop] Max rounds ({}) reached for session '{}'",
                    loop_state.max_tool_rounds, session_id);
                break;
            }

            loop_state.transition(AgentPhase::RePlanning);
        }

        // ── Phase 14: ResultReporting (limit hit) ────────────────────────
        loop_state.transition(AgentPhase::ResultReporting);
        loop_state.log_self_inspection();
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

        let base_dir = std::path::Path::new("/opt/usr/share/tizen-tools");
        let tools_to_check = ["cli", "actions", "skills", "system_cli"];
        let mut needs_update = false;

        // Check tools.md
        let tools_md = base_dir.join("tools.md");
        if !tools_md.exists() {
            needs_update = true;
        }

        // Check subdirectory indices
        for subdir in &tools_to_check {
            let dir_path = base_dir.join(subdir);
            if dir_path.exists() && dir_path.is_dir() {
                let index_path = dir_path.join("index.md");
                if !index_path.exists() {
                    needs_update = true;
                    break;
                }
                
                // Compare contents
                let index_content = std::fs::read_to_string(&index_path).unwrap_or_default();
                if let Ok(entries) = std::fs::read_dir(&dir_path) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name == "index.md" || name == "tools.md" || name == "SKILL.md" {
                            continue;
                        }
                        if !index_content.contains(&name) {
                            needs_update = true;
                            break;
                        }
                    }
                }
            }
            if needs_update { break; }
        }

        if !needs_update && tools_md.exists() {
            log::info!("[Startup Indexing] Index files are fully synchronized. Skipping LLM request to avoid token waste.");
            return;
        }

        log::info!("[Startup Indexing] Mismatch detected. Requesting dynamic indexing of /opt/usr/share/tizen-tools/...");

        // Ensure we don't accumulate tokens from previous iterations
        let session_id = "system_startup_indexer";
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                store.clear_session(session_id);
            }
        }
        
        let prompt = "Please check the directories under `/opt/usr/share/tizen-tools/` (specifically `skills/`, `cli/`, `actions/`, `system_cli/`) and update `tools.md` and their respective `index.md` files to reflect the current state. Read the directories first, then overwrite the markdown files cleanly. Do not ask for permissions, simply execute via your file manager or execution tools.";
        
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
