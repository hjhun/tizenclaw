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

use futures_util::future::join_all;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, RwLock};

static THINK_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<think>(.*?)</think>").unwrap());
static EXPLICIT_PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(/(?:[^/\s"'`<>()\[\]{};,]+/)*[^/\s"'`<>()\[\]{};,]+/?)"#).unwrap()
});
static LEVEL_ANSWER_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[Level\s+(\d+)\]").unwrap());
static LEVEL_ANSWER_LINE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)^\[Level\s+(\d+)\]\s+Answer:\s*(.+?)\s*$").unwrap());
static LEVEL_OUTPUT_FILE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^level-\d+-solution\.py$").unwrap());
static LEVEL_OUTPUT_LEVEL_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^level-(\d+)-solution\.py$").unwrap());
static LEVEL_INPUT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"level[_-]?(\d+)[^/\s]*\.csv$").unwrap());
static SPECULATION_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?i)\b(assuming|assume|placeholder)\b").unwrap());
static QUOTED_IDENTIFIER_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"['"]([A-Za-z_][A-Za-z0-9_]*)['"]"#).unwrap());
static MARKDOWN_LEVEL_HEADING_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^##\s+\[(\d+)단계\]").unwrap());
static CSV_FILE_NAME_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([A-Za-z0-9_-]+\.csv)\b").unwrap());
static RELATIVE_FILE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b((?:[A-Za-z0-9._-]+/)*[A-Za-z0-9._-]+\.[A-Za-z0-9_-]+)\b").unwrap()
});
const COMMON_FILE_EXTENSIONS: &[&str] = &[
    "c", "cc", "cfg", "conf", "cpp", "css", "csv", "doc", "docx", "env", "gif", "h", "hpp",
    "htm", "html", "ics", "ini", "java", "jpeg", "jpg", "js", "json", "lock", "log", "md",
    "pdf", "png", "py", "pyi", "rb", "rs", "scss", "sh", "sql", "svg", "toml", "ts", "tsx",
    "txt", "xml", "yaml", "yml",
];

use crate::core::agent_loop_state::{
    AgentLoopState, AgentPhase, EvalVerdict, LoopTransitionReason,
};
use crate::core::agent_role::{AgentRole, AgentRoleRegistry};
use crate::core::context_engine::{
    ContextEngine, SizedContextEngine, DEFAULT_TOOL_RESULT_BUDGET_CHARS,
};
use crate::core::event_bus::{EventBus, EventType, SystemEvent};
use crate::core::fallback_parser::FallbackParser;
use crate::core::feature_tools;
use crate::core::llm_config_store;
use crate::core::prompt_builder::{PromptMode, ReasoningPolicy};
use crate::core::registration_store::{self, RegisteredPaths, RegistrationKind};
use crate::core::runtime_capabilities;
use crate::core::safety_guard::{SafetyGuard, SideEffect};
use crate::core::skill_capability_manager;
use crate::core::textual_skill_scanner::TextualSkill;
use crate::core::tool_dispatcher::ToolDispatcher;
use crate::infra::key_store::KeyStore;
use crate::llm::backend::{self, LlmBackend, LlmMessage, LlmResponse};
use crate::storage::session_store::SessionStore;

const MAX_CONTEXT_MESSAGES: usize = 120;
const CONTEXT_TOKEN_BUDGET: usize = 0;
const CONTEXT_COMPACT_THRESHOLD: f32 = 0.90;
const MAX_PREFETCHED_SKILLS: usize = 3;
const MAX_OUTBOUND_DASHBOARD_MESSAGES: usize = 200;
const MAX_TELEGRAM_OUTBOUND_CHARS: usize = 4000;
const AUTHENTICATED_BACKEND_PRIORITY_BOOST: i64 = 10_000;
const DEFAULT_FILE_READ_MAX_CHARS: usize = 12_000;
const DEFAULT_FILE_READ_MATCH_WINDOW_CHARS: usize = 240;
const DEFAULT_FILE_READ_MAX_MATCHES: usize = 5;
const SYNTHETIC_INLINE_FULL_TEXT_LIMIT: usize = 6_000;
const SYNTHETIC_EXTRACTION_PREVIEW_CHARS: usize = 2_000;

include!("agent_core/foundation.rs");
include!("agent_core/research.rs");
include!("agent_core/content_and_workspace.rs");
include!("agent_core/news_and_grounding.rs");
include!("agent_core/tool_runtime.rs");
include!("agent_core/runtime_core.rs");
include!("agent_core/runtime_core_impl.rs");
include!("agent_core/process_prompt.rs");
include!("agent_core/runtime_admin_impl.rs");
include!("agent_core/session_store_ref.rs");
include!("agent_core/tests.rs");
