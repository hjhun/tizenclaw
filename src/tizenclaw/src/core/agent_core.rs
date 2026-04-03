//! Agent Core — the brain of TizenClaw.
//!
//! Manages LLM interaction, tool calling, session management,
//! and the agentic loop (prompt → LLM → tool call → result → LLM → ...).
//!
//! ## Prompt Caching
//! After building the system_prompt, `process_prompt()` computes a simple
//! hash and compares it to `prompt_hash`. On change, it calls
//! `backend.prepare_cache()` (no-op for non-Gemini backends). GeminiBackend
//! creates/refreshes a `CachedContent` resource; subsequent `chat()` calls
//! reference that resource instead of re-sending the full text.
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
    memory_store: Mutex<Option<crate::storage::memory_store::MemoryStore>>,
    workflow_engine: tokio::sync::RwLock<crate::core::workflow_engine::WorkflowEngine>,
    /// Hash of the last system_prompt sent to the backend.
    /// Used to detect when the prompt changes so that the server-side
    /// cached content can be refreshed (e.g. Gemini CachedContent API).
    prompt_hash: tokio::sync::RwLock<u64>,
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
            memory_store: Mutex::new(None),
            workflow_engine: tokio::sync::RwLock::new(crate::core::workflow_engine::WorkflowEngine::new()),
            prompt_hash: tokio::sync::RwLock::new(0),
        }
    }

    /// Compute a fast 64-bit hash of an arbitrary string slice.
    /// Used to detect system_prompt changes without storing the full text.
    fn hash_str(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    }

    pub async fn initialize(&self) -> bool {
        log::debug!("AgentCore initializing...");
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

        // Initialize memory store
        let mem_dir = std::path::PathBuf::from("/opt/usr/share/tizenclaw/memory");
        let mem_db = mem_dir.join("memories.db");
        let model_dir = paths.data_dir.join("models");
        match crate::storage::memory_store::MemoryStore::new(
            &mem_dir.to_string_lossy(), 
            &mem_db.to_string_lossy(),
            &model_dir.to_string_lossy()
        ) {
            Ok(store) => {
                log::info!("Memory store initialized");
                if let Ok(mut ms) = self.memory_store.lock() {
                    *ms = Some(store);
                }
            }
            Err(e) => log::error!("Memory store failed: {}", e),
        }

        // Load tools from all subdirectories under the tools directory
        {
            let mut td = self.tool_dispatcher.write().await;
            td.load_tools_from_root(&paths.tools_dir.to_string_lossy());
        }
        log::info!("Tools loaded from {:?}", paths.tools_dir);

        // Load workflows
        {
            let mut we = self.workflow_engine.write().await;
            we.load_workflows(); // uses default path "/opt/usr/share/tizenclaw/workflows"
        }

        {
            let mut bridge = self.action_bridge.lock().unwrap();
            bridge.start();
        }

        true
    }

    /// Dynamically handle package manager events for plugins
    pub async fn handle_pkgmgr_event(&self, event_name: &str, pkgid: &str) {
        log::debug!("Handling pkgmgr event: {} for pkgid: {}", event_name, pkgid);
        
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
            log::debug!("Triggering LLM backend reload due to plugin changes...");
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
                    log::debug!("Dynamically swapped Primary LLM backend to '{}' (priority {})", cand.name, cand.priority);
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
            log::debug!("Backend '{}' skipped: not configured or initialization failed", name);
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
        max_tokens: Option<u32>,
    ) -> LlmResponse {
        // Try primary backend — lock is held only during chat()
        {
            let bn = self.backend_name.read().map(|n| n.clone()).unwrap_or_default();
            if self.is_backend_available(&bn) {
                let be_guard = self.backend.read().await;
                if let Some(be) = be_guard.as_ref() {
                    let resp = be.chat(messages, tools, on_chunk, system_prompt, max_tokens).await;
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
                    log::debug!("Trying fallback backend '{}'", bn);
                    let resp = fb.chat(messages, tools, on_chunk, system_prompt, max_tokens).await;
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

    /// Extract intent keywords for dynamic tool filtering.
    fn extract_intent_keywords(prompt: &str) -> Vec<String> {
        let p = prompt.to_lowercase();
        let mut keywords = Vec::new();

        if p.contains("파일") || p.contains("읽어") || p.contains("열어") || p.contains("file") || p.contains("read") {
            keywords.extend(["fs", "file", "read", "write"].map(String::from));
        }
        if p.contains("설치") || p.contains("앱") || p.contains("패키지") || p.contains("install") || p.contains("package") || p.contains("app") {
            keywords.extend(["pkg", "app", "install"].map(String::from));
        }
        if p.contains("기억") || p.contains("저장") || p.contains("remember") || p.contains("memory") || p.contains("search") || p.contains("knowledge") {
            keywords.extend(["mem", "remember", "forget", "recall", "search"].map(String::from));
        }
        if p.contains("실행") || p.contains("명령") || p.contains("run") || p.contains("exec") || p.contains("shell") {
            keywords.extend(["exec", "shell", "run"].map(String::from));
        }
        if p.contains("일정") || p.contains("알람") || p.contains("task") || p.contains("schedule") {
            keywords.extend(["task", "sched", "alarm"].map(String::from));
        }
        
        for word in p.split_whitespace() {
            if word.len() >= 4 {
                keywords.push(word.into());
            }
        }
        keywords
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

        log::debug!(
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
        
        log::debug!("[AgentCore] USER: {}", prompt);

        // Store user message
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                store.add_message(session_id, "user", prompt);
            }
        }

        // Build conversation history — compaction-aware load
        let history = {
            let ss = self.session_store.lock();
            if let Some(Ok(guard)) = ss.ok().map(|s| {
                // Returns (Vec<SessionMessage>, bool)
                Ok::<_, ()>(s.as_ref().map(|store| {
                    store.load_session_context(session_id, MAX_CONTEXT_MESSAGES)
                }))
            }) {
                if let Some((msgs, from_compact)) = guard {
                    if from_compact {
                        log::info!("[ContextLoading] session='{}' loaded from compacted.md",
                            session_id);
                    } else {
                        log::info!("[ContextLoading] session='{}' loaded {} msgs from history",
                            session_id, msgs.len());
                    }
                    msgs
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
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

        // Extract intent keywords for optimal tool injection
        let intent_keywords = Self::extract_intent_keywords(prompt);

        // Get tool declarations
        let mut tools = self.tool_dispatcher.read().await.get_tool_declarations_filtered(&intent_keywords);
        crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_builtin_tools(&mut tools, prompt);
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

        // Load long term memory dynamically and inject into messages (preserves system_prompt cache)
        if let Ok(ms) = self.memory_store.lock() {
            if let Some(store) = ms.as_ref() {
                let mem_str = store.load_relevant_for_prompt(prompt, 5, 0.1);
                if !mem_str.is_empty() {
                    let memory_context = format!("## Context from Long-Term Memory\n<long_term_memory>\n{}\n</long_term_memory>", mem_str);
                    if !messages.is_empty() {
                        let last_idx = messages.len() - 1;
                        messages.insert(last_idx, LlmMessage::user(&memory_context));
                    }
                }
            }
        }


        // ── Phase 2.5: Prompt Cache Preparation ─────────────────────────
        // Compute hash of system_prompt; refresh server-side cache only when
        // the prompt actually changed. For GeminiBackend this creates/refreshes
        // a CachedContent resource so subsequent rounds skip re-sending the
        // full system_instruction text (~60-80% prompt token savings).
        {
            let new_hash = Self::hash_str(&system_prompt);
            let cached_hash = *self.prompt_hash.read().await;
            if new_hash != cached_hash {
                log::debug!(
                    "[PromptCache] System prompt changed (hash {} → {}), refreshing cache…",
                    cached_hash, new_hash
                );
                let be_guard = self.backend.read().await;
                if let Some(be) = be_guard.as_ref() {
                    let cached = be.prepare_cache(&system_prompt).await;
                    if cached {
                        log::info!("[PromptCache] Cache ready — subsequent rounds will reference cached content");
                    } else {
                        log::debug!("[PromptCache] Backend does not support caching or prompt too short; using inline system_instruction");
                    }
                }
                drop(be_guard);
                // Always update the stored hash so we do not retry on every call
                *self.prompt_hash.write().await = new_hash;
            } else {
                log::debug!("[PromptCache] Prompt unchanged (hash={}), reusing cached content", cached_hash);
            }
        }

        // ── Phase 3: Planning (Cognitive Plan-and-Solve & compaction) ────
        loop_state.transition(AgentPhase::Planning);
        let context_engine = SizedContextEngine::new().with_threshold(loop_state.compact_threshold);

        let mut matched_workflow_id = None;
        {
            let we = self.workflow_engine.read().await;
            for wf_val in we.list_workflows() {
                if let (Some(w_id), Some(trigger)) = (
                    wf_val.get("id").and_then(|v| v.as_str()), 
                    wf_val.get("trigger").and_then(|v| v.as_str())
                ) {
                    if trigger != "manual" && (prompt.contains(trigger) || trigger == prompt) {
                        matched_workflow_id = Some(w_id.to_string());
                        break;
                    }
                }
            }
        }

        if let Some(wf_id) = matched_workflow_id {
            log::info!("[Planning] Matched workflow trigger '{}', entering Workflow Mode.", wf_id);
            loop_state.active_workflow_id = Some(wf_id);
        } else {
            // Optional LLM Cognitive Step for Complex Prompts
            if crate::core::intent_analyzer::IntentAnalyzer::is_complex_task(prompt) {
                log::debug!("[AgentLoop] Complex prompt detected. Triggering explicit Plan-and-Solve...");
                let plan_sys = "You are a precise planner. Outline the distinct steps to solve the user's request. Output only a list of concise steps.";
                
                // Release writer locks safely for LLM call
                let plan_resp_opt = {
                    let be_guard = self.backend.read().await;
                    if let Some(be) = be_guard.as_ref() {
                        Some(be.chat(&[LlmMessage::user(prompt)], &[], None, plan_sys, Some(1024)).await)
                    } else {
                        None
                    }
                };

                if let Some(p_resp) = plan_resp_opt {
                    if p_resp.success {
                        let steps = p_resp.text.trim().to_string();
                        loop_state.plan_steps.push(steps.clone());
                        messages.push(LlmMessage {
                            role: "system".into(),
                            text: format!("## Active Plan (Follow these steps):\n{}", steps),
                            ..Default::default()
                        });
                        log::info!("[Planning] Extracted plan steps into context.");
                    }
                }
            }
        }

        // Update token_used estimate
        loop_state.token_used = context_engine.estimate_tokens(&messages);
        if loop_state.needs_compaction() {
            log::debug!("[AgentLoop] Pre-loop compaction triggered ({}% used)",
                (loop_state.token_used as f32 / loop_state.token_budget as f32 * 100.0) as u32);
            messages = context_engine.compact(messages, loop_state.token_budget);
            loop_state.token_used = context_engine.estimate_tokens(&messages);
        }

        // ── Phases 4–13: Main agentic loop ───────────────────────────────
        loop {
            // ── Phase 4: DecisionMaking / LLM call ──────────────────────
            loop_state.transition(AgentPhase::DecisionMaking);
            log::debug!(
                "[AgentLoop] Round {} | session='{}' phase=DecisionMaking msgs={}",
                loop_state.round, session_id, messages.len()
            );

            log::debug!("[System Prompt]:\n{}", system_prompt);
            for (i, msg) in messages.iter().enumerate() {
                log::debug!("[Message {}] Role: {}\nText: {}", i, msg.role, msg.text);
            }

            // Step 6: Set Max Tokens Dynamically
            let dynamic_max_tokens = if prompt.len() < 50 { 1024 } else { 4096 };

            let mut response = LlmResponse::default();
            let mut is_workflow_tool = false;

            if let Some(wf_id) = loop_state.active_workflow_id.clone() {
                let we = self.workflow_engine.read().await;
                if let Some(wf) = we.get_workflow(&wf_id) {
                    if loop_state.current_workflow_step >= wf.steps.len() {
                        log::info!("[Workflow] All steps completed for {}", wf.name);
                        loop_state.active_workflow_id = None;
                        loop_state.transition(AgentPhase::ResultReporting);
                        let text = format!("Workflow '{}' completed successfully.\nVariables:\n{:?}", wf.name, loop_state.workflow_vars.keys().collect::<Vec<_>>());
                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                store.add_message(session_id, "assistant", &text);
                            }
                        }
                        return text;
                    }

                    let step = &wf.steps[loop_state.current_workflow_step];
                    
                    use crate::core::workflow_engine::WorkflowStepType;
                    match step.step_type {
                        WorkflowStepType::Condition => {
                            if crate::core::workflow_engine::WorkflowEngine::eval_condition(&step.condition, &loop_state.workflow_vars) {
                                log::debug!("Condition evaluated to TRUE. Branching to '{}'", step.then_step);
                                loop_state.current_workflow_step += 1;
                            } else {
                                log::debug!("Condition evaluated to FALSE. Branching to '{}'", step.else_step);
                                loop_state.current_workflow_step += 1;
                            }
                            continue;
                        }
                        WorkflowStepType::Tool => {
                            let resolved_args = crate::core::workflow_engine::WorkflowEngine::interpolate_json(&step.args, &loop_state.workflow_vars);
                            response.success = true;
                            // Add randomness so observe_output Doesn't see identical strings and trigger Stuck
                            response.text = format!("Executing workflow tool '{}' (Round {})", step.tool_name, loop_state.round);
                            response.tool_calls.push(crate::llm::backend::LlmToolCall {
                                id: format!("call_{}_{}", step.id, loop_state.round),
                                name: step.tool_name.clone(),
                                args: resolved_args,
                            });
                            is_workflow_tool = true;
                        }
                        WorkflowStepType::Prompt => {
                            // Only inject the prompt if we haven't already for this step
                            let step_marker = format!("## [Workflow: {}]", step.id);
                            let already_injected = messages.iter().any(|m| m.text.contains(&step_marker));
                            
                            if !already_injected {
                                let resolved_instruction = crate::core::workflow_engine::WorkflowEngine::interpolate(&step.instruction, &loop_state.workflow_vars);
                                messages.push(LlmMessage {
                                    role: "system".into(),
                                    text: format!("{}\n{}", step_marker, resolved_instruction),
                                    ..Default::default()
                                });
                            }
                            response = self.chat_with_fallback(&messages, &tools, on_chunk, &system_prompt, Some(dynamic_max_tokens)).await;
                        }
                    }
                }
            } else {
                response = self.chat_with_fallback(&messages, &tools, on_chunk, &system_prompt, Some(dynamic_max_tokens)).await;
            }

            // ── Phase 6: ObservationCollect ──────────────────────────────
            loop_state.transition(AgentPhase::ObservationCollect);
            log::debug!("[AgentLoop] Round {} Response: success={} text_len={}",
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
                    log::debug!("[AgentLoop] FallbackParser detected {} tool call(s)",
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
                        log::debug!("[TokenUsage] Round: P{}+C{}={} | Session cumulative: {}",
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
                log::debug!("[AgentLoop] Round {} dispatching {} tool(s)",
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
                let mem_store_opt = self.memory_store.lock().ok().and_then(|ms| ms.as_ref().cloned());

                for tc in detected_tool_calls.iter() {
                    let skills_dir = self.platform.paths.skills_dir.clone();
                    let td_guard_ref = &*td_guard;
                    let tc_name = tc.name.clone();
                    let tc_args = tc.args.clone();
                    let tc_id = tc.id.clone();
                    let bridge_ref = &self.action_bridge;
                    let ms_clone = mem_store_opt.clone();

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
                                    if skill_md_path.is_dir() {
                                        let _ = std::fs::remove_dir_all(&skill_md_path);
                                    }
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
                        } else if tc_name == "remember" {
                            if let Some(store) = ms_clone {
                                let key = tc_args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                let value = tc_args.get("value").and_then(|v| v.as_str()).unwrap_or("");
                                let category = tc_args.get("category").and_then(|v| v.as_str()).unwrap_or("general");
                                if !key.is_empty() && !value.is_empty() {
                                    store.set(key, value, category);
                                    serde_json::json!({"status": "success", "message": format!("Remembered '{}'", key)})
                                } else {
                                    serde_json::json!({"error": "Missing key or value"})
                                }
                            } else {
                                serde_json::json!({"error": "MemoryStore not initialized"})
                            }
                        } else if tc_name == "recall" {
                            if let Some(store) = ms_clone {
                                let key = tc_args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                if let Some(val) = store.get(key) {
                                    serde_json::json!({"status": "success", "value": val})
                                } else {
                                    serde_json::json!({"error": "Key not found"})
                                }
                            } else {
                                serde_json::json!({"error": "MemoryStore not initialized"})
                            }
                        } else if tc_name == "forget" {
                            if let Some(store) = ms_clone {
                                let key = tc_args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                if store.delete(key) {
                                    serde_json::json!({"status": "success", "message": format!("Forgot '{}'", key)})
                                } else {
                                    serde_json::json!({"error": "Key not found"})
                                }
                            } else {
                                serde_json::json!({"error": "MemoryStore not initialized"})
                            }
                        } else {
                            td_guard_ref.execute(&tc_name, &tc_args).await
                        };

                        log::debug!("[ObservationCollect] Tool '{}' result: {} chars",
                            tc_name, result.to_string().len());
                        LlmMessage::tool_result(&tc_id, &tc_name, result)
                    });
                }

                let results = futures_util::future::join_all(futures_list).await;
                messages.extend(results);

                // ── Phase 7: Evaluating (partial progress) ───────────────
                loop_state.transition(AgentPhase::Evaluating);
                let verdict = loop_state.observe_output(&response.text);
                log::debug!("[Evaluating] Round {} verdict={}", loop_state.round, verdict.as_str());

                if verdict == EvalVerdict::Stuck {
                    loop_state.stuck_retry_count += 1;
                    if loop_state.stuck_retry_count > 2 {
                        log::warn!("[AgentLoop] Idle loop detected (round {}) - Terminating.", loop_state.round);
                        loop_state.transition(AgentPhase::TerminationCheck);
                        loop_state.transition(AgentPhase::ResultReporting);

                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                store.add_message(session_id, "assistant",
                                    "Task aborted (terminal idle loop).");
                            }
                        }
                        return "Error: Agent is stuck in an execution loop.".into();
                    } else {
                        log::warn!("[AgentLoop] Idle loop detected (round {}) - Triggering Dynamic Fallback RePlanning.", loop_state.round);
                        messages.push(LlmMessage {
                            role: "user".into(),
                            text: "System Error: You are stuck in a loop. Re-evaluate your plan and try a completely different approach using different tools. Do not repeat the previous action.".into(),
                            ..Default::default()
                        });
                        loop_state.transition(AgentPhase::RePlanning);
                    }
                }

                // If it was a workflow tool, we just successfully completed it! Save output and advance.
                if is_workflow_tool {
                    let last_msg = messages.last().unwrap();
                    let output_val = serde_json::from_str(&last_msg.text).unwrap_or(Value::String(last_msg.text.clone()));
                    
                    let we = self.workflow_engine.read().await;
                    if let Some(wf_id) = loop_state.active_workflow_id.clone() {
                        if let Some(wf) = we.get_workflow(&wf_id) {
                            let step = &wf.steps[loop_state.current_workflow_step];
                            loop_state.workflow_vars.insert(step.output_var.clone(), output_val);
                            loop_state.current_workflow_step += 1;
                        }
                    }
                    continue; // Immediately start next round to pick up next workflow step
                }

            } else {
                let mut advance_workflow = false;
                if loop_state.active_workflow_id.is_some() {
                    let we = self.workflow_engine.read().await;
                    if let Some(wf) = we.get_workflow(loop_state.active_workflow_id.as_ref().unwrap()) {
                        let step = &wf.steps[loop_state.current_workflow_step];
                        loop_state.workflow_vars.insert(step.output_var.clone(), serde_json::Value::String(response.text.clone()));
                        loop_state.current_workflow_step += 1;
                        advance_workflow = true;
                    }
                }
                if advance_workflow {
                    // Push the prompt assistant response so context isn't lost
                    messages.push(LlmMessage {
                        role: "assistant".into(),
                        text: response.text.clone(),
                        ..Default::default()
                    });
                    continue;
                }

                // ── Phase 7: Evaluating — GoalAchieved ──────────────────
                loop_state.transition(AgentPhase::Evaluating);
                loop_state.last_eval_verdict = EvalVerdict::GoalAchieved;

                log::debug!("[Evaluating] Round {} verdict=GoalAchieved (no tool calls)",
                    loop_state.round);

                let text = response.text;
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.add_message(session_id, "assistant", &text);
                    }
                }

                // ── Phase 14: ResultReporting ────────────────────────────
                loop_state.transition(AgentPhase::ResultReporting);
                
                // Trigger auto-extraction (small overhead at end of conversation)
                self.extract_and_save_memory(&messages, &text).await;

                loop_state.transition(AgentPhase::Complete);
                loop_state.log_self_inspection();
                
                log::debug!("[AgentCore] AGENT: {}", text);
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
                log::debug!("[ContextEngine] In-loop compaction triggered (round {})",
                    loop_state.round);
                messages = context_engine.compact(messages, loop_state.token_budget);
                loop_state.token_used = context_engine.estimate_tokens(&messages);

                // Persist compacted snapshot to disk (compacted.md)
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        use crate::storage::session_store::SessionMessage;
                        let session_msgs: Vec<SessionMessage> = messages
                            .iter()
                            .map(|m| SessionMessage {
                                role: m.role.clone(),
                                text: m.text.clone(),
                                timestamp: String::new(),
                            })
                            .collect();
                        match store.save_compacted(session_id, &session_msgs) {
                            Ok(_) => log::debug!(
                                "[ContextEngine] compacted.md saved ({} msgs)",
                                session_msgs.len()
                            ),
                            Err(e) => log::warn!(
                                "[ContextEngine] Failed to save compacted.md: {}", e
                            ),
                        }
                    }
                }
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
        use crate::core::tool_indexer;

        let root_dir = "/opt/usr/share/tizen-tools";

        // Phase 1: Hash-based change detection (fast, no I/O beyond stat)
        if !tool_indexer::needs_reindex(root_dir) {
            log::info!(
                "[Startup Indexing] No changes detected (hash match). Skipping."
            );
            return;
        }

        // Phase 2: Local filesystem scan — collect all tool metadata
        log::info!("[Startup Indexing] Scanning tool metadata from {}...", root_dir);
        let metadata = tool_indexer::scan_tools_metadata(root_dir);

        if metadata.total_tools() == 0 {
            log::info!("[Startup Indexing] No tools found. Skipping index generation.");
            return;
        }

        log::info!(
            "[Startup Indexing] Found {} tools across {} categories.",
            metadata.total_tools(),
            metadata.categories.len(),
        );

        // Phase 3: LLM-assisted markdown generation (single call)
        let has_primary = self.backend.read().await.is_some();
        let has_fallback = !self.fallback_backends.read().await.is_empty();

        if has_primary || has_fallback {
            log::info!("[Startup Indexing] Generating documentation via LLM...");
            let prompt = tool_indexer::build_indexing_prompt(&metadata);
            let system_prompt = "You are a precise documentation generator. \
                Output ONLY the requested JSON. No extra commentary.";

            let msgs = vec![LlmMessage::user(&prompt)];
            let response = self.chat_with_fallback(
                &msgs, &[], None, system_prompt, Some(8192),
            ).await;

            if response.success {
                let written = tool_indexer::apply_llm_index_result(
                    &response.text, root_dir,
                );
                if written > 0 {
                    tool_indexer::save_index_hash(root_dir);
                    log::info!(
                        "[Startup Indexing] LLM generated {} index files.",
                        written,
                    );
                } else {
                    log::warn!(
                        "[Startup Indexing] LLM response parsed but 0 files \
                         written. Falling back to template."
                    );
                    tool_indexer::generate_fallback_index(&metadata, root_dir);
                    tool_indexer::save_index_hash(root_dir);
                }
            } else {
                log::warn!(
                    "[Startup Indexing] LLM call failed. Using fallback template."
                );
                tool_indexer::generate_fallback_index(&metadata, root_dir);
                tool_indexer::save_index_hash(root_dir);
            }
        } else {
            // No LLM available — generate a basic template
            log::info!(
                "[Startup Indexing] No LLM available. Generating fallback index."
            );
            tool_indexer::generate_fallback_index(&metadata, root_dir);
            tool_indexer::save_index_hash(root_dir);
        }

        log::info!("[Startup Indexing] Completed.");
    }

    /// Extractor sub-task logic. Invokes the LLM to glean long-term knowledge.
    async fn extract_and_save_memory(&self, history: &[LlmMessage], final_response: &str) {
        let ms_clone = self.memory_store.lock().ok().and_then(|ms| ms.as_ref().cloned());
        if ms_clone.is_none() {
            return;
        }
        let store = ms_clone.unwrap();

        // only extract if we have some messages
        if history.is_empty() {
            return;
        }

        let system_prompt = "You are an automated daemon component for TizenClaw responsible for extracting \
useful Long-Term Memories. Analyze the recent conversation snippet and the assistant's final response. \
Identify permanent facts, user preferences, names, device states, or specific instructions the user wants kept. \
Output ONLY a raw JSON array. DO NOT append Markdown code blocks. \
Output format: [{\"category\": \"preferences\", \"key\": \"pref::timezone\", \"value\": \"KST\"}] \
If there is nothing new to remember, output exactly: []";

        let mut msgs = vec![];
        let mut convo_text = String::new();
        // Give the last few messages for context
        for m in history.iter().rev().take(3).rev() {
            convo_text.push_str(&format!("{}: {}\n", m.role, m.text));
        }
        convo_text.push_str(&format!("assistant: {}\n", final_response));
        msgs.push(LlmMessage::user(&convo_text));

        log::debug!("[MemoryExtractor] Triggering LLM extraction sub-task...");
        let response = self.chat_with_fallback(&msgs, &[], None, system_prompt, Some(8192)).await;

        if response.success {
            let text = response.text.trim();
            // clean potential markdown code block
            let clean_json = if text.starts_with("```json") {
                text.trim_start_matches("```json").trim_end_matches("```").trim()
            } else if text.starts_with("```") {
                text.trim_start_matches("```").trim_end_matches("```").trim()
            } else {
                text
            };

            if clean_json == "[]" || clean_json.is_empty() {
                log::debug!("[MemoryExtractor] No new memories extracted.");
                return;
            }

            if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(clean_json) {
                let mut count = 0;
                for item in parsed {
                    if let (Some(cat), Some(k), Some(v)) = (
                        item.get("category").and_then(|v| v.as_str()),
                        item.get("key").and_then(|v| v.as_str()),
                        item.get("value").and_then(|v| v.as_str())
                    ) {
                        store.set(k, v, cat);
                        count += 1;
                        log::debug!("[MemoryExtractor] Saved memory -> {}: {}", k, v);
                    }
                }
                if count > 0 {
                    log::debug!("[MemoryExtractor] Successfully saved {} extracted memories.", count);
                }
            } else {
                log::warn!("[MemoryExtractor] Failed to parse JSON response: {}", clean_json);
            }
        } else {
            log::warn!("[MemoryExtractor] Extractor LLM call failed.");
        }
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
