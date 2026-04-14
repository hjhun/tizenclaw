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
    safety_guard: Arc<Mutex<SafetyGuard>>,
    context_engine: Arc<SizedContextEngine>,
    event_bus: Arc<EventBus>,
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
    agent_roles: RwLock<AgentRoleRegistry>,
    session_profiles: Mutex<HashMap<String, SessionPromptProfile>>,
    /// Hash of the last system_prompt sent to the backend.
    /// Used to detect when the prompt changes so that the server-side
    /// cached content can be refreshed (e.g. Gemini CachedContent API).
    prompt_hash: tokio::sync::RwLock<u64>,
}
