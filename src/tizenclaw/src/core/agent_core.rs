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

const MAX_CONTEXT_MESSAGES: usize = 100;
const CONTEXT_TOKEN_BUDGET: usize = 0;
const CONTEXT_COMPACT_THRESHOLD: f32 = 0.90;
const MAX_PREFETCHED_SKILLS: usize = 3;
const MAX_OUTBOUND_DASHBOARD_MESSAGES: usize = 200;
const MAX_TELEGRAM_OUTBOUND_CHARS: usize = 4000;
const AUTHENTICATED_BACKEND_PRIORITY_BOOST: i64 = 10_000;

#[derive(Clone, Debug, Default)]
struct SessionPromptProfile {
    role_name: Option<String>,
    role_description: Option<String>,
    system_prompt: Option<String>,
    allowed_tools: Option<Vec<String>>,
    max_iterations: Option<usize>,
    role_type: Option<String>,
    can_delegate_to: Option<Vec<String>>,
    prompt_mode: Option<PromptMode>,
    reasoning_policy: Option<ReasoningPolicy>,
}

fn normalize_text_block(text: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut blank_run = 0usize;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            blank_run += 1;
            if !lines.is_empty() && blank_run == 1 {
                lines.push(String::new());
            }
            continue;
        }

        blank_run = 0;
        lines.push(line.to_string());
    }

    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn utf8_safe_preview(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }

    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

fn normalize_conversation_log_text(text: &str) -> Option<String> {
    normalize_text_block(text)
}

fn log_conversation(role: &str, text: &str) {
    if let Some(normalized) = normalize_conversation_log_text(text) {
        log::info!("[Conversation][{}]\n{}", role, normalized);
    }
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn inject_context_message(messages: &mut Vec<LlmMessage>, text: String) {
    if text.trim().is_empty() {
        return;
    }
    let context_message = LlmMessage::user(&text);
    if let Some(last_user_idx) = messages.iter().rposition(|message| message.role == "user") {
        messages.insert(last_user_idx, context_message);
    } else {
        messages.push(context_message);
    }
}

fn sanitize_message_for_transport(mut message: LlmMessage) -> Option<LlmMessage> {
    message.text = message.text.trim().to_string();
    message.reasoning_text = message.reasoning_text.trim().to_string();
    message.tool_name = message.tool_name.trim().to_string();
    message.tool_call_id = message.tool_call_id.trim().to_string();

    let has_text = !message.text.is_empty();
    let has_reasoning = !message.reasoning_text.is_empty();
    let has_tool_calls = !message.tool_calls.is_empty();
    let has_tool_payload = message.role == "tool";

    if has_text || has_reasoning || has_tool_calls || has_tool_payload {
        Some(message)
    } else {
        None
    }
}

fn sanitize_messages_for_transport(messages: Vec<LlmMessage>) -> Vec<LlmMessage> {
    messages
        .into_iter()
        .filter_map(sanitize_message_for_transport)
        .collect()
}

fn estimate_text_tokens(text: &str) -> usize {
    estimate_char_tokens(text.chars().count())
}

fn estimate_char_tokens(chars: usize) -> usize {
    (chars.saturating_add(3)) / 4
}

fn current_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn dashboard_outbound_queue_path(base_dir: &Path) -> PathBuf {
    base_dir.join("outbound").join("web_dashboard.jsonl")
}

fn append_dashboard_outbound_message(
    base_dir: &Path,
    title: Option<&str>,
    message: &str,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Outbound message cannot be empty".to_string());
    }

    let path = dashboard_outbound_queue_path(base_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut entries = if let Ok(content) = std::fs::read_to_string(&path) {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let created_at_ms = current_timestamp_millis();
    let record = json!({
        "id": format!("dashboard-{}", created_at_ms),
        "channel": "web_dashboard",
        "title": title.unwrap_or("TizenClaw"),
        "message": message,
        "session_id": session_id,
        "created_at_ms": created_at_ms,
    });
    entries.push(record.to_string());
    if entries.len() > MAX_OUTBOUND_DASHBOARD_MESSAGES {
        let start = entries.len() - MAX_OUTBOUND_DASHBOARD_MESSAGES;
        entries = entries.split_off(start);
    }

    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    for entry in entries {
        writeln!(file, "{}", entry).map_err(|e| e.to_string())?;
    }

    Ok(record)
}

fn tool_schema_char_count(tools: &[backend::LlmToolDecl]) -> usize {
    tools
        .iter()
        .map(|tool| {
            tool.name.len() + tool.description.len() + tool.parameters.to_string().chars().count()
        })
        .sum()
}

fn total_message_chars(messages: &[LlmMessage]) -> usize {
    messages
        .iter()
        .map(|message| {
            message.text.chars().count()
                + message.reasoning_text.chars().count()
                + message.tool_name.chars().count()
                + message.tool_call_id.chars().count()
                + message.tool_result.to_string().chars().count()
                + message
                    .tool_calls
                    .iter()
                    .map(|tool_call| {
                        tool_call.id.chars().count()
                            + tool_call.name.chars().count()
                            + tool_call.args.to_string().chars().count()
                    })
                    .sum::<usize>()
        })
        .sum()
}

fn log_payload_breakdown(
    session_id: &str,
    prompt: &str,
    history: &[crate::storage::session_store::SessionMessage],
    prefetched_skill_context: Option<&str>,
    dynamic_context: Option<&str>,
    memory_context: Option<&str>,
    system_prompt: &str,
    tools: &[backend::LlmToolDecl],
    messages: &[LlmMessage],
    context_engine: &SizedContextEngine,
) {
    let history_chars: usize = history.iter().map(|msg| msg.text.chars().count()).sum();
    let system_chars = system_prompt.chars().count();
    let skill_chars = prefetched_skill_context
        .map(|text| text.chars().count())
        .unwrap_or(0);
    let runtime_chars = dynamic_context
        .map(|text| text.chars().count())
        .unwrap_or(0);
    let memory_chars = memory_context.map(|text| text.chars().count()).unwrap_or(0);
    let tool_schema_chars = tool_schema_char_count(tools);
    let message_chars = total_message_chars(messages);
    let estimated_message_tokens = context_engine.estimate_tokens(messages);
    let estimated_system_tokens = estimate_text_tokens(system_prompt);
    let estimated_tool_schema_tokens = estimate_char_tokens(tool_schema_chars);

    log::debug!(
        "[PayloadBreakdown] session='{}'\n  prompt_chars={} (~{} tok)\n  history_msgs={} history_chars={} (~{} tok)\n  prefetched_skill_chars={} (~{} tok)\n  runtime_context_chars={} (~{} tok)\n  memory_context_chars={} (~{} tok)\n  system_prompt_chars={} (~{} tok)\n  tools={} tool_schema_chars={} (~{} tok)\n  transport_msgs={} transport_chars={} (~{} tok)\n  estimated_total_input_tokens~={}",
        session_id,
        prompt.chars().count(),
        estimate_text_tokens(prompt),
        history.len(),
        history_chars,
        estimate_char_tokens(history_chars),
        skill_chars,
        estimate_char_tokens(skill_chars),
        runtime_chars,
        estimate_char_tokens(runtime_chars),
        memory_chars,
        estimate_char_tokens(memory_chars),
        system_chars,
        estimated_system_tokens,
        tools.len(),
        tool_schema_chars,
        estimated_tool_schema_tokens,
        messages.len(),
        message_chars,
        estimated_message_tokens,
        estimated_system_tokens + estimated_tool_schema_tokens + estimated_message_tokens
    );
}

fn skill_relevance_score(prompt: &str, skill: &TextualSkill) -> usize {
    let prompt_lower = prompt.to_lowercase();
    let searchable = skill.searchable_text.as_str();

    let mut score = 0;
    if prompt_lower.len() >= 3 && searchable.contains(&prompt_lower) {
        score += 4;
    }

    for token in prompt_lower.split(|c: char| !c.is_alphanumeric()) {
        if token.len() >= 2 && searchable.contains(token) {
            score += 1;
        }
    }

    for trigger in &skill.triggers {
        let trigger_lower = trigger.to_lowercase();
        if !trigger_lower.is_empty() && prompt_lower.contains(&trigger_lower) {
            score += 4;
        }
    }

    for example in &skill.examples {
        let example_lower = example.to_lowercase();
        if !example_lower.is_empty() && prompt_lower.contains(&example_lower) {
            score += 3;
        }
    }

    for tag in &skill.tags {
        let tag_lower = tag.to_lowercase();
        if !tag_lower.is_empty() && prompt_lower.contains(&tag_lower) {
            score += 2;
        }
    }

    score
}

fn select_relevant_skills(
    prompt: &str,
    skills: &[TextualSkill],
    limit: usize,
) -> Vec<TextualSkill> {
    let mut scored: Vec<(usize, TextualSkill)> = skills
        .iter()
        .cloned()
        .filter_map(|skill| {
            let score = skill_relevance_score(prompt, &skill);
            (score > 0).then_some((score, skill))
        })
        .collect();

    scored.sort_by(|(left_score, left_skill), (right_score, right_skill)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_skill.file_name.cmp(&right_skill.file_name))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill)
        .collect()
}

fn role_relevance_score(goal: &str, role: &AgentRole) -> usize {
    let goal_lower = goal.to_lowercase();
    let searchable = format!(
        "{} {} {}",
        role.name.to_lowercase(),
        role.description.to_lowercase(),
        role.system_prompt.to_lowercase()
    );

    let mut score = 0;
    if goal_lower.len() >= 3 && searchable.contains(&goal_lower) {
        score += 5;
    }

    for token in goal_lower.split(|c: char| !c.is_alphanumeric()) {
        if token.len() >= 2 && searchable.contains(token) {
            score += 1;
        }
    }

    score
}

fn select_delegate_roles(goal: &str, roles: &[AgentRole], limit: usize) -> Vec<AgentRole> {
    let mut scored: Vec<(usize, AgentRole)> = roles
        .iter()
        .cloned()
        .map(|role| (role_relevance_score(goal, &role), role))
        .collect();

    scored.sort_by(|(left_score, left_role), (right_score, right_role)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_role.name.cmp(&right_role.name))
    });

    let mut selected: Vec<AgentRole> = scored
        .iter()
        .filter(|(score, _)| *score > 0)
        .take(limit)
        .map(|(_, role)| role.clone())
        .collect();

    if selected.is_empty() {
        selected = scored
            .into_iter()
            .take(limit.max(1))
            .map(|(_, role)| role)
            .collect();
    }

    selected
}

fn build_skill_prefetch_message(skills: &[TextualSkill]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut lines = vec![
        "## Prefetched Skill Snapshot".to_string(),
        "These skills look relevant to the current request. Read the full skill only if you need its exact workflow.".to_string(),
    ];
    for skill in skills {
        lines.push(format!(
            "- {}: {}",
            skill.file_name,
            format_skill_summary(skill)
        ));
    }

    Some(lines.join("\n"))
}

fn collect_tool_roots(paths: &libtizenclaw_core::framework::paths::PlatformPaths) -> Vec<String> {
    let mut roots = vec![paths.tools_dir.to_string_lossy().to_string()];
    roots.extend(RegisteredPaths::load(&paths.config_dir).tool_paths);
    roots.sort();
    roots.dedup();
    roots
}

fn collect_skill_roots(paths: &libtizenclaw_core::framework::paths::PlatformPaths) -> Vec<String> {
    let mut roots = vec![
        paths.skills_dir.to_string_lossy().to_string(),
        paths.skill_hubs_dir.to_string_lossy().to_string(),
    ];
    roots.extend(
        paths
            .discover_skill_hub_roots()
            .into_iter()
            .map(|path| path.to_string_lossy().to_string()),
    );
    roots.extend(RegisteredPaths::load(&paths.config_dir).skill_paths);
    roots.sort();
    roots.dedup();
    roots
}

fn resolve_skill_file(skill_roots: &[String], normalized_name: &str) -> Option<PathBuf> {
    for root in skill_roots {
        for candidate_name in [
            normalized_name.to_string(),
            normalized_name.replace('_', "-"),
        ] {
            let candidate = Path::new(root).join(candidate_name).join("SKILL.md");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn build_progress_marker(
    response_text: &str,
    reasoning_text: &str,
    tool_calls: &[backend::LlmToolCall],
) -> String {
    if !tool_calls.is_empty() {
        let signatures = tool_calls
            .iter()
            .map(|tool_call| format!("{}:{}", tool_call.name, tool_call.args))
            .collect::<Vec<_>>()
            .join("|");
        return format!("<tool_calls>{}</tool_calls>", signatures);
    }

    let trimmed_text = response_text.trim();
    if !trimmed_text.is_empty() {
        return trimmed_text.to_string();
    }

    let trimmed_reasoning = reasoning_text.trim();
    if !trimmed_reasoning.is_empty() {
        return format!("<reasoning>{}</reasoning>", trimmed_reasoning);
    }

    "<empty-response>".into()
}

fn extract_final_text(response_text: &str) -> String {
    if let Some(start) = response_text.find("<final>") {
        if let Some(end) = response_text.rfind("</final>") {
            if end > start + 7 {
                return response_text[start + 7..end].trim().to_string();
            }
            return response_text[start + 7..].trim().to_string();
        }
        return response_text[start + 7..].trim().to_string();
    }

    let stripped_think = THINK_RE.replace_all(response_text, "");
    let normalized = stripped_think.trim();
    if normalized.is_empty() {
        response_text.trim().to_string()
    } else {
        normalized.to_string()
    }
}

fn extract_json_block(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("```") {
        let after_lang = rest.find('\n')?;
        let body = &rest[after_lang + 1..];
        let end = body.rfind("```")?;
        return Some(body[..end].trim().to_string());
    }

    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    Some(trimmed[start..=end].trim().to_string())
}

fn generated_web_app_args_from_text(text: &str) -> Option<Value> {
    let candidate = extract_json_block(text)?;
    let parsed: Value = serde_json::from_str(&candidate).ok()?;
    let obj = parsed.as_object()?;

    let app_id = obj.get("app_id")?.as_str()?.trim();
    if app_id.is_empty() {
        return None;
    }
    let title = obj
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(app_id);

    let mut html = obj
        .get("html")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let mut css = obj
        .get("css")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let mut js = obj
        .get("js")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    if let Some(pages) = obj.get("pages").and_then(|value| value.as_array()) {
        for page in pages {
            let name = page
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let content = page
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            if content.is_empty() {
                continue;
            }
            match name.as_str() {
                "index.html" | "index.htm" => html = content,
                "style.css" if css.is_empty() => css = content,
                "app.js" | "index.js" | "script.js" if js.is_empty() => js = content,
                _ if html.is_empty() && name.ends_with(".html") => html = content,
                _ => {}
            }
        }
    }

    if html.trim().is_empty() {
        return None;
    }

    let mut args = json!({
        "app_id": app_id,
        "title": title,
        "html": html,
    });
    if !css.trim().is_empty() {
        args["css"] = Value::String(css);
    }
    if !js.trim().is_empty() {
        args["js"] = Value::String(js);
    }
    if let Some(allowed_tools) = obj.get("allowed_tools").and_then(|value| value.as_array()) {
        args["allowed_tools"] = Value::Array(
            allowed_tools
                .iter()
                .filter_map(|value| value.as_str().map(|item| Value::String(item.to_string())))
                .collect(),
        );
    }
    Some(args)
}

fn prompt_mode_from_doc(doc: &Value, backend_name: &str) -> PromptMode {
    match doc
        .get("prompt")
        .and_then(|value| value.get("mode"))
        .and_then(|value| value.as_str())
        .unwrap_or("auto")
    {
        "full" => PromptMode::Full,
        "minimal" => PromptMode::Minimal,
        _ => match backend_name {
            "ollama" => PromptMode::Minimal,
            _ => PromptMode::Full,
        },
    }
}

fn reasoning_policy_from_doc(doc: &Value, backend_name: &str) -> ReasoningPolicy {
    match doc
        .get("prompt")
        .and_then(|value| value.get("reasoning_policy"))
        .and_then(|value| value.as_str())
        .unwrap_or("auto")
    {
        "tagged" => ReasoningPolicy::Tagged,
        "native" => ReasoningPolicy::Native,
        _ => match backend_name {
            "ollama" => ReasoningPolicy::Tagged,
            _ => ReasoningPolicy::Native,
        },
    }
}

fn prompt_mode_from_str(value: Option<&str>) -> Option<PromptMode> {
    match value.map(str::trim) {
        Some("full") => Some(PromptMode::Full),
        Some("minimal") => Some(PromptMode::Minimal),
        _ => None,
    }
}

fn reasoning_policy_from_str(value: Option<&str>) -> Option<ReasoningPolicy> {
    match value.map(str::trim) {
        Some("native") => Some(ReasoningPolicy::Native),
        Some("tagged") => Some(ReasoningPolicy::Tagged),
        _ => None,
    }
}

fn format_skill_summary(skill: &TextualSkill) -> String {
    let mut parts = vec![skill.description.clone()];
    if !skill.tags.is_empty() {
        parts.push(format!("tags: {}", skill.tags.join(", ")));
    }
    if !skill.triggers.is_empty() {
        parts.push(format!("triggers: {}", skill.triggers.join(" | ")));
    }
    if !skill.openclaw_requires.is_empty() {
        parts.push(format!("requires: {}", skill.openclaw_requires.join(", ")));
    }
    if !skill.openclaw_install.is_empty() {
        parts.push(format!("install: {}", skill.openclaw_install.join(" | ")));
    }
    parts.join(" | ")
}

fn build_role_supervisor_hint(session_id: &str, goal: &str, role: &AgentRole) -> String {
    format!(
        "Supervisor goal from session '{}': {}\n\
Role: {}\n\
Focus: {}\n\
Work only within this role's scope. Return concise role-specific findings, \
recommended actions, and any missing information needed for the next step.",
        session_id, goal, role.name, role.description
    )
}

fn list_known_sessions(paths: &libtizenclaw_core::framework::paths::PlatformPaths) -> Vec<String> {
    let root = paths.data_dir.join("sessions");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut sessions = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect::<Vec<_>>();
    sessions.sort();
    sessions
}

fn parse_shell_like_args(args: &str) -> Vec<String> {
    let mut parsed = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';

    for ch in args.chars() {
        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
            } else {
                current.push(ch);
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quotes = true;
            quote_char = ch;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                parsed.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        parsed.push(current);
    }

    parsed
}

fn normalize_explicit_path_token(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\'' | '`' | ',' | ';' | ':' | '.' | '(' | ')' | '[' | ']' | '{' | '}'
            )
    });
    if !trimmed.starts_with('/') {
        return None;
    }
    let candidate = trimmed.trim_end_matches(|ch: char| ch == '.' || ch == ',');
    let candidate = if candidate.len() > 1 {
        candidate.trim_end_matches('/')
    } else {
        candidate
    };
    if candidate.is_empty() {
        return None;
    }
    Some(candidate.to_string())
}

/// Filter out path-like tokens that are actually fragments of URLs,
/// user-agent strings, or shebang lines — not real filesystem paths.
fn is_likely_false_positive_path(path: &str) -> bool {
    // Version-like fragments: /5.0, /7.68.0, /1.1, /2.0
    if path
        .trim_start_matches('/')
        .chars()
        .next()
        .map_or(false, |ch| ch.is_ascii_digit())
    {
        return true;
    }
    // Common well-known non-file paths from code strings
    let known_fp = [
        "/bin/bash",
        "/bin/sh",
        "/usr/bin/env",
        "/usr/bin/python",
        "/usr/bin/python3",
        "/usr/bin/node",
        "/dev/null",
    ];
    if known_fp.contains(&path) {
        return true;
    }
    // Very short paths (< 4 chars) like /v1, /v2 are likely API fragments
    if path.len() < 4 {
        return true;
    }
    false
}

fn extract_explicit_paths(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for captures in EXPLICIT_PATH_RE.captures_iter(text) {
        if let Some(path) = captures
            .get(0)
            .and_then(|matched| normalize_explicit_path_token(matched.as_str()))
        {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}

fn path_looks_like_file(path: &str) -> bool {
    let path_obj = Path::new(path);
    std::fs::metadata(path_obj)
        .map(|metadata| metadata.is_file())
        .unwrap_or_else(|_| path_obj.extension().is_some())
}

fn path_looks_like_directory(path: &str) -> bool {
    let path_obj = Path::new(path);
    std::fs::metadata(path_obj)
        .map(|metadata| metadata.is_dir())
        .unwrap_or_else(|_| path_obj.extension().is_none())
}

fn extract_explicit_file_paths(text: &str) -> Vec<String> {
    extract_explicit_paths(text)
        .into_iter()
        .filter(|path| path_looks_like_file(path))
        .collect()
}

fn extract_explicit_directory_paths(text: &str) -> Vec<String> {
    extract_explicit_paths(text)
        .into_iter()
        .filter(|path| path_looks_like_directory(path))
        .collect()
}

fn extract_relative_file_paths(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();

    for captures in RELATIVE_FILE_RE.captures_iter(text) {
        let Some(matched) = captures.get(1).map(|value| {
            value.as_str().trim_matches(|ch| {
                matches!(ch, '`' | '"' | '\'' | ',' | '.' | ':' | ';' | ')' | '(')
            })
        }) else {
            continue;
        };
        if matched.is_empty()
            || matched.starts_with('/')
            || matched.contains("://")
            || matched.starts_with("www.")
        {
            continue;
        }
        if Path::new(matched)
            .extension()
            .and_then(|value| value.to_str())
            .map(|ext| ext.chars().all(|ch| ch.is_ascii_digit()))
            .unwrap_or(false)
        {
            continue;
        }
        let normalized = matched.trim_start_matches("./").to_string();
        if seen.insert(normalized.clone()) {
            paths.push(normalized);
        }
    }

    paths
}

fn prompt_mentions_readme(text: &str) -> bool {
    normalize_prompt_intent_text(text)
        .to_ascii_lowercase()
        .contains("readme")
}

fn expected_file_management_targets(prompt: &str) -> Vec<Vec<String>> {
    let intent_prompt = normalize_prompt_intent_text(prompt);
    let mut groups = extract_relative_file_paths(intent_prompt)
        .into_iter()
        .map(|path| vec![path])
        .collect::<Vec<_>>();

    if prompt_mentions_readme(intent_prompt)
        && !groups.iter().any(|group| {
            group.iter().any(|path| {
                path.eq_ignore_ascii_case("README") || path.eq_ignore_ascii_case("README.md")
            })
        })
    {
        groups.push(vec!["README".to_string(), "README.md".to_string()]);
    }

    groups
}

fn prompt_mentions_history_or_memory(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "history", "memory", "remember", "previous", "earlier", "context", "continue", "again",
    ]
    .iter()
    .any(|keyword| prompt_lower.contains(keyword))
}

fn normalize_prompt_intent_text(prompt: &str) -> &str {
    prompt
        .split_once("Telegram user request:\n")
        .map(|(_, user_request)| user_request.trim())
        .filter(|user_request| !user_request.is_empty())
        .unwrap_or(prompt)
}

fn prompt_requests_executable_script(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    (prompt_lower.contains("script")
        || prompt_lower.contains("python")
        || prompt_lower.contains("bash")
        || prompt_lower.contains("node")
        || prompt_lower.contains("executable"))
        && (prompt_lower.contains("run")
            || prompt_lower.contains("execute")
            || prompt_lower.contains("launch")
            || prompt_lower.contains("command"))
}

fn is_simple_file_management_request(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let has_file_action = [
        "create", "write", "save", "edit", "update", "append", "read", "list", "show", "remove",
        "delete", "copy", "move", "rename", "mkdir", "open", "display", "print", "view", "읽",
        "열", "보여", "출력", "확인", "조회", "내용",
    ]
    .iter()
    .any(|keyword| prompt_lower.contains(keyword));
    let has_file_target = [
        "file",
        "files",
        "directory",
        "folder",
        "project structure",
        "working directory",
        "readme",
        ".md",
        ".txt",
        ".json",
        ".yaml",
        ".yml",
        ".py",
        ".js",
        ".ts",
        ".rs",
        "src/",
        "파일",
        "디렉터리",
        "폴더",
        "경로",
        ".toml",
        ".log",
    ]
    .iter()
    .any(|keyword| prompt_lower.contains(keyword))
        || !extract_explicit_paths(normalize_prompt_intent_text(prompt)).is_empty();

    has_file_action && has_file_target && !prompt_requests_executable_script(prompt)
}

fn should_skip_memory_for_prompt(prompt: &str) -> bool {
    is_simple_file_management_request(prompt) && !prompt_mentions_history_or_memory(prompt)
}

fn should_prefetch_prompt_file(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|value| value.to_str()),
        Some("md" | "txt" | "json" | "yaml" | "yml")
    )
}

fn load_prefetched_prompt_file_previews(paths: &[String]) -> Vec<(String, String, bool)> {
    const MAX_PREFETCH_FILES: usize = 3;
    const MAX_PREFETCH_BYTES: u64 = 64 * 1024;
    const MAX_PREFETCH_CHARS: usize = 12_000;

    paths
        .iter()
        .filter(|path| should_prefetch_prompt_file(path))
        .take(MAX_PREFETCH_FILES)
        .filter_map(|path| {
            let metadata = std::fs::metadata(path).ok()?;
            if !metadata.is_file() || metadata.len() > MAX_PREFETCH_BYTES {
                return None;
            }
            let content = std::fs::read_to_string(path).ok()?;
            let preview = utf8_safe_preview(&content, MAX_PREFETCH_CHARS).to_string();
            Some((path.clone(), preview.clone(), preview.len() < content.len()))
        })
        .collect()
}

fn build_prefetched_prompt_file_messages(paths: &[String]) -> Vec<LlmMessage> {
    load_prefetched_prompt_file_previews(paths)
        .into_iter()
        .enumerate()
        .map(|(idx, (path, preview, truncated))| {
            LlmMessage::tool_result(
                &format!("prefetch_prompt_file_{}", idx + 1),
                "read_file",
                json!({
                    "path": path,
                    "content": preview,
                    "prefetched": true,
                    "truncated": truncated,
                }),
            )
        })
        .collect()
}

fn build_prefetched_prompt_file_context(paths: &[String]) -> Option<String> {
    const MAX_CONTEXT_CHARS_PER_FILE: usize = 2_000;

    let previews = load_prefetched_prompt_file_previews(paths);
    if previews.is_empty() {
        return None;
    }

    let sections = previews
        .into_iter()
        .map(|(path, preview, truncated)| {
            let shortened = utf8_safe_preview(&preview, MAX_CONTEXT_CHARS_PER_FILE);
            let suffix = if truncated || shortened.len() < preview.len() {
                "\n...[truncated]"
            } else {
                ""
            };
            format!("### {}\n```\n{}{}\n```", path, shortened.trim_end(), suffix)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    Some(format!(
        "## Prompt File Excerpts\nThese prefetched files are authoritative. Follow their exact requirements and do not invent extra questions, columns, or output formats.\n\n{}",
        sections
    ))
}

fn parse_markdown_level_requirements(markdown: &str) -> Vec<(String, String)> {
    let mut current_level: Option<String> = None;
    let mut requirements = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(captures) = MARKDOWN_LEVEL_HEADING_RE.captures(trimmed) {
            current_level = captures.get(1).map(|value| value.as_str().to_string());
            continue;
        }

        if let Some(level) = current_level.as_ref() {
            if let Some(question) = trimmed.strip_prefix("**문제:**") {
                requirements.push((level.clone(), question.trim().to_string()));
                current_level = None;
            }
        }
    }

    requirements
}

fn resolve_markdown_requirement_csv_path(problem_path: &str, requirement: &str) -> Option<String> {
    if let Some(path) = extract_explicit_file_paths(requirement).into_iter().next() {
        return Some(path);
    }

    let file_name = CSV_FILE_NAME_RE
        .captures(requirement)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))?;
    let parent = Path::new(problem_path).parent()?;
    Some(parent.join(file_name).to_string_lossy().to_string())
}

fn summarize_requirement_directive(requirement: &str, csv_path: Option<&str>) -> String {
    let csv_suffix = csv_path
        .map(|path| format!(" using {}", path))
        .unwrap_or_default();

    if requirement.contains("가장 많이 팔렸") {
        return format!(
            "Count the frequency of Fruit{} and print only the single most common fruit name.",
            csv_suffix
        );
    }
    if requirement.contains("평균") {
        return format!(
            "Compute the arithmetic mean of Hours{} and print only that single average value.",
            csv_suffix
        );
    }
    if requirement.contains("차이") || requirement.contains("Range") {
        return format!(
            "Compute max(Height_cm) - min(Height_cm){} and print only that single range value.",
            csv_suffix
        );
    }
    if requirement.contains("이상치") {
        return format!(
            "Use the IQR rule on Score{} and print only the final outlier count.",
            csv_suffix
        );
    }
    if requirement.contains("회귀 계수")
        || requirement.contains("beta_1")
        || requirement.contains("β1")
    {
        return format!(
            "Estimate the normal-equation OLS coefficients from X1, X2, Y{} and print only beta_1 rounded to three decimals.",
            csv_suffix
        );
    }

    requirement.to_string()
}

fn build_authoritative_problem_requirements_context(paths: &[String]) -> Option<String> {
    let previews = load_prefetched_prompt_file_previews(paths);
    let mut lines = Vec::new();

    for (path, preview, _truncated) in previews {
        for (level, requirement) in parse_markdown_level_requirements(&preview) {
            let csv_path = resolve_markdown_requirement_csv_path(&path, &requirement);
            let directive = summarize_requirement_directive(&requirement, csv_path.as_deref());
            match csv_path {
                Some(path) => lines.push(format!(
                    "- Level {} uses {}. Final directive: {}",
                    level, path, directive
                )),
                None => lines.push(format!("- Level {} final directive: {}", level, directive)),
            }
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!(
            "## Authoritative Level Requirements\nUse these exact level tasks from the prefetched problem document. Do not add extra metrics, helper outputs, or invented columns.\n{}\nEach generated script should print exactly one final answer value after '[Level N] Answer:'. All generated scripts must execute successfully on the current target runtime. If pandas or numpy are unavailable, fall back to the Python standard library instead of failing.",
            lines.join("\n")
        ))
    }
}

fn parse_tool_stdout_json(result: &Value) -> Option<Value> {
    let stdout = result
        .get("stdout")
        .and_then(|value| value.as_str())?
        .trim();
    serde_json::from_str(stdout).ok()
}

fn collect_grounded_paths_from_value(value: &Value, grounded_paths: &mut HashSet<String>) {
    if let Some(path) = value.get("path").and_then(|item| item.as_str()) {
        grounded_paths.insert(path.to_string());
        if let Some(entries) = value.get("entries").and_then(|item| item.as_array()) {
            for entry in entries {
                if let Some(name) = entry.get("name").and_then(|item| item.as_str()) {
                    grounded_paths.insert(format!("{}/{}", path.trim_end_matches('/'), name));
                }
            }
        }
    }
}

fn collect_grounded_paths(messages: &[LlmMessage]) -> HashSet<String> {
    let mut grounded_paths = HashSet::new();
    for message in messages.iter().filter(|item| item.role == "tool") {
        match message.tool_name.as_str() {
            "read_file" | "list_files" | "extract_document_text" | "inspect_tabular_data" => {
                collect_grounded_paths_from_value(&message.tool_result, &mut grounded_paths);
                if let Some(stdout_json) = parse_tool_stdout_json(&message.tool_result) {
                    collect_grounded_paths_from_value(&stdout_json, &mut grounded_paths);
                }
            }
            _ => {}
        }
    }
    grounded_paths
}

fn script_references_any_known_input(script_text: &str, known_paths: &HashSet<String>) -> bool {
    known_paths.iter().any(|path| {
        script_text.contains(path)
            || std::path::Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .map(|name| script_text.contains(name))
                .unwrap_or(false)
    })
}

fn path_is_within_any_directory(path: &str, directories: &HashSet<String>) -> bool {
    let path_obj = Path::new(path);
    directories
        .iter()
        .any(|dir| path_obj.starts_with(Path::new(dir)))
}

fn prompt_requires_persisted_level_scripts(prompt: &str) -> bool {
    prompt.contains("level-{problem level}-solution.py")
        || (prompt.contains("/result/library-used") && prompt.contains("/result/library-not-used"))
}

fn extract_prompt_level_numbers(prompt: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut levels = extract_explicit_file_paths(prompt)
        .into_iter()
        .filter_map(|path| {
            let file_name = Path::new(&path).file_name()?.to_str()?;
            let captures = LEVEL_INPUT_RE.captures(file_name)?;
            let level = captures.get(1)?.as_str().to_string();
            if seen.insert(level.clone()) {
                Some(level)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    levels.sort_by_key(|value| value.parse::<usize>().unwrap_or(usize::MAX));
    levels
}

fn extract_level_answer_markers(code: &str) -> HashSet<String> {
    LEVEL_ANSWER_RE
        .captures_iter(code)
        .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .collect()
}

fn extract_declared_output_path_from_leading_comment(code: &str) -> Option<String> {
    let first_content_line = code.lines().map(str::trim).find(|line| !line.is_empty())?;
    let comment_body = first_content_line
        .strip_prefix('#')
        .or_else(|| first_content_line.strip_prefix("//"))?
        .trim();
    let candidate = normalize_explicit_path_token(comment_body)?;
    if candidate != comment_body || !path_looks_like_file(&candidate) {
        return None;
    }
    Some(candidate)
}

fn extract_level_number_from_output_path(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_str()?;
    LEVEL_OUTPUT_LEVEL_RE
        .captures(file_name)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
}

fn expected_persisted_level_script_paths(prompt: &str) -> Vec<String> {
    if !prompt_requires_persisted_level_scripts(prompt) {
        return Vec::new();
    }

    let output_dirs = extract_explicit_directory_paths(prompt);
    let levels = extract_prompt_level_numbers(prompt);
    if output_dirs.is_empty() || levels.is_empty() {
        return Vec::new();
    }

    let mut expected_paths = output_dirs
        .into_iter()
        .flat_map(|dir| {
            levels.iter().map(move |level| {
                format!("{}/level-{}-solution.py", dir.trim_end_matches('/'), level)
            })
        })
        .collect::<Vec<_>>();
    expected_paths.sort();
    expected_paths
}

fn parse_csv_headers_from_preview(preview: &str) -> HashSet<String> {
    preview
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.split(',')
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default()
}

fn collect_grounded_csv_headers(messages: &[LlmMessage]) -> HashSet<String> {
    let mut headers = HashSet::new();

    for message in messages.iter().filter(|message| message.role == "tool") {
        match message.tool_name.as_str() {
            "extract_document_text" => {
                let path = message
                    .tool_result
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if !path.ends_with(".csv") {
                    continue;
                }
                if let Some(preview) = message
                    .tool_result
                    .get("text_preview")
                    .and_then(|value| value.as_str())
                {
                    headers.extend(parse_csv_headers_from_preview(preview));
                }
            }
            "inspect_tabular_data" => {
                if let Some(sheets) = message
                    .tool_result
                    .get("inspection")
                    .and_then(|value| value.get("sheets"))
                    .and_then(|value| value.as_array())
                {
                    for sheet in sheets {
                        if let Some(sheet_headers) =
                            sheet.get("headers").and_then(|value| value.as_array())
                        {
                            headers.extend(
                                sheet_headers
                                    .iter()
                                    .filter_map(|value| value.as_str())
                                    .map(|value| value.to_string()),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    headers
}

fn extract_indexed_column_names(code: &str) -> HashSet<String> {
    let mut columns = HashSet::new();
    let chars = code.char_indices().collect::<Vec<_>>();

    for (idx, ch) in chars.iter().copied() {
        if ch != '[' {
            continue;
        }

        let prev_non_ws = code[..idx]
            .chars()
            .rev()
            .find(|value| !value.is_whitespace());
        if !matches!(prev_non_ws, Some(value) if value.is_ascii_alphanumeric() || value == '_' || value == ')' || value == ']')
        {
            continue;
        }

        let mut depth = 0usize;
        let mut end_idx = None;
        for (candidate_idx, candidate) in code[idx..].char_indices() {
            match candidate {
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end_idx = Some(idx + candidate_idx + candidate.len_utf8());
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(end_idx) = end_idx else {
            continue;
        };
        let segment = &code[idx..end_idx];
        if segment.contains('/') {
            continue;
        }
        columns.extend(
            QUOTED_IDENTIFIER_RE
                .captures_iter(segment)
                .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string())),
        );
    }

    columns
}

fn collect_successful_saved_output_paths(messages: &[LlmMessage]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| message.role == "tool" && message.tool_name == "run_generated_code")
        .filter(|message| {
            message
                .tool_result
                .get("result")
                .and_then(|value| value.get("success"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|message| {
            message
                .tool_result
                .get("saved_output_path")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct GeneratedCodeGrounding {
    declared_output_path: Option<String>,
    declared_output_level: Option<String>,
}

fn prompt_requires_atomic_level_answer(prompt: &str) -> bool {
    prompt.contains("[Level X] Answer:")
}

fn validate_generated_code_execution_output(
    stdout: &str,
    expected_level: Option<&str>,
    enforce_atomic_answer: bool,
) -> Result<(), String> {
    let answer_lines = LEVEL_ANSWER_LINE_RE
        .captures_iter(stdout)
        .filter_map(|captures| {
            let level = captures.get(1)?.as_str().to_string();
            let answer = captures.get(2)?.as_str().trim().to_string();
            Some((level, answer))
        })
        .collect::<Vec<_>>();

    if let Some(level) = expected_level {
        if answer_lines.len() != 1 {
            return Err(format!(
                "Generated script for level {} must print exactly one '[Level {}] Answer:' line.",
                level, level
            ));
        }
        if answer_lines[0].0 != level {
            return Err(format!(
                "Generated script was saved as level {}, but printed a level {} answer line.",
                level, answer_lines[0].0
            ));
        }
    }

    if enforce_atomic_answer {
        for (level, answer) in answer_lines {
            if answer.is_empty() {
                return Err(format!(
                    "Generated script for level {} printed an empty answer value.",
                    level
                ));
            }
            if answer.split_whitespace().count() > 1 {
                return Err(format!(
                    "Generated script for level {} printed multiple answer tokens ('{}'). Print exactly one final answer value after '[Level {}] Answer:'.",
                    level, answer, level
                ));
            }
            if answer.contains('|') || answer.contains('\n') || answer.contains('\t') {
                return Err(format!(
                    "Generated script for level {} printed a compound answer ('{}'). Print exactly one final answer value after '[Level {}] Answer:'.",
                    level, answer, level
                ));
            }
        }
    }

    Ok(())
}

fn validate_generated_code_grounding(
    prompt: &str,
    grounded_paths: &HashSet<String>,
    grounded_csv_headers: &HashSet<String>,
    code: &str,
    args: &str,
) -> Result<GeneratedCodeGrounding, String> {
    let prompt_paths = extract_explicit_paths(prompt);
    let prompt_files = extract_explicit_file_paths(prompt);
    let prompt_directories = extract_explicit_directory_paths(prompt)
        .into_iter()
        .collect::<HashSet<_>>();
    if prompt_files.is_empty() && prompt_directories.is_empty() {
        return Ok(GeneratedCodeGrounding::default());
    }

    let missing_paths = prompt_files
        .iter()
        .filter(|path| {
            // Only require prior inspection for files that actually exist on disk.
            // Files referenced in the prompt that don't exist yet are likely output
            // targets — the agent is creating them, not reading them.
            !grounded_paths.contains(path.as_str()) && Path::new(path).exists()
        })
        .cloned()
        .collect::<Vec<_>>();
    if !missing_paths.is_empty() {
        return Err(format!(
            "Inspect the referenced input files before executing generated code: {}. Use read_file, extract_document_text, or inspect_tabular_data first.",
            missing_paths.join(", ")
        ));
    }

    let combined = if args.trim().is_empty() {
        code.to_string()
    } else {
        format!("{}\n{}", code, args)
    };
    let combined_paths = extract_explicit_file_paths(&combined);
    let known_paths = prompt_paths
        .iter()
        .chain(grounded_paths.iter())
        .cloned()
        .collect::<HashSet<_>>();
    let unexpected_paths = combined_paths
        .into_iter()
        .filter(|path| {
            !known_paths.contains(path)
                && !path_is_within_any_directory(path, &prompt_directories)
                && !is_likely_false_positive_path(path)
        })
        .collect::<Vec<_>>();
    if !unexpected_paths.is_empty() {
        return Err(format!(
            "Generated code references unverified file paths: {}. Use only the inspected inputs or the user-provided paths.",
            unexpected_paths.join(", ")
        ));
    }

    let known_input_paths = prompt_files
        .iter()
        .filter(|path| Path::new(path.as_str()).exists())
        .chain(
            grounded_paths
                .iter()
                .filter(|path| path_looks_like_file(path)),
        )
        .cloned()
        .collect::<HashSet<_>>();
    // Only enforce input-grounding when there are known input files.
    // Tasks without input files (e.g. "create a weather script") should
    // not be blocked by the grounding check.
    if !known_input_paths.is_empty()
        && !script_references_any_known_input(&combined, &known_input_paths)
    {
        return Err(
            "Generated code is not grounded in the inspected input files. Do not substitute mock data or invented datasets; reference the real inputs or pass them through args."
                .to_string(),
        );
    }

    if SPECULATION_RE.is_match(code) {
        return Err(
            "Generated code contains speculative assumptions or placeholder logic. Use only the exact problem statement, real CSV headers, and real data values."
                .to_string(),
        );
    }

    if !grounded_csv_headers.is_empty() {
        let unexpected_columns = extract_indexed_column_names(&combined)
            .into_iter()
            .filter(|column| !grounded_csv_headers.contains(column))
            .collect::<Vec<_>>();
        if !unexpected_columns.is_empty() {
            let mut known_headers = grounded_csv_headers.iter().cloned().collect::<Vec<_>>();
            known_headers.sort();
            return Err(format!(
                "Generated code references unverified CSV columns: {}. Use only inspected headers: {}.",
                unexpected_columns.join(", "),
                known_headers.join(", ")
            ));
        }
    }

    let declared_output_path = extract_declared_output_path_from_leading_comment(code)
        .filter(|path| path_is_within_any_directory(path, &prompt_directories));
    let declared_output_level = declared_output_path
        .as_deref()
        .and_then(extract_level_number_from_output_path);

    if prompt_requires_persisted_level_scripts(prompt) && !prompt_directories.is_empty() {
        let Some(output_path) = declared_output_path.as_ref() else {
            return Err(
                "Persist each generated script by placing the exact absolute output file path in a leading comment line, for example '# /tmp/ds_olympiad/result/library-used/level-1-solution.py'."
                    .to_string(),
            );
        };
        let file_name = Path::new(output_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !LEVEL_OUTPUT_FILE_RE.is_match(file_name) {
            return Err(format!(
                "Use the requested filename format 'level-N-solution.py' for generated scripts, not '{}'.",
                file_name
            ));
        }

        if let Some(level) = declared_output_level.as_ref() {
            let matching_level_inputs = prompt_files
                .iter()
                .filter(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|file_name| LEVEL_INPUT_RE.captures(file_name))
                        .and_then(|captures| {
                            captures.get(1).map(|value| value.as_str().to_string())
                        })
                        .as_deref()
                        == Some(level.as_str())
                })
                .cloned()
                .collect::<Vec<_>>();
            let matching_level_input_set = matching_level_inputs
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            if !matching_level_inputs.is_empty()
                && !script_references_any_known_input(&combined, &matching_level_input_set)
            {
                return Err(format!(
                    "Generated level {} script must use the matching inspected input file(s): {}.",
                    level,
                    matching_level_inputs.join(", ")
                ));
            }

            let unrelated_level_inputs = prompt_files
                .iter()
                .filter(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|file_name| LEVEL_INPUT_RE.captures(file_name))
                        .and_then(|captures| {
                            captures.get(1).map(|value| value.as_str().to_string())
                        })
                        .is_some_and(|candidate| candidate != *level)
                })
                .cloned()
                .collect::<Vec<_>>();
            let cross_level_inputs = unrelated_level_inputs
                .into_iter()
                .filter(|path| {
                    combined.contains(path)
                        || Path::new(path)
                            .file_name()
                            .and_then(|value| value.to_str())
                            .map(|name| combined.contains(name))
                            .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            if !cross_level_inputs.is_empty() {
                return Err(format!(
                    "Generated level {} script references other levels' input files: {}.",
                    level,
                    cross_level_inputs.join(", ")
                ));
            }
        }

        let level_markers = extract_level_answer_markers(code);
        if level_markers.len() > 1 {
            return Err(
                "Generate exactly one level per script. One run_generated_code call must produce one level-N-solution.py file."
                    .to_string(),
            );
        }
    }

    Ok(GeneratedCodeGrounding {
        declared_output_path,
        declared_output_level,
    })
}

fn persist_generated_code_copy(code: &str, output_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create generated output directory '{}': {}",
                parent.display(),
                err
            )
        })?;
    }

    let mut file = std::fs::File::create(output_path).map_err(|err| {
        format!(
            "Failed to create generated output file '{}': {}",
            output_path.display(),
            err
        )
    })?;
    file.write_all(code.as_bytes()).map_err(|err| {
        format!(
            "Failed to write generated output file '{}': {}",
            output_path.display(),
            err
        )
    })?;
    file.flush().map_err(|err| {
        format!(
            "Failed to flush generated output file '{}': {}",
            output_path.display(),
            err
        )
    })
}

fn resolve_workspace_path(session_workdir: &Path, path_str: &str) -> Result<PathBuf, String> {
    if path_str.trim().is_empty() {
        return Err("Missing required path".to_string());
    }

    let path = Path::new(path_str);
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        session_workdir.join(path)
    })
}

fn collect_successful_file_management_actions(messages: &[LlmMessage]) -> usize {
    messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter(|message| matches!(message.tool_name.as_str(), "file_manager" | "file_write"))
        .filter(|message| {
            message
                .tool_result
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .count()
}

fn collect_successful_file_management_paths(messages: &[LlmMessage]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter(|message| matches!(message.tool_name.as_str(), "file_manager" | "file_write"))
        .filter(|message| {
            message
                .tool_result
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|message| {
            message
                .tool_result
                .get("path")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect()
}

fn missing_file_management_targets(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
) -> Vec<String> {
    let expected_groups = expected_file_management_targets(prompt);
    if expected_groups.is_empty() {
        return Vec::new();
    }

    let created_paths = collect_successful_file_management_paths(messages);
    expected_groups
        .into_iter()
        .filter_map(|group| {
            let satisfied = group.iter().any(|candidate| {
                let absolute = session_workdir.join(candidate);
                absolute.exists()
                    || created_paths.contains(absolute.to_string_lossy().as_ref())
                    || created_paths.contains(candidate.as_str())
            });
            if satisfied {
                None
            } else {
                group.first().cloned()
            }
        })
        .collect()
}

async fn file_manager_tool(tc_args: &Value, session_workdir: &Path) -> Value {
    let force_rust_fallback = tc_args
        .get("backend_preference")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().eq_ignore_ascii_case("rust_fallback"))
        .unwrap_or(false);
    let operation = tc_args
        .get("operation")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match operation.as_str() {
        "read" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::read_file_via_system(&path).await {
                    Ok(content) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "content": content,
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] read fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "content": content,
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to read file '{}': {}", path.display(), error)})
                }
            }
        }
        "write" | "append" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let content = tc_args
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if let Some(parent) = path.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return json!({"error": format!("Failed to create directory '{}': {}", parent.display(), error)});
                }
            }
            let write_result = if operation == "append" {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut file| std::io::Write::write_all(&mut file, content.as_bytes()))
            } else {
                std::fs::write(&path, content.as_bytes())
            };
            match write_result {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "bytes_written": content.len()
                }),
                Err(error) => {
                    json!({"error": format!("Failed to {} file '{}': {}", operation, path.display(), error)})
                }
            }
        }
        "remove" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !path.exists() {
                return json!({"error": format!("Failed to remove '{}': path does not exist", path.display())});
            }
            let is_dir = path.is_dir();
            if !force_rust_fallback {
                match runtime_capabilities::remove_via_system(&path, is_dir).await {
                    Ok(()) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] remove fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            let result = if is_dir {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match result {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to remove '{}': {}", path.display(), error)})
                }
            }
        }
        "mkdir" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::mkdir_via_system(&path).await {
                    Ok(()) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] mkdir fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::create_dir_all(&path) {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to create directory '{}': {}", path.display(), error)})
                }
            }
        }
        "list" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::list_dir_via_system(&path).await {
                    Ok(entries) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "entries": entries.into_iter().map(|entry| json!({
                                "name": entry.name,
                                "path": entry.path,
                                "is_dir": entry.is_dir,
                                "size": entry.size,
                            })).collect::<Vec<_>>(),
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] list fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let entries = entries
                        .filter_map(|entry| entry.ok())
                        .map(|entry| {
                            let entry_path = entry.path();
                            let metadata = entry.metadata().ok();
                            json!({
                                "name": entry.file_name().to_string_lossy(),
                                "path": entry_path.to_string_lossy(),
                                "is_dir": metadata.as_ref().map(|value| value.is_dir()).unwrap_or(false),
                                "size": metadata.as_ref().map(|value| value.len()).unwrap_or(0)
                            })
                        })
                        .collect::<Vec<_>>();
                    json!({
                        "success": true,
                        "operation": operation,
                        "path": path.to_string_lossy(),
                        "entries": entries,
                        "backend": "rust_fallback",
                    })
                }
                Err(error) => {
                    json!({"error": format!("Failed to list directory '{}': {}", path.display(), error)})
                }
            }
        }
        "stat" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::stat_path_via_system(&path).await {
                    Ok(metadata) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "is_dir": metadata.is_dir,
                            "is_file": metadata.is_file,
                            "size": metadata.size,
                            "readonly": metadata.readonly,
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] stat fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::metadata(&path) {
                Ok(metadata) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "is_dir": metadata.is_dir(),
                    "is_file": metadata.is_file(),
                    "size": metadata.len(),
                    "readonly": metadata.permissions().readonly(),
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to stat '{}': {}", path.display(), error)})
                }
            }
        }
        "copy" | "move" => {
            let src_str = tc_args
                .get("src")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let dst_str = tc_args
                .get("dst")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let src = match resolve_workspace_path(session_workdir, src_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": format!("src: {}", error)}),
            };
            let dst = match resolve_workspace_path(session_workdir, dst_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": format!("dst: {}", error)}),
            };
            if let Some(parent) = dst.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return json!({"error": format!("Failed to create destination directory '{}': {}", parent.display(), error)});
                }
            }
            let recursive = src.is_dir();
            if !force_rust_fallback {
                let system_result = if operation == "move" {
                    runtime_capabilities::move_via_system(&src, &dst)
                        .await
                        .map(|_| 0u64)
                } else {
                    runtime_capabilities::copy_via_system(&src, &dst, recursive).await
                };
                match system_result {
                    Ok(bytes) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "src": src.to_string_lossy(),
                            "dst": dst.to_string_lossy(),
                            "bytes_copied": bytes,
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] {} fallback '{}' -> '{}': {}",
                            operation,
                            src.display(),
                            dst.display(),
                            system_error
                        );
                    }
                }
            }
            let result = if operation == "move" {
                std::fs::rename(&src, &dst).map(|_| 0u64)
            } else {
                std::fs::copy(&src, &dst)
            };
            match result {
                Ok(bytes) => json!({
                    "success": true,
                    "operation": operation,
                    "src": src.to_string_lossy(),
                    "dst": dst.to_string_lossy(),
                    "bytes_copied": bytes,
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to {} '{}' -> '{}': {}", operation, src.display(), dst.display(), error)})
                }
            }
        }
        "download" => {
            let url = tc_args
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            let dest_str = tc_args
                .get("dest")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if url.is_empty() {
                return json!({"error": "Missing required url"});
            }
            let dest = match resolve_workspace_path(session_workdir, dest_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if let Some(parent) = dest.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return json!({"error": format!("Failed to create destination directory '{}': {}", parent.display(), error)});
                }
            }
            let response = crate::infra::http_client::http_get(url, &[], 1, 60).await;
            if !response.success {
                return json!({"error": format!("Failed to download '{}': {}", url, response.error)});
            }
            match std::fs::write(&dest, response.body.as_bytes()) {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "url": url,
                    "dest": dest.to_string_lossy(),
                    "bytes_written": response.body.len()
                }),
                Err(error) => {
                    json!({"error": format!("Failed to save download to '{}': {}", dest.display(), error)})
                }
            }
        }
        _ => json!({"error": format!("Unsupported file_manager operation '{}'", operation)}),
    }
}

fn extract_option_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn canonical_tool_trace(tc: &backend::LlmToolCall) -> Value {
    let default = json!({
        "type": "toolCall",
        "tool_call_id": tc.id,
        "name": tc.name,
        "params": tc.args,
        "arguments": tc.args,
        "actual_tool_name": tc.name
    });

    if tc.name == "file_manager" {
        let operation = tc
            .args
            .get("operation")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        return match operation.as_str() {
            "read" => {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "read_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path,
                        "file_path": path,
                        "files": [path],
                        "operation": operation,
                        "actual_tool_name": tc.name
                    },
                    "arguments": {
                        "path": path,
                        "file_path": path,
                        "files": [path]
                    }
                })
            }
            "write" | "append" => {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let content = tc
                    .args
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "write_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path,
                        "file_path": path,
                        "content": content,
                        "operation": operation,
                        "actual_tool_name": tc.name
                    },
                    "arguments": {
                        "path": path,
                        "file_path": path,
                        "content": content
                    }
                })
            }
            "list" => {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or(".");
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "list_files",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path,
                        "operation": operation,
                        "actual_tool_name": tc.name
                    },
                    "arguments": {
                        "path": path
                    }
                })
            }
            _ => default.clone(),
        };
    }

    if tc.name == "run_generated_code" {
        let runtime = tc
            .args
            .get("runtime")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let code = tc
            .args
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();

        if runtime == "bash" && !code.is_empty() {
            let parsed = parse_shell_like_args(code);
            if let Some(command) = parsed.first().map(String::as_str) {
                match command {
                    "cat" => {
                        if let Some(path) = parsed.get(1) {
                            return json!({
                                "type": "toolCall",
                                "tool_call_id": tc.id,
                                "name": "read_file",
                                "actual_tool_name": tc.name,
                                "params": {
                                    "path": path.as_str(),
                                    "file_path": path.as_str(),
                                    "files": [path.as_str()],
                                    "runtime": runtime,
                                    "actual_tool_name": tc.name,
                                    "raw_code": code
                                },
                                "arguments": {
                                    "path": path.as_str(),
                                    "file_path": path.as_str(),
                                    "files": [path.as_str()]
                                }
                            });
                        }
                    }
                    "ls" => {
                        let path = parsed
                            .iter()
                            .skip(1)
                            .find(|value| !value.starts_with('-'))
                            .map(String::as_str)
                            .unwrap_or(".");
                        return json!({
                            "type": "toolCall",
                            "tool_call_id": tc.id,
                            "name": "list_files",
                            "actual_tool_name": tc.name,
                            "params": {
                                "path": path,
                                "runtime": runtime,
                                "actual_tool_name": tc.name,
                                "raw_code": code
                            },
                            "arguments": {
                                "path": path
                            }
                        });
                    }
                    "find" => {
                        let path = parsed.get(1).map(String::as_str).unwrap_or(".");
                        return json!({
                            "type": "toolCall",
                            "tool_call_id": tc.id,
                            "name": "list_files",
                            "actual_tool_name": tc.name,
                            "params": {
                                "path": path,
                                "runtime": runtime,
                                "actual_tool_name": tc.name,
                                "raw_code": code
                            },
                            "arguments": {
                                "path": path
                            }
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    let Some(args_str) = tc.args.get("args").and_then(|value| value.as_str()) else {
        return default;
    };

    if !tc.name.contains("file-manager") {
        return default;
    }

    let parsed = parse_shell_like_args(args_str);
    let Some(subcommand) = parsed.first().map(String::as_str) else {
        return default;
    };

    match subcommand {
        "read" => {
            if let Some(path) = extract_option_value(&parsed, "--path") {
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "read_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "files": [path.as_str()],
                        "subcommand": subcommand,
                        "actual_tool_name": tc.name,
                        "raw_args": args_str
                    },
                    "arguments": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "files": [path.as_str()]
                    }
                })
            } else {
                default
            }
        }
        "write" | "append" => {
            if let Some(path) = extract_option_value(&parsed, "--path") {
                let content = extract_option_value(&parsed, "--content").unwrap_or_default();
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "write_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "content": content.as_str(),
                        "subcommand": subcommand,
                        "actual_tool_name": tc.name,
                        "raw_args": args_str
                    },
                    "arguments": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "content": content.as_str()
                    }
                })
            } else {
                default
            }
        }
        "list" => {
            if let Some(path) = extract_option_value(&parsed, "--path") {
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "list_files",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path.as_str(),
                        "subcommand": subcommand,
                        "actual_tool_name": tc.name,
                        "raw_args": args_str
                    },
                    "arguments": {
                        "path": path.as_str()
                    }
                })
            } else {
                default
            }
        }
        _ => default,
    }
}

fn generated_code_runtime_spec(runtime: &str) -> Option<(&'static str, &'static str)> {
    match runtime.trim().to_ascii_lowercase().as_str() {
        "python" | "python3" => Some(("python3", ".py")),
        "node" => Some(("node", ".js")),
        "bash" => Some(("bash", ".sh")),
        _ => None,
    }
}

fn sanitize_generated_code_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut previous_was_dash = false;

    for ch in name.chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() {
            slug.push(lowered);
            previous_was_dash = false;
        } else if !previous_was_dash && !slug.is_empty() {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "script".to_string()
    } else {
        slug
    }
}

fn generated_code_date_prefix() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm_buf) };

    format!(
        "{:04}-{:02}-{:02}",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday
    )
}

fn generated_code_script_path(base_dir: &Path, runtime: &str, name: &str) -> Option<PathBuf> {
    let (_, suffix) = generated_code_runtime_spec(runtime)?;
    let codes_dir = base_dir.join("codes");
    let date_prefix = generated_code_date_prefix();
    let base_name = sanitize_generated_code_name(name);

    for attempt in 0..1000usize {
        let file_name = if attempt == 0 {
            format!("{date_prefix}-generated-{base_name}{suffix}")
        } else {
            format!("{date_prefix}-generated-{base_name}-{attempt}{suffix}")
        };
        let candidate = codes_dir.join(file_name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn list_generated_code_entries(codes_dir: &Path) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(codes_dir).map_err(|err| {
        format!(
            "Failed to read codes dir '{}': {}",
            codes_dir.display(),
            err
        )
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|err| format!("Failed to read codes entry: {}", err))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|err| format!("Failed to read metadata for '{}': {}", path.display(), err))?;
        let name = entry.file_name().to_string_lossy().to_string();
        entries.push(json!({
            "name": name,
            "path": path.to_string_lossy().to_string(),
            "size_bytes": metadata.len(),
        }));
    }

    entries.sort_by(|left, right| {
        let left_name = left.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let right_name = right.get("name").and_then(|v| v.as_str()).unwrap_or("");
        left_name.cmp(right_name)
    });

    Ok(entries)
}

fn validate_generated_code_filename(name: &str) -> Result<&str, String> {
    if name.is_empty() {
        return Err("Missing generated code filename".to_string());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(format!("Invalid generated code filename '{}'", name));
    }
    Ok(name)
}

fn manage_generated_code_tool(operation: &str, name: Option<&str>, base_dir: &Path) -> Value {
    let codes_dir = base_dir.join("codes");
    if let Err(err) = std::fs::create_dir_all(&codes_dir) {
        return json!({"error": format!("Failed to create codes dir: {}", err)});
    }

    match operation {
        "list" => match list_generated_code_entries(&codes_dir) {
            Ok(entries) => json!({
                "status": "success",
                "operation": "list",
                "count": entries.len(),
                "entries": entries,
            }),
            Err(err) => json!({ "error": err }),
        },
        "delete" => {
            let Some(file_name) = name else {
                return json!({"error": "Missing 'name' for delete operation"});
            };
            let file_name = match validate_generated_code_filename(file_name) {
                Ok(file_name) => file_name,
                Err(err) => return json!({ "error": err }),
            };
            let target = codes_dir.join(file_name);
            if !target.exists() {
                return json!({
                    "error": format!("Generated code file '{}' was not found", file_name)
                });
            }
            match std::fs::remove_file(&target) {
                Ok(()) => json!({
                    "status": "success",
                    "operation": "delete",
                    "name": file_name,
                    "path": target.to_string_lossy().to_string(),
                }),
                Err(err) => json!({
                    "error": format!("Failed to delete '{}': {}", target.display(), err)
                }),
            }
        }
        "delete_all" => match list_generated_code_entries(&codes_dir) {
            Ok(entries) => {
                let mut deleted = Vec::new();
                for entry in entries {
                    if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                        if let Err(err) = std::fs::remove_file(path) {
                            return json!({
                                "error": format!("Failed to delete '{}': {}", path, err)
                            });
                        }
                        deleted.push(path.to_string());
                    }
                }
                json!({
                    "status": "success",
                    "operation": "delete_all",
                    "deleted_count": deleted.len(),
                    "deleted_paths": deleted,
                })
            }
            Err(err) => json!({ "error": err }),
        },
        other => json!({
            "error": format!(
                "Unsupported operation '{}'. Expected list, delete, or delete_all.",
                other
            )
        }),
    }
}

fn task_scheduler_dir(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join("tasks")
}

fn list_tasks_tool(base_dir: &Path) -> Value {
    let task_dir = task_scheduler_dir(base_dir);
    match crate::core::task_scheduler::TaskScheduler::list_tasks_from_dir(&task_dir) {
        Ok(tasks) => json!({
            "status": "success",
            "count": tasks.len(),
            "tasks": tasks.into_iter().map(|task| json!({
                "id": task.id,
                "name": task.name,
                "session_id": task.session_id,
                "interval_secs": task.interval_secs,
                "schedule": task.schedule_expr,
                "one_shot": task.one_shot,
                "enabled": task.enabled,
                "project_dir": task.project_dir,
                "coding_backend": task.coding_backend,
                "coding_model": task.coding_model,
                "execution_mode": task.execution_mode,
                "auto_approve": task.auto_approve,
                "prompt": task.prompt,
            })).collect::<Vec<_>>(),
        }),
        Err(err) => json!({ "error": err }),
    }
}

fn create_task_tool(
    base_dir: &Path,
    schedule: &str,
    prompt: &str,
    project_dir: Option<&str>,
    coding_backend: Option<&str>,
    coding_model: Option<&str>,
    execution_mode: Option<&str>,
    auto_approve: bool,
) -> Value {
    let task_dir = task_scheduler_dir(base_dir);
    match crate::core::task_scheduler::TaskScheduler::create_task_file(
        &task_dir,
        schedule,
        prompt,
        project_dir,
        coding_backend,
        coding_model,
        execution_mode,
        auto_approve,
    ) {
        Ok(task) => json!({
            "status": "success",
            "task": {
                "id": task.id,
                "name": task.name,
                "session_id": task.session_id,
                "interval_secs": task.interval_secs,
                "schedule": task.schedule_expr,
                "one_shot": task.one_shot,
                "enabled": task.enabled,
                "project_dir": task.project_dir,
                "coding_backend": task.coding_backend,
                "coding_model": task.coding_model,
                "execution_mode": task.execution_mode,
                "auto_approve": task.auto_approve,
                "prompt": task.prompt,
            }
        }),
        Err(err) => json!({ "error": err }),
    }
}

fn cancel_task_tool(base_dir: &Path, task_id: &str) -> Value {
    let task_dir = task_scheduler_dir(base_dir);
    match crate::core::task_scheduler::TaskScheduler::delete_task_file(&task_dir, task_id) {
        Ok(true) => json!({
            "status": "success",
            "task_id": task_id.trim(),
        }),
        Ok(false) => json!({
            "error": format!("Task '{}' was not found", task_id.trim()),
        }),
        Err(err) => json!({ "error": err }),
    }
}

async fn run_generated_code_tool(
    runtime: &str,
    name: Option<&str>,
    code: &str,
    args: &str,
    base_dir: &Path,
    workdir: Option<&Path>,
    declared_output_path: Option<&str>,
    declared_output_level: Option<&str>,
    enforce_atomic_answer: bool,
) -> Value {
    let code_trimmed = code.trim_start();
    let looks_like_web_markup = code_trimmed.starts_with("<!DOCTYPE html")
        || code_trimmed.starts_with("<html")
        || code_trimmed.contains("<head")
        || code_trimmed.contains("<body")
        || code_trimmed.contains("<script")
        || code_trimmed.contains("<style");
    if looks_like_web_markup {
        return json!({
            "error": "HTML/CSS/JS browser content is not supported by run_generated_code. Use generate_web_app instead."
        });
    }

    let binary = match generated_code_runtime_spec(runtime) {
        Some((binary, _suffix)) => binary,
        None => {
            return json!({
                "error": format!(
                    "Unsupported runtime '{}'. Expected python, python3, node, or bash.",
                    runtime
                )
            });
        }
    };

    let target_dir = workdir
        .map(|path| path.join("codes"))
        .unwrap_or_else(|| base_dir.join("codes"));
    if let Err(err) = std::fs::create_dir_all(&target_dir) {
        return json!({"error": format!("Failed to create codes dir: {}", err)});
    }

    let Some((_, suffix)) = generated_code_runtime_spec(runtime) else {
        return json!({"error": format!("Unsupported runtime '{}'.", runtime)});
    };
    let script_name = format!(
        "{}{}",
        sanitize_generated_code_name(name.unwrap_or("script")),
        suffix
    );
    let script_path = target_dir.join(script_name);
    let mut temp_file = match std::fs::File::create(&script_path) {
        Ok(file) => file,
        Err(err) => {
            return json!({"error": format!("Failed to create code file: {}", err)});
        }
    };

    if let Err(err) = temp_file.write_all(code.as_bytes()) {
        return json!({"error": format!("Failed to write generated code: {}", err)});
    }
    if let Err(err) = temp_file.flush() {
        return json!({"error": format!("Failed to flush generated code: {}", err)});
    }
    let script_path = script_path.to_string_lossy().to_string();
    let mut exec_args = vec![script_path.clone()];
    exec_args.extend(parse_shell_like_args(args));
    let exec_args_ref: Vec<&str> = exec_args.iter().map(|value| value.as_str()).collect();

    let engine = crate::infra::container_engine::ContainerEngine::new();
    let cwd = target_dir.to_string_lossy().to_string();
    match engine
        .execute_oneshot(binary, &exec_args_ref, Some(cwd.as_str()))
        .await
    {
        Ok(result) => {
            let mut saved_output_path = None;
            let stderr = result
                .get("stderr")
                .and_then(|value| value.as_str())
                .unwrap_or("");

            if result
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                let stdout = result
                    .get("stdout")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if let Err(err) = validate_generated_code_execution_output(
                    stdout,
                    declared_output_level,
                    enforce_atomic_answer,
                ) {
                    return json!({
                        "runtime": runtime,
                        "script_path": script_path,
                        "name": script_path.rsplit('/').next().unwrap_or(""),
                        "saved_output_path": saved_output_path,
                        "result": result,
                        "error": err
                    });
                }

                if let Some(path) = declared_output_path {
                    let output_path = Path::new(path);
                    if let Err(err) = persist_generated_code_copy(code, output_path) {
                        return json!({ "error": err });
                    }
                    saved_output_path = Some(output_path.to_string_lossy().to_string());
                }
            }

            let module_error = if stderr.contains("ModuleNotFoundError")
                && (stderr.contains("pandas") || stderr.contains("numpy"))
            {
                Some(
                    "Required third-party Python packages are unavailable on the target runtime. Generate code that executes successfully with the available standard library instead of importing missing packages."
                        .to_string(),
                )
            } else {
                None
            };

            json!({
                "runtime": runtime,
                "script_path": script_path,
                "name": script_path.rsplit('/').next().unwrap_or(""),
                "saved_output_path": saved_output_path,
                "result": result,
                "error": module_error
            })
        }
        Err(err) => json!({
            "runtime": runtime,
            "script_path": script_path,
            "name": script_path.rsplit('/').next().unwrap_or(""),
            "saved_output_path": serde_json::Value::Null,
            "error": format!("Failed to execute generated code: {}", err)
        }),
    }
}

/// LLM backend configuration loaded from `llm_config.json`.
#[derive(Debug)]
struct LlmConfig {
    active_backend: String,
    fallback_backends: Vec<String>,
    backends: Value,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self::from_document(&llm_config_store::default_document())
    }
}

impl LlmConfig {
    fn from_document(json: &Value) -> Self {
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
                .unwrap_or_else(|| vec!["openai".into(), "ollama".into()]),
            backends: json.get("backends").cloned().unwrap_or_else(|| json!({})),
        }
    }

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

        Self::from_document(&json)
    }

    /// Get config for a specific backend.
    fn backend_config(&self, name: &str) -> Value {
        self.backends.get(name).cloned().unwrap_or(json!({}))
    }
}

/// Merge backend auth material from `KeyStore` into a backend config `Value`.
///
/// Priority:
///  1. Explicit `api_key` in the JSON config block (non-empty)
///  2. Explicit `oauth.access_token` in the JSON config block (non-empty)
///  3. `{config_dir}/keys/<backend>.key`
///  4. `{config_dir}/keys/<backend>.access_token.key`
///  5. `{config_dir}/keys/<backend>.refresh_token.key`
fn merge_backend_auth(mut cfg: Value, name: &str, ks: &crate::infra::key_store::KeyStore) -> Value {
    let has_api_key = cfg
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let has_access_token = cfg
        .get("oauth")
        .and_then(|v| v.get("access_token"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if !has_api_key && !has_access_token {
        if let Some(key) = ks.get(name) {
            if !key.is_empty() {
                cfg["api_key"] = Value::String(key);
            }
        } else if let Some(access_token) = ks.get(&format!("{}.access_token", name)) {
            if !access_token.is_empty() {
                if !cfg.get("oauth").map(|v| v.is_object()).unwrap_or(false) {
                    cfg["oauth"] = json!({});
                }
                cfg["oauth"]["access_token"] = Value::String(access_token);
            }
        }
    }

    let has_refresh_token = cfg
        .get("oauth")
        .and_then(|v| v.get("refresh_token"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if !has_refresh_token {
        if let Some(refresh_token) = ks.get(&format!("{}.refresh_token", name)) {
            if !refresh_token.is_empty() {
                if !cfg.get("oauth").map(|v| v.is_object()).unwrap_or(false) {
                    cfg["oauth"] = json!({});
                }
                cfg["oauth"]["refresh_token"] = Value::String(refresh_token);
            }
        }
    }

    cfg
}

fn config_string<'a>(config: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = config;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn backend_has_direct_auth(cfg: &Value) -> bool {
    config_string(cfg, &["api_key"]).is_some()
        || config_string(cfg, &["oauth", "access_token"]).is_some()
        || config_string(cfg, &["oauth", "refresh_token"]).is_some()
}

fn codex_auth_path_from_config(cfg: &Value) -> Option<PathBuf> {
    config_string(cfg, &["oauth", "auth_path"]).map(PathBuf::from)
}

fn codex_default_auth_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let home = home.trim();
    if home.is_empty() {
        return None;
    }
    Some(Path::new(home).join(".codex").join("auth.json"))
}

fn codex_auth_file_has_tokens(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };

    doc.get("tokens")
        .and_then(Value::as_object)
        .map(|tokens| {
            ["access_token", "refresh_token"].iter().all(|key| {
                tokens
                    .get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(|value| !value.is_empty())
                    .unwrap_or(false)
            })
        })
        .unwrap_or_else(|| {
            ["access_token", "refresh_token"].iter().all(|key| {
                doc.get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(|value| !value.is_empty())
                    .unwrap_or(false)
            })
        })
}

fn backend_has_preferred_auth(name: &str, cfg: &Value) -> bool {
    if backend_has_direct_auth(cfg) {
        return true;
    }

    if name != "openai-codex" {
        return false;
    }

    codex_auth_path_from_config(cfg)
        .or_else(codex_default_auth_path)
        .map(|path| codex_auth_file_has_tokens(&path))
        .unwrap_or(false)
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

fn sort_backend_candidates(candidates: &mut [BackendCandidate], config: &LlmConfig) {
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
}

fn build_backend_candidates(
    config: &LlmConfig,
    plugin_manager: &crate::llm::plugin_manager::PluginManager,
    ks: &crate::infra::key_store::KeyStore,
) -> Vec<BackendCandidate> {
    let mut candidates = Vec::new();
    let mut all_names: Vec<String> = Vec::new();

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

    for plugin_name in plugin_manager.available_plugins() {
        if !all_names.contains(&plugin_name) {
            all_names.push(plugin_name);
        }
    }

    for name in all_names {
        let mut priority = 0;
        let mut is_explicitly_in_config = false;
        let effective_cfg = merge_backend_auth(config.backend_config(&name), &name, ks);

        if name == config.active_backend
            || config.fallback_backends.contains(&name)
            || config.backends.get(&name).is_some()
        {
            priority = 1;
            is_explicitly_in_config = true;
        }

        if let Some(p) = config
            .backends
            .get(&name)
            .and_then(|v| v.get("priority"))
            .and_then(|v| v.as_i64())
        {
            priority = p;
            is_explicitly_in_config = true;
        }

        if !is_explicitly_in_config {
            if let Some(cfg) = plugin_manager.get_plugin_config(&name) {
                priority = cfg.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }

        if backend_has_preferred_auth(&name, &effective_cfg) {
            // OAuth-backed Codex sessions are more fragile than API-key
            // backends because the daemon may restart while the CLI auth
            // file remains the only surviving credential source. Give any
            // authenticated backend a strong priority lift so valid auth is
            // consumed first instead of silently falling back to a weaker
            // default backend.
            priority = priority.max(AUTHENTICATED_BACKEND_PRIORITY_BOOST);
        }

        candidates.push(BackendCandidate { name, priority });
    }

    sort_backend_candidates(&mut candidates, config);
    candidates
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
    safety_guard: Mutex<SafetyGuard>,
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

impl AgentCore {
    pub fn new(platform: Arc<libtizenclaw_core::framework::PlatformContext>) -> Self {
        let keys_dir = platform.paths.config_dir.join("keys");
        AgentCore {
            platform,
            backend: tokio::sync::RwLock::new(None),
            fallback_backends: tokio::sync::RwLock::new(Vec::new()),
            session_store: Mutex::new(None),
            tool_dispatcher: tokio::sync::RwLock::new(ToolDispatcher::new()),
            safety_guard: Mutex::new(SafetyGuard::new()),
            context_engine: Arc::new(SizedContextEngine::new()),
            event_bus: Arc::new(EventBus::new()),
            key_store: Mutex::new(KeyStore::new(&keys_dir)),
            system_prompt: RwLock::new(String::new()),
            soul_content: RwLock::new(None),
            backend_name: RwLock::new(String::new()),
            llm_config: Mutex::new(LlmConfig::default()),
            circuit_breakers: RwLock::new(std::collections::HashMap::new()),
            action_bridge: Mutex::new(crate::core::action_bridge::ActionBridge::new()),
            tool_policy: Mutex::new(crate::core::tool_policy::ToolPolicy::new()),
            memory_store: Mutex::new(None),
            workflow_engine: tokio::sync::RwLock::new(
                crate::core::workflow_engine::WorkflowEngine::new(),
            ),
            agent_roles: RwLock::new(AgentRoleRegistry::new()),
            session_profiles: Mutex::new(HashMap::new()),
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

    fn role_file_path(&self) -> PathBuf {
        self.platform.paths.config_dir.join("agent_roles.json")
    }

    fn safety_guard_path(&self) -> PathBuf {
        self.platform.paths.config_dir.join("safety_guard.json")
    }

    fn reload_safety_guard(&self) {
        let guard_path = self.safety_guard_path();
        if let Ok(mut safety_guard) = self.safety_guard.lock() {
            *safety_guard = SafetyGuard::new();
            safety_guard.load_config(&guard_path.to_string_lossy());
        }
    }

    fn publish_runtime_event(&self, event_name: &str, data: Value) {
        self.event_bus.publish(SystemEvent {
            event_type: EventType::Custom(event_name.to_string()),
            source: "agent_core".to_string(),
            data,
            timestamp: 0,
        });
    }

    fn persist_compacted_messages(&self, session_id: &str, messages: &[LlmMessage]) {
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                use crate::storage::session_store::SessionMessage;
                let session_msgs: Vec<SessionMessage> = messages
                    .iter()
                    .map(SessionMessage::from_llm_message)
                    .collect();
                if let Err(err) = store.save_compacted_structured(session_id, &session_msgs) {
                    log::warn!(
                        "[ContextEngine] Failed to save compacted structured snapshot: {}",
                        err
                    );
                }
                if let Err(err) = store.save_compacted(session_id, &session_msgs) {
                    log::warn!("[ContextEngine] Failed to save compacted.md: {}", err);
                }
            }
        }
    }

    fn check_context_message_limit(
        &self,
        session_id: &str,
        messages: &[LlmMessage],
        loop_state: &mut AgentLoopState,
    ) -> Result<(), String> {
        if messages.len() <= MAX_CONTEXT_MESSAGES {
            return Ok(());
        }

        let error = format!(
            "Context message limit exceeded for session '{}': {} > {}",
            session_id,
            messages.len(),
            MAX_CONTEXT_MESSAGES
        );
        log::error!("[AgentLoop] {}", error);
        loop_state.last_error = Some(error.clone());
        loop_state.mark_terminal(
            LoopTransitionReason::RoundLimitReached,
            format!(
                "message count {} exceeded {}",
                messages.len(),
                MAX_CONTEXT_MESSAGES
            ),
        );
        self.persist_loop_snapshot(loop_state);
        Err(error)
    }

    fn resolve_session_profile(&self, session_id: &str) -> Option<SessionPromptProfile> {
        self.session_profiles
            .lock()
            .ok()
            .and_then(|profiles| profiles.get(session_id).cloned())
    }

    fn role_registry_snapshot(&self) -> Vec<AgentRole> {
        self.agent_roles
            .read()
            .map(|registry| {
                registry
                    .get_role_names()
                    .into_iter()
                    .filter_map(|name| registry.get_role(&name).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn bridge_builtin_tools() -> Vec<backend::LlmToolDecl> {
        let mut builtins = Vec::new();
        crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_builtin_tools(
            &mut builtins,
            "workflow memory search",
        );
        builtins.push(backend::LlmToolDecl {
            name: "execute_cli".into(),
            description: "Execute a registered CLI tool by name for Bridge API compatibility."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Registered CLI tool name such as tizen-device-info-cli"
                    },
                    "arguments": {
                        "description": "Optional CLI arguments as a string or object",
                        "oneOf": [
                            {"type": "string"},
                            {"type": "object"}
                        ]
                    }
                },
                "required": ["tool_name"]
            }),
        });
        let supported = [
            "execute_cli",
            "generate_web_app",
            "remember",
            "recall",
            "forget",
            "web_search",
            "validate_web_search",
        ];
        builtins.retain(|tool| supported.iter().any(|name| tool.name == *name));
        builtins
    }

    pub async fn get_bridge_tool_declarations(
        &self,
        allowed_tools: &[String],
    ) -> Vec<backend::LlmToolDecl> {
        let mut tools = self.tool_dispatcher.read().await.get_tool_declarations();
        tools.extend(Self::bridge_builtin_tools());
        if let Ok(bridge) = self.action_bridge.lock() {
            tools.extend(bridge.get_action_declarations());
        }

        let mut seen = std::collections::HashSet::new();
        tools.retain(|tool| seen.insert(tool.name.clone()));

        if !allowed_tools.is_empty() {
            tools.retain(|tool| allowed_tools.iter().any(|name| name == &tool.name));
        }

        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    async fn send_outbound_message(&self, args: &Value, default_session_id: Option<&str>) -> Value {
        let channels = args
            .get("channels")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|item| item.trim().to_string()))
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let message = args
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let session_id = args
            .get("session_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(default_session_id);

        if channels.is_empty() {
            return json!({"error": "At least one outbound channel is required"});
        }
        if message.is_empty() {
            return json!({"error": "Outbound message cannot be empty"});
        }

        let mut results = Vec::new();
        for channel in channels {
            match channel.as_str() {
                "web_dashboard" => match append_dashboard_outbound_message(
                    &self.platform.paths.data_dir,
                    title,
                    message,
                    session_id,
                ) {
                    Ok(record) => results.push(json!({
                        "channel": "web_dashboard",
                        "status": "sent",
                        "record": record,
                    })),
                    Err(err) => results.push(json!({
                        "channel": "web_dashboard",
                        "status": "error",
                        "error": err,
                    })),
                },
                "telegram" => {
                    results.push(self.send_telegram_outbound_message(title, message).await);
                }
                other => results.push(json!({
                    "channel": other,
                    "status": "error",
                    "error": "Unsupported outbound channel",
                })),
            }
        }

        let success = results
            .iter()
            .any(|item| item.get("status").and_then(|value| value.as_str()) == Some("sent"));

        json!({
            "status": if success { "success" } else { "error" },
            "message": message,
            "session_id": session_id,
            "results": results,
        })
    }

    async fn send_telegram_outbound_message(&self, title: Option<&str>, message: &str) -> Value {
        let config_path = self.platform.paths.config_dir.join("telegram_config.json");
        let content = match std::fs::read_to_string(&config_path) {
            Ok(content) => content,
            Err(err) => {
                return json!({
                    "channel": "telegram",
                    "status": "error",
                    "error": format!("Failed to read telegram config: {}", err),
                });
            }
        };

        let config: Value = match serde_json::from_str(&content) {
            Ok(config) => config,
            Err(err) => {
                return json!({
                    "channel": "telegram",
                    "status": "error",
                    "error": format!("Invalid telegram config JSON: {}", err),
                });
            }
        };

        let bot_token = config
            .get("bot_token")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if bot_token.is_empty() || bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
            return json!({
                "channel": "telegram",
                "status": "error",
                "error": "Telegram bot token is not configured",
            });
        }

        let chat_ids = config
            .get("allowed_chat_ids")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_i64())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if chat_ids.is_empty() {
            return json!({
                "channel": "telegram",
                "status": "error",
                "error": "No allowed_chat_ids configured for Telegram",
            });
        }

        let composed = if let Some(title) = title {
            format!("{}\n\n{}", title, message)
        } else {
            message.to_string()
        };
        let safe_text = if composed.chars().count() > MAX_TELEGRAM_OUTBOUND_CHARS {
            format!(
                "{}\n...(truncated)",
                utf8_safe_preview(&composed, MAX_TELEGRAM_OUTBOUND_CHARS)
            )
        } else {
            composed
        };

        let client = crate::infra::http_client::HttpClient::new();
        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let mut delivered = Vec::new();
        let mut errors = Vec::new();

        for chat_id in chat_ids {
            let payload = json!({
                "chat_id": chat_id,
                "text": safe_text.clone(),
            })
            .to_string();
            match client.post(&url, &payload).await {
                Ok(resp) if resp.status_code < 400 => delivered.push(chat_id),
                Ok(resp) => errors.push(format!(
                    "chat {} returned HTTP {}",
                    chat_id, resp.status_code
                )),
                Err(err) => errors.push(format!("chat {} failed: {}", chat_id, err)),
            }
        }

        json!({
            "channel": "telegram",
            "status": if delivered.is_empty() { "error" } else { "sent" },
            "delivered_chat_ids": delivered,
            "errors": errors,
        })
    }

    async fn generate_web_app(&self, args: &Value) -> Value {
        let app_id = args
            .get("app_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let html = args
            .get("html")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let css = args
            .get("css")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let js = args
            .get("js")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if app_id.is_empty() || app_id.len() > 64 {
            return json!({"error": "app_id is required (max 64 chars)"});
        }
        if !app_id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            return json!({"error": "app_id must be lowercase alphanumeric + underscore only"});
        }
        if title.is_empty() {
            return json!({"error": "title is required"});
        }
        if html.is_empty() {
            return json!({"error": "html is required"});
        }

        let html = Self::inject_generated_web_app_assets(html, !css.is_empty(), !js.is_empty());

        let apps_dir = self.platform.paths.web_root.join("apps");
        let app_dir = apps_dir.join(app_id);
        if let Err(err) = std::fs::create_dir_all(&app_dir) {
            return json!({"error": format!("Failed to create app directory: {}", err)});
        }

        if let Err(err) = std::fs::write(app_dir.join("index.html"), html) {
            return json!({"error": format!("Failed to write index.html: {}", err)});
        }
        if !css.is_empty() {
            if let Err(err) = std::fs::write(app_dir.join("style.css"), css) {
                return json!({"error": format!("Failed to write style.css: {}", err)});
            }
        }
        if !js.is_empty() {
            if let Err(err) = std::fs::write(app_dir.join("app.js"), js) {
                return json!({"error": format!("Failed to write app.js: {}", err)});
            }
        }

        let mut downloaded_assets = Vec::new();
        if let Some(assets) = args.get("assets").and_then(|value| value.as_array()) {
            let client = reqwest::Client::new();
            for asset in assets {
                let url = asset
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                let filename = asset
                    .get("filename")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();

                if url.is_empty() || filename.is_empty() {
                    continue;
                }
                if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
                    downloaded_assets.push(json!({
                        "filename": filename,
                        "status": "failed",
                        "error": "Unsafe filename",
                    }));
                    continue;
                }

                let target_path = app_dir.join(filename);
                let asset_result = async {
                    let response = client
                        .get(url)
                        .send()
                        .await
                        .map_err(|err| err.to_string())?;
                    if !response.status().is_success() {
                        return Err(format!("HTTP {}", response.status()));
                    }
                    if let Some(len) = response.content_length() {
                        if len > 10 * 1024 * 1024 {
                            return Err("Asset exceeds 10MB limit".to_string());
                        }
                    }
                    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
                    if bytes.len() > 10 * 1024 * 1024 {
                        return Err("Asset exceeds 10MB limit".to_string());
                    }
                    std::fs::write(&target_path, &bytes).map_err(|err| err.to_string())?;
                    Ok::<(), String>(())
                }
                .await;

                match asset_result {
                    Ok(()) => downloaded_assets.push(json!({
                        "filename": filename,
                        "status": "ok",
                    })),
                    Err(err) => {
                        let _ = std::fs::remove_file(&target_path);
                        downloaded_assets.push(json!({
                            "filename": filename,
                            "status": "failed",
                            "error": err,
                        }));
                    }
                }
            }
        }

        let mut manifest = json!({
            "app_id": app_id,
            "title": title,
            "created_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "has_css": !css.is_empty(),
            "has_js": !js.is_empty(),
            "assets": downloaded_assets,
        });

        if let Some(allowed_tools) = args.get("allowed_tools").and_then(|value| value.as_array()) {
            manifest["allowed_tools"] = Value::Array(
                allowed_tools
                    .iter()
                    .filter_map(|value| value.as_str().map(|item| Value::String(item.to_string())))
                    .collect(),
            );
        }

        let manifest_bytes = match serde_json::to_vec_pretty(&manifest) {
            Ok(bytes) => bytes,
            Err(err) => {
                return json!({"error": format!("Failed to encode manifest.json: {}", err)});
            }
        };
        if let Err(err) = std::fs::write(app_dir.join("manifest.json"), manifest_bytes) {
            return json!({"error": format!("Failed to write manifest.json: {}", err)});
        }

        let is_tizen = std::path::Path::new("/etc/tizen-release").exists();
        let dashboard_base_url = crate::core::runtime_paths::default_dashboard_base_url();
        let launched = self.launch_generated_web_app(app_id);
        let message = if is_tizen {
            if launched {
                format!(
                    "Web app created and launched at {}/apps/{}/",
                    dashboard_base_url, app_id
                )
            } else {
                format!(
                    "Web app created. Access at {}/apps/{}/",
                    dashboard_base_url, app_id
                )
            }
        } else {
            format!(
                "Web app created. Open {}/apps/{}/ on the host.",
                dashboard_base_url, app_id
            )
        };
        json!({
            "status": "ok",
            "app_id": app_id,
            "title": title,
            "url": format!("/apps/{}/", app_id),
            "webview_launched": launched,
            "message": message,
            "assets": manifest["assets"].clone(),
        })
    }

    fn inject_generated_web_app_assets(html: &str, has_css: bool, has_js: bool) -> String {
        let mut rendered = html.to_string();

        if has_css && !html.contains("style.css") {
            let link_tag = r#"<link rel="stylesheet" href="style.css">"#;
            if let Some(index) = rendered.find("</head>") {
                rendered.insert_str(index, &format!("    {}\n", link_tag));
            } else if let Some(index) = rendered.find("<body") {
                rendered.insert_str(index, &format!("{}\n", link_tag));
            } else {
                rendered.push_str(&format!("\n{}\n", link_tag));
            }
        }

        if has_js && !html.contains("app.js") {
            let script_tag = r#"<script src="app.js"></script>"#;
            if let Some(index) = rendered.rfind("</body>") {
                rendered.insert_str(index, &format!("    {}\n", script_tag));
            } else if let Some(index) = rendered.rfind("</html>") {
                rendered.insert_str(index, &format!("{}\n", script_tag));
            } else {
                rendered.push_str(&format!("\n{}\n", script_tag));
            }
        }

        rendered
    }

    fn launch_generated_web_app(&self, app_id: &str) -> bool {
        if !std::path::Path::new("/etc/tizen-release").exists() {
            return false;
        }

        let app_url = format!(
            "{}/apps/{}/",
            crate::core::runtime_paths::default_dashboard_base_url(),
            app_id
        );
        if self.launch_app_with_bundle("QvaPeQ7RDA.tizenclawbridge", &[("url", app_url.as_str())]) {
            return true;
        }
        if self.launch_app_open("QvaPeQ7RDA.tizenclawbridge") {
            return true;
        }
        if self.launch_app_with_bundle(
            "org.tizen.tizenclaw-webview",
            &[("__APP_SVC_URI__", app_url.as_str())],
        ) {
            return true;
        }
        if self.launch_app_open("org.tizen.tizenclaw-webview") {
            return true;
        }
        if self.launch_app_with_request(
            "QvaPeQ7RDA.tizenclawbridge",
            &app_url,
            &[("url", app_url.as_str())],
        ) {
            return true;
        }
        if self.launch_app_with_request(
            "org.tizen.tizenclaw-webview",
            &app_url,
            &[("__APP_SVC_URI__", app_url.as_str())],
        ) {
            return true;
        }
        self.platform
            .app_control
            .launch_app("org.tizen.tizenclaw-webview")
            .is_ok()
    }

    fn launch_app_with_bundle(&self, app_id: &str, extras: &[(&str, &str)]) -> bool {
        unsafe {
            use libtizenclaw_core::tizen_sys::{aul::*, bundle::*};

            let Ok(app_id) = std::ffi::CString::new(app_id) else {
                return false;
            };
            let bundle = bundle_create();
            if bundle.is_null() {
                return false;
            }

            for (key, value) in extras {
                let Ok(key) = std::ffi::CString::new(*key) else {
                    continue;
                };
                let Ok(value) = std::ffi::CString::new(*value) else {
                    continue;
                };
                let _ = bundle_add_str(bundle, key.as_ptr(), value.as_ptr());
            }

            let result = aul_launch_app(app_id.as_ptr(), bundle.cast());
            let _ = bundle_free(bundle);
            result >= 0
        }
    }

    fn launch_app_open(&self, app_id: &str) -> bool {
        unsafe {
            use libtizenclaw_core::tizen_sys::aul::aul_open_app;

            let Ok(app_id) = std::ffi::CString::new(app_id) else {
                return false;
            };
            aul_open_app(app_id.as_ptr()) >= 0
        }
    }

    fn launch_app_with_request(&self, app_id: &str, uri: &str, extras: &[(&str, &str)]) -> bool {
        unsafe {
            use libtizenclaw_core::tizen_sys::app_control::*;

            let mut handle: app_control_h = std::ptr::null_mut();
            if app_control_create(&mut handle) != APP_CONTROL_ERROR_NONE {
                return false;
            }

            let operation =
                match std::ffi::CString::new("http://tizen.org/appcontrol/operation/default") {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = app_control_destroy(handle);
                        return false;
                    }
                };
            let app_id = match std::ffi::CString::new(app_id) {
                Ok(value) => value,
                Err(_) => {
                    let _ = app_control_destroy(handle);
                    return false;
                }
            };
            let uri = match std::ffi::CString::new(uri) {
                Ok(value) => value,
                Err(_) => {
                    let _ = app_control_destroy(handle);
                    return false;
                }
            };

            let _ = app_control_set_operation(handle, operation.as_ptr());
            let _ = app_control_set_app_id(handle, app_id.as_ptr());
            let _ = app_control_set_uri(handle, uri.as_ptr());

            for (key, value) in extras {
                let Ok(key) = std::ffi::CString::new(*key) else {
                    continue;
                };
                let Ok(value) = std::ffi::CString::new(*value) else {
                    continue;
                };
                let _ = app_control_add_extra_data(handle, key.as_ptr(), value.as_ptr());
            }

            let result = app_control_send_launch_request(handle, None, std::ptr::null_mut());
            let _ = app_control_destroy(handle);
            result == APP_CONTROL_ERROR_NONE
        }
    }

    pub async fn execute_bridge_tool(
        &self,
        tool_name: &str,
        args: &Value,
        allowed_tools: &[String],
    ) -> Value {
        if !allowed_tools.is_empty() && !allowed_tools.iter().any(|name| name == tool_name) {
            return json!({"error": format!("Tool not allowed for this app: {}", tool_name)});
        }

        if let Ok(policy) = self.tool_policy.lock() {
            if matches!(
                policy.get_risk_level(tool_name),
                crate::core::tool_policy::RiskLevel::High
            ) {
                return json!({"error": format!("High-risk tool not available via bridge: {}", tool_name)});
            }
        }

        match tool_name {
            "execute_cli" => {
                let requested_tool = args
                    .get("tool_name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                if requested_tool.is_empty() {
                    return json!({"error": "Missing CLI tool name"});
                }

                let cli_args = match args.get("arguments") {
                    Some(Value::String(raw)) => json!({ "args": raw }),
                    Some(Value::Object(map)) => Value::Object(map.clone()),
                    Some(other) if !other.is_null() => json!({ "args": other.to_string() }),
                    _ => json!({}),
                };

                let candidates = [
                    requested_tool.to_string(),
                    requested_tool.replace("tizenclaw-", ""),
                ];

                for candidate in candidates {
                    let result = match self
                        .tool_dispatcher
                        .read()
                        .await
                        .execute(&candidate, &cli_args, None)
                        .await
                    {
                        Ok(value) => value,
                        Err(error) => json!({ "error": error }),
                    };
                    let is_unknown = result
                        .get("error")
                        .and_then(|value| value.as_str())
                        .map(|value| value.contains("Unknown tool:"))
                        .unwrap_or(false);
                    if !is_unknown {
                        return result;
                    }
                }

                json!({"error": format!("Unknown CLI tool: {}", requested_tool)})
            }
            "generate_web_app" => self.generate_web_app(args).await,
            "file_manager" => {
                let requested_session_id = args
                    .get("session_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("ipc-bridge");
                let session_workdir = if let Ok(store_guard) = self.session_store.lock() {
                    if let Some(store) = store_guard.as_ref() {
                        store.ensure_session(requested_session_id);
                        store.session_workdir(requested_session_id)
                    } else {
                        let fallback = self
                            .platform
                            .paths
                            .data_dir
                            .join("bridge_tool")
                            .join(requested_session_id);
                        let _ = std::fs::create_dir_all(&fallback);
                        fallback
                    }
                } else {
                    let fallback = self
                        .platform
                        .paths
                        .data_dir
                        .join("bridge_tool")
                        .join(requested_session_id);
                    let _ = std::fs::create_dir_all(&fallback);
                    fallback
                };
                file_manager_tool(args, &session_workdir).await
            }
            "validate_web_search" => {
                let engine = args.get("engine").and_then(|value| value.as_str());
                feature_tools::validate_web_search(&self.platform.paths.config_dir, engine)
            }
            "web_search" => {
                let query = args
                    .get("query")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let engine = args.get("engine").and_then(|value| value.as_str());
                let limit = args
                    .get("limit")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize)
                    .unwrap_or(5);
                feature_tools::web_search(
                    query,
                    engine,
                    limit,
                    &self.platform.paths.data_dir,
                    &self.platform.paths.config_dir,
                )
                .await
            }
            "remember" => {
                let key = args
                    .get("key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let value = args
                    .get("value")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let category = args
                    .get("category")
                    .and_then(|value| value.as_str())
                    .unwrap_or("general");
                match self.memory_store.lock() {
                    Ok(store_guard) => {
                        if let Some(store) = store_guard.as_ref() {
                            if key.is_empty() || value.is_empty() {
                                json!({"error": "Missing key or value"})
                            } else {
                                store.set(key, value, category);
                                json!({"status": "success", "message": format!("Remembered '{}'", key)})
                            }
                        } else {
                            json!({"error": "MemoryStore not initialized"})
                        }
                    }
                    Err(_) => json!({"error": "MemoryStore lock failed"}),
                }
            }
            "recall" => {
                let key = args
                    .get("key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                match self.memory_store.lock() {
                    Ok(store_guard) => {
                        if let Some(store) = store_guard.as_ref() {
                            if let Some(value) = store.get(key) {
                                json!({"status": "success", "value": value})
                            } else {
                                json!({"error": "Key not found"})
                            }
                        } else {
                            json!({"error": "MemoryStore not initialized"})
                        }
                    }
                    Err(_) => json!({"error": "MemoryStore lock failed"}),
                }
            }
            "forget" => {
                let key = args
                    .get("key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                match self.memory_store.lock() {
                    Ok(store_guard) => {
                        if let Some(store) = store_guard.as_ref() {
                            if store.delete(key) {
                                json!({"status": "success", "message": format!("Forgot '{}'", key)})
                            } else {
                                json!({"error": "Key not found"})
                            }
                        } else {
                            json!({"error": "MemoryStore not initialized"})
                        }
                    }
                    Err(_) => json!({"error": "MemoryStore lock failed"}),
                }
            }
            _ if tool_name.starts_with("action_") => match self.action_bridge.lock() {
                Ok(bridge) => {
                    let action_id = tool_name.strip_prefix("action_").unwrap_or(tool_name);
                    bridge.execute_action(action_id, args)
                }
                Err(_) => json!({"error": "Failed to lock action bridge"}),
            },
            _ => {
                match self
                    .tool_dispatcher
                    .read()
                    .await
                    .execute_in_dir(tool_name, args, None, Some(&self.platform.paths.data_dir))
                    .await
                {
                    Ok(value) => value,
                    Err(error) => json!({ "error": error }),
                }
            }
        }
    }

    fn build_session_profile(
        &self,
        role_name: Option<&str>,
        system_prompt: Option<&str>,
        prompt_mode: Option<PromptMode>,
        reasoning_policy: Option<ReasoningPolicy>,
        allowed_tools: Option<Vec<String>>,
        description: Option<&str>,
    ) -> Result<SessionPromptProfile, String> {
        let mut profile = SessionPromptProfile::default();

        if let Some(role_name) = role_name.filter(|name| !name.trim().is_empty()) {
            let role = self
                .agent_roles
                .read()
                .ok()
                .and_then(|registry| registry.get_role(role_name).cloned())
                .ok_or_else(|| format!("Unknown agent role '{}'", role_name))?;
            profile.role_name = Some(role.name.clone());
            profile.role_description = Some(role.description.clone());
            if !role.system_prompt.trim().is_empty() {
                profile.system_prompt = Some(role.system_prompt);
            }
            if !role.allowed_tools.is_empty() {
                profile.allowed_tools = Some(role.allowed_tools);
            }
            profile.max_iterations = Some(role.max_iterations);
            profile.role_type = Some(role.role_type);
            if !role.can_delegate_to.is_empty() {
                profile.can_delegate_to = Some(role.can_delegate_to);
            }
            profile.prompt_mode = role.prompt_mode;
            profile.reasoning_policy = role.reasoning_policy;
        }

        if let Some(system_prompt) = system_prompt.filter(|value| !value.trim().is_empty()) {
            profile.system_prompt = Some(system_prompt.trim().to_string());
        }
        if let Some(prompt_mode) = prompt_mode {
            profile.prompt_mode = Some(prompt_mode);
        }
        if let Some(reasoning_policy) = reasoning_policy {
            profile.reasoning_policy = Some(reasoning_policy);
        }
        if let Some(allowed_tools) = allowed_tools.filter(|items| !items.is_empty()) {
            profile.allowed_tools = Some(allowed_tools);
        }
        if let Some(description) = description.filter(|value| !value.trim().is_empty()) {
            profile.role_description = Some(description.trim().to_string());
        }

        Ok(profile)
    }

    pub async fn initialize(&self) -> bool {
        log::debug!("AgentCore initializing...");
        let paths = &self.platform.paths;
        let _ = self.event_bus.start();

        let policy_path = paths.config_dir.join("tool_policy.json");
        if let Ok(mut tp) = self.tool_policy.lock() {
            tp.load_config(&policy_path.to_string_lossy());
        }
        self.reload_safety_guard();

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

        if let Ok(mut roles) = self.agent_roles.write() {
            roles.ensure_builtin_roles();
            let role_path = self.role_file_path();
            let _ = roles.load_roles(&role_path.to_string_lossy());
        }

        // Load LLM config (supports multi-backend + fallback)
        let llm_config_path = paths.config_dir.join("llm_config.json");
        let config = LlmConfig::load(&llm_config_path.to_string_lossy());
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
                merge_backend_auth(base, &cand.name, &ks_guard)
            };

            if let Some(be) =
                Self::create_and_init_backend_static(&plugin_manager, &cand.name, merged_cfg)
            {
                if !primary_initialized {
                    log::info!(
                        "Primary LLM backend '{}' initialized (priority {})",
                        cand.name,
                        cand.priority
                    );
                    *self.backend.write().await = Some(be);
                    if let Ok(mut bn) = self.backend_name.write() {
                        *bn = cand.name.clone();
                    }
                    primary_initialized = true;
                } else if fallback_names.contains(&cand.name) {
                    log::info!(
                        "Fallback LLM backend '{}' initialized (priority {})",
                        cand.name,
                        cand.priority
                    );
                    fallbacks.push(be);
                } else {
                    log::debug!(
                        "Backend '{}' initialized but not in fallback_backends, skipping as fallback",
                        cand.name
                    );
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
        match SessionStore::new(&paths.app_data_dir(), &db_path.to_string_lossy()) {
            Ok(store) => {
                log::info!("Session store initialized");
                if let Ok(mut ss) = self.session_store.lock() {
                    *ss = Some(store);
                }
            }
            Err(e) => log::error!("Session store failed: {}", e),
        }

        // Initialize memory store
        let mem_dir = paths.data_dir.join("memory");
        let mem_db = mem_dir.join("memories.db");
        let model_dir = paths.data_dir.join("models");
        match crate::storage::memory_store::MemoryStore::new(
            &mem_dir.to_string_lossy(),
            &mem_db.to_string_lossy(),
            &model_dir.to_string_lossy(),
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
            let tool_roots = collect_tool_roots(paths);
            td.load_tools_from_paths(tool_roots.iter().map(|root| root.as_str()));
        }
        log::info!("Tools loaded from {:?}", collect_tool_roots(paths));

        // Load workflows
        {
            let mut we = self.workflow_engine.write().await;
            we.load_workflows_from(&paths.workflows_dir.to_string_lossy());
        }

        {
            let mut bridge = self.action_bridge.lock().unwrap();
            bridge.start();
        }

        self.publish_runtime_event(
            "initialize",
            json!({
                "primary_backend": self.get_llm_runtime()["runtime_primary_backend"],
                "tool_roots": collect_tool_roots(paths),
            }),
        );

        true
    }

    /// Dynamically handle package manager events for plugins
    pub async fn handle_pkgmgr_event(&self, event_name: &str, pkgid: &str) {
        log::debug!("Handling pkgmgr event: {} for pkgid: {}", event_name, pkgid);

        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        let loaded = if event_name == "install"
            || event_name == "recoverinstall"
            || event_name == "upgrade"
            || event_name == "recoverupgrade"
        {
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
        self.reload_safety_guard();

        // Re-scan plugins
        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        plugin_manager.scan_plugins(Some(self.platform.package_manager.as_ref()));

        // Unified priority-based selection
        let candidates = self.get_backend_candidates(&config, &plugin_manager);

        let mut primary_initialized = false;
        let mut fallbacks = Vec::new();

        for cand in candidates {
            // Acquire KeyStore briefly — merge api_key, then drop guard.
            let merged_cfg = {
                let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
                let base = config.backend_config(&cand.name);
                merge_backend_auth(base, &cand.name, &ks_guard)
            };

            if let Some(be) =
                Self::create_and_init_backend_static(&plugin_manager, &cand.name, merged_cfg)
            {
                if !primary_initialized {
                    log::debug!(
                        "Dynamically swapped Primary LLM backend to '{}' (priority {})",
                        cand.name,
                        cand.priority
                    );
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

        if let Ok(mut stored_config) = self.llm_config.lock() {
            *stored_config = config;
        }

        self.publish_runtime_event("reload_backends", self.get_llm_runtime());
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
            log::debug!(
                "Backend '{}' skipped: not configured or initialization failed",
                name
            );
            None
        }
    }

    /// Determine LLM backend candidates and their priorities.
    fn get_backend_candidates(
        &self,
        config: &LlmConfig,
        plugin_manager: &crate::llm::plugin_manager::PluginManager,
    ) -> Vec<BackendCandidate> {
        let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
        build_backend_candidates(config, plugin_manager, &ks_guard)
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
        let state = cb_guard
            .entry(name.to_string())
            .or_insert(CircuitBreakerState {
                consecutive_failures: 0,
                last_failure_time: None,
            });
        state.consecutive_failures = 0;
        state.last_failure_time = None;
    }

    /// Reset all circuit breakers. Called at the start of each new session
    /// so that failures from a prior session do not block new requests.
    fn reset_circuit_breakers(&self) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        cb_guard.clear();
    }

    fn record_failure(&self, name: &str) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        let state = cb_guard
            .entry(name.to_string())
            .or_insert(CircuitBreakerState {
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
        let mut failure_summaries: Vec<String> = Vec::new();

        // Try primary backend — lock is held only during chat()
        {
            let bn = match self.backend_name.read() {
                Ok(guard) => (*guard).clone(),
                Err(p) => (*p.into_inner()).clone(),
            };

            if self.is_backend_available(&bn) {
                let be_guard = self.backend.read().await;
                if let Some(be) = be_guard.as_ref() {
                    let resp = be
                        .chat(messages, tools, on_chunk, system_prompt, max_tokens)
                        .await;
                    if resp.success {
                        self.record_success(&bn);
                        return resp;
                    }
                    self.record_failure(&bn);
                    log::warn!(
                        "Primary backend '{}' failed (HTTP {}): {}",
                        bn,
                        resp.http_status,
                        resp.error_message
                    );
                    failure_summaries.push(format!(
                        "{} (HTTP {}): {}",
                        bn, resp.http_status, resp.error_message
                    ));
                }
            } else {
                log::warn!("Primary backend '{}' skipped due to Circuit Breaker", bn);
                failure_summaries.push(format!("{}: skipped by circuit breaker", bn));
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
                    let resp = fb
                        .chat(messages, tools, on_chunk, system_prompt, max_tokens)
                        .await;
                    if resp.success {
                        self.record_success(&bn);
                        return resp;
                    }
                    self.record_failure(&bn);
                    log::warn!("Fallback '{}' also failed: {}", bn, resp.error_message);
                    failure_summaries.push(format!(
                        "{} (HTTP {}): {}",
                        bn, resp.http_status, resp.error_message
                    ));
                } else {
                    log::warn!("Fallback backend '{}' skipped due to Circuit Breaker", bn);
                    failure_summaries.push(format!("{}: skipped by circuit breaker", bn));
                }
            }
        }

        let error_message = if failure_summaries.is_empty() {
            "All LLM backends failed".to_string()
        } else {
            format!(
                "All LLM backends failed. Last attempts: {}",
                failure_summaries.join(" | ")
            )
        };

        LlmResponse {
            error_message,
            ..Default::default()
        }
    }

    /// Extract intent keywords for dynamic tool filtering.
    fn extract_intent_keywords(prompt: &str) -> Vec<String> {
        let p = prompt.to_lowercase();
        let mut keywords = Vec::new();

        if p.contains("file") || p.contains("read") || p.contains("cat") {
            keywords.extend(["fs", "file", "read", "write", "content"].map(String::from));
        }
        if p.contains("install") || p.contains("package") || p.contains("app") || p.contains("run")
        {
            keywords.extend(["pkg", "app", "install", "exec", "shell", "run"].map(String::from));
        }
        if p.contains("remember")
            || p.contains("memory")
            || p.contains("search")
            || p.contains("knowledge")
            || p.contains("recall")
        {
            keywords.extend(
                ["mem", "remember", "forget", "recall", "search", "know"].map(String::from),
            );
        }
        if p.contains("task") || p.contains("schedule") || p.contains("alarm") || p.contains("time")
        {
            keywords.extend(["task", "sched", "alarm", "time", "date"].map(String::from));
        }
        if p.contains("system")
            || p.contains("info")
            || p.contains("status")
            || p.contains("battery")
            || p.contains("wifi")
        {
            keywords.extend(
                [
                    "sys", "info", "status", "battery", "wifi", "network", "device",
                ]
                .map(String::from),
            );
        }
        if p.contains("help") || p.contains("list") {
            keywords.extend(["ALL"].map(String::from));
        }

        keywords
    }

    fn is_web_dashboard_app_request(session_id: &str, prompt: &str) -> bool {
        if !session_id.starts_with("web_") {
            return false;
        }

        let p = prompt.to_lowercase();
        let asks_to_create = [
            "create", "make", "build", "generate", "write", "update", "improve", "enhance",
            "modify", "edit",
        ]
        .iter()
        .any(|needle| p.contains(needle));
        let mentions_browser_ui = [
            "web app",
            "browser app",
            "browser",
            "html",
            "css",
            "javascript",
            "js",
            "webview",
            "dashboard",
            "ui",
            "interface",
            "screen",
            "panel",
            "visualization",
            "chart",
            "monitor",
            "tetris",
            "game",
            "page",
        ]
        .iter()
        .any(|needle| p.contains(needle));

        asks_to_create && mentions_browser_ui
    }

    /// Process a user prompt through the 15-phase autonomous agent loop.
    ///
    /// ## Loop Phases
    /// 1. GoalParsing: Initialize AgentLoopState for this session + prompt
    /// 2. ContextLoading: Load session history, build messages + tools
    /// 3. Pre-loop Compaction: Compact if ≥90% of 256k token budget
    ///    4-13. Main loop: DecisionMaking → SafetyCheck → ToolDispatching
    ///    → ObservationCollect → Evaluating → ErrorRecovery
    ///    → StateTracking → SelfInspection → RePlanning → TerminationCheck
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
        // Reset circuit breakers at the start of each session so failures
        // from a prior session do not cascade into new requests.
        self.reset_circuit_breakers();
        let mut loop_state = AgentLoopState::new(session_id, prompt);
        let mut skip_memory_extraction = false;

        // Load context token budget from config if available
        let (budget, threshold) = {
            let cfg = self.llm_config.lock().ok();
            let b = cfg
                .as_ref()
                .and_then(|c| c.backends.get("context_token_budget"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(CONTEXT_TOKEN_BUDGET);
            let t = cfg
                .as_ref()
                .and_then(|c| c.backends.get("context_compact_threshold"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(CONTEXT_COMPACT_THRESHOLD);
            (b, t)
        };
        loop_state.token_budget = budget;
        loop_state.compact_threshold = threshold;
        self.persist_loop_snapshot(&loop_state);

        log::debug!(
            "[AgentLoop] Phase=GoalParsing session='{}' goal='{}' budget={}",
            session_id,
            utf8_safe_preview(prompt, 80),
            budget
        );

        // Quick check: do we have any backend?
        {
            let has_primary = self.backend.read().await.is_some();
            let has_fallback = !self.fallback_backends.read().await.is_empty();
            if !has_primary && !has_fallback {
                loop_state.last_error = Some("No LLM backend configured".into());
                loop_state.mark_terminal(
                    LoopTransitionReason::NoBackendConfigured,
                    "no primary or fallback backend is configured",
                );
                self.persist_loop_snapshot(&loop_state);
                return "Error: No LLM backend configured".into();
            }
        }

        // ── Phase 2: ContextLoading ──────────────────────────────────────
        loop_state.transition(AgentPhase::ContextLoading);

        log_conversation("User", prompt);

        let session_workdir = if let Ok(ss) = self.session_store.lock() {
            ss.as_ref()
                .map(|store| store.session_workdir(session_id))
                .unwrap_or_else(|| self.platform.paths.data_dir.clone())
        } else {
            self.platform.paths.data_dir.clone()
        };

        // Store user message
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                store.add_message(session_id, "user", prompt);
                store.add_structured_user_message(session_id, prompt);
            }
        }

        // Build conversation history — compaction-aware load
        let history = {
            let ss = self.session_store.lock();
            if let Some(Ok(Some((msgs, from_compact)))) = ss.ok().map(|s| {
                // Returns (Vec<SessionMessage>, bool)
                Ok::<_, ()>(
                    s.as_ref()
                        .map(|store| store.load_session_context(session_id, MAX_CONTEXT_MESSAGES)),
                )
            }) {
                if from_compact {
                    log::info!(
                        "[ContextLoading] session='{}' loaded from compacted.md",
                        session_id
                    );
                } else {
                    log::info!(
                        "[ContextLoading] session='{}' loaded {} msgs from history",
                        session_id,
                        msgs.len()
                    );
                }
                msgs
            } else {
                vec![]
            }
        };

        let mut messages: Vec<LlmMessage> = history
            .iter()
            .cloned()
            .map(|m| m.into_llm_message())
            .filter_map(sanitize_message_for_transport)
            .collect();

        if messages.is_empty() || messages.last().map(|m| m.role.as_str()) != Some("user") {
            messages.push(LlmMessage::user(prompt));
        }
        if let Err(err) = self.check_context_message_limit(session_id, &messages, &mut loop_state) {
            return format!("Error: {}", err);
        }

        // Extract intent keywords for optimal tool injection
        let intent_keywords = Self::extract_intent_keywords(prompt);

        let registrations = self.list_registered_paths();
        let skill_capabilities =
            skill_capability_manager::load_snapshot(&self.platform.paths, &registrations);
        let skill_roots = skill_capabilities
            .roots
            .iter()
            .map(|root| root.path.clone())
            .collect::<Vec<_>>();
        let textual_skills = skill_capabilities.enabled_skills();
        let is_dashboard_web_app_request = Self::is_web_dashboard_app_request(session_id, prompt);
        let is_file_management_request = is_simple_file_management_request(prompt);
        let mut session_profile = self.resolve_session_profile(session_id);
        if session_profile.is_none() && is_dashboard_web_app_request {
            session_profile = Some(SessionPromptProfile {
                system_prompt: Some(
                    "For browser-based apps in dashboard web sessions, use only the \
                     generate_web_app tool. Do not write raw HTML files into the \
                     session workdir, do not use run_generated_code for HTML, and do \
                     not open file:// or local workdir paths."
                        .to_string(),
                ),
                allowed_tools: Some(vec!["generate_web_app".to_string()]),
                ..SessionPromptProfile::default()
            });
        }
        if session_profile.is_none() && is_file_management_request {
            session_profile = Some(SessionPromptProfile {
                role_name: Some("file_manager_flow".to_string()),
                role_description: Some(
                    "Direct file management profile for normal file and directory operations."
                        .to_string(),
                ),
                system_prompt: Some(
                    "For normal file and directory tasks, manage files directly with file_manager \
                     or file_write. Create directories explicitly, write the requested files \
                     into the working directory, and avoid run_generated_code unless the user \
                     explicitly asks for an executable script to be generated and run."
                        .to_string(),
                ),
                allowed_tools: Some(vec!["file_manager".to_string(), "file_write".to_string()]),
                max_iterations: Some(0),
                ..SessionPromptProfile::default()
            });
        }
        if let Some(max_iterations) = session_profile
            .as_ref()
            .and_then(|profile| profile.max_iterations)
        {
            loop_state.max_tool_rounds = max_iterations;
        }
        let skill_reference_docs =
            crate::core::skill_support::list_skill_reference_docs(&self.platform.paths.docs_dir);
        let prefetched_skills =
            select_relevant_skills(prompt, &textual_skills, MAX_PREFETCHED_SKILLS);
        for skill in &prefetched_skills {
            log::info!(
                "[SkillAudit] skill='{}' shell_prelude={} code_fence_languages={:?} prelude_excerpt='{}'",
                skill.file_name,
                skill.shell_prelude,
                skill.code_fence_languages,
                utf8_safe_preview(&skill.prelude_excerpt, 160),
            );
        }
        loop_state.record_prefetch_skills(
            prefetched_skills
                .iter()
                .map(|skill| skill.file_name.clone())
                .collect(),
        );
        let skill_context = build_skill_prefetch_message(&prefetched_skills);
        if let Some(skill_context) = skill_context.as_ref() {
            inject_context_message(&mut messages, skill_context.clone());
        }

        // Get tool declarations
        let mut tools = self
            .tool_dispatcher
            .read()
            .await
            .get_tool_declarations_filtered(&intent_keywords);
        crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_all_builtin_tools(
            &mut tools,
        );
        if is_dashboard_web_app_request && !tools.iter().any(|tool| tool.name == "generate_web_app")
        {
            tools.push(crate::llm::backend::LlmToolDecl {
                name: "generate_web_app".into(),
                description: "Generate or update a web application served by the web dashboard at /apps/<app_id>/. Supports HTML/CSS/JS files, optional asset downloads, bridge tool allowlists, and best-effort bridge or webview launch on Tizen.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "app_id": {
                            "type": "string",
                            "description": "Unique identifier for the app (lowercase alphanumeric + underscore, max 64 chars)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Display title for the web app"
                        },
                        "html": {
                            "type": "string",
                            "description": "Complete HTML content. Can be a single-file app or reference style.css and app.js"
                        },
                        "css": {
                            "type": "string",
                            "description": "Optional separate CSS stylesheet saved as style.css"
                        },
                        "js": {
                            "type": "string",
                            "description": "Optional separate JavaScript code saved as app.js"
                        },
                        "assets": {
                            "type": "array",
                            "description": "Optional external assets to download. Each item is {url, filename}. Max 10MB per file.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string", "description": "Asset download URL"},
                                    "filename": {"type": "string", "description": "Local filename such as logo.png"}
                                }
                            }
                        },
                        "allowed_tools": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Optional tool names this app may call via the bridge API"
                        }
                    },
                    "required": ["app_id", "title", "html"]
                }),
            });
        }
        if let Ok(bridge) = self.action_bridge.lock() {
            tools.extend(bridge.get_action_declarations());
        }
        if let Some(allowed_tools) = session_profile
            .as_ref()
            .and_then(|profile| profile.allowed_tools.as_ref())
        {
            tools.retain(|tool| allowed_tools.iter().any(|name| name == &tool.name));
        }

        let restrict_to_generate_web_app = is_dashboard_web_app_request
            && session_profile
                .as_ref()
                .and_then(|profile| profile.allowed_tools.as_ref())
                .map(|tools| tools.len() == 1 && tools[0] == "generate_web_app")
                .unwrap_or(false);
        if !restrict_to_generate_web_app {
            // Add search_tools meta-tool for Two-Tier router
            tools.push(crate::llm::backend::LlmToolDecl {
                name: "search_tools".into(),
                description: "Search available tools across all categories or within a specific category. Use this whenever the required capability is not already present in context.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Keyword to search tools, or 'ALL'."}
                    },
                    "required": ["query"]
                })
            });
        }

        // Build System Prompt
        let (system_prompt, dynamic_context) = {
            let prompt_doc = llm_config_store::load(&self.platform.paths.config_dir)
                .unwrap_or_else(|_| llm_config_store::default_document());
            let mut builder = crate::core::prompt_builder::SystemPromptBuilder::new()
                .add_available_tools(tools.clone()); // XML Inject
            if let Some(role_prompt) = session_profile
                .as_ref()
                .and_then(|profile| profile.system_prompt.clone())
            {
                builder = builder.set_base_prompt(role_prompt);
            } else if let Ok(base) = self.system_prompt.read() {
                builder = builder.set_base_prompt(base.clone());
            }
            if let Ok(soul_lock) = self.soul_content.read() {
                if let Some(ref soul) = *soul_lock {
                    builder = builder.set_soul_content(soul.clone());
                }
            }

            let formatted_skills = prefetched_skills
                .into_iter()
                .map(|s| {
                    let summary = format_skill_summary(&s);
                    (s.absolute_path, summary)
                })
                .collect();
            builder = builder.add_available_skills(formatted_skills);
            let formatted_skill_references = skill_reference_docs
                .iter()
                .map(|doc| (doc.absolute_path.clone(), doc.description.clone()))
                .collect();
            builder = builder.add_available_skill_references(formatted_skill_references);

            let model_name = {
                let bn = self.backend_name.read().unwrap_or_else(|e| e.into_inner());
                (*bn).clone()
            };
            builder = builder
                .set_prompt_mode(
                    session_profile
                        .as_ref()
                        .and_then(|profile| profile.prompt_mode)
                        .unwrap_or_else(|| prompt_mode_from_doc(&prompt_doc, &model_name)),
                )
                .set_reasoning_policy(
                    session_profile
                        .as_ref()
                        .and_then(|profile| profile.reasoning_policy)
                        .unwrap_or_else(|| reasoning_policy_from_doc(&prompt_doc, &model_name)),
                );
            let platform_name = self.platform.platform_name().to_string();
            let data_dir = session_workdir.to_string_lossy().to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| {
                    let secs = d.as_secs();
                    format!("UNIX:{}", secs) // Simple enough for now
                })
                .unwrap_or_else(|_| "unknown".into());
            builder = builder.set_runtime_context(platform_name, model_name, data_dir, now);

            let dynamic_context = builder.build_dynamic_context();
            let system_prompt = builder.build();
            (system_prompt, dynamic_context)
        };

        if let Some(dynamic_context) = dynamic_context.as_ref() {
            inject_context_message(&mut messages, dynamic_context.clone());
        }
        if let Some(profile) = session_profile.as_ref() {
            let role_name = profile.role_name.as_deref().unwrap_or("custom");
            let description = profile
                .role_description
                .as_deref()
                .unwrap_or("No role description provided.");
            inject_context_message(
                &mut messages,
                format!(
                    "## Active Role Profile\nRole: {}\nDescription: {}",
                    role_name, description
                ),
            );
        }
        inject_context_message(
            &mut messages,
            format!(
                "## Working Directory\nUse '{}' as the primary working directory for file reads, file writes, generated scripts, and task artifacts unless the user explicitly gives a different absolute path.",
                session_workdir.to_string_lossy()
            ),
        );
        let explicit_prompt_paths = extract_explicit_file_paths(prompt);
        let explicit_output_dirs = extract_explicit_directory_paths(prompt);
        if !explicit_prompt_paths.is_empty() {
            for prefetched_message in build_prefetched_prompt_file_messages(&explicit_prompt_paths)
            {
                if let Some(last_user_idx) =
                    messages.iter().rposition(|message| message.role == "user")
                {
                    messages.insert(last_user_idx, prefetched_message);
                } else {
                    messages.push(prefetched_message);
                }
            }
            inject_context_message(
                &mut messages,
                format!(
                    "## File Grounding\nThe user explicitly referenced these real input files:\n{}\nInspect the relevant files before generating or executing code. Base every answer and script on the real file contents. Do not invent substitute datasets, placeholder paths, or mock values.",
                    explicit_prompt_paths
                        .iter()
                        .map(|path| format!("- {}", path))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            );
            if let Some(prefetched_context) =
                build_prefetched_prompt_file_context(&explicit_prompt_paths)
            {
                inject_context_message(&mut messages, prefetched_context);
            }
            if let Some(problem_requirements_context) =
                build_authoritative_problem_requirements_context(&explicit_prompt_paths)
            {
                inject_context_message(&mut messages, problem_requirements_context);
            }
        }
        if prompt_requires_persisted_level_scripts(prompt) && !explicit_output_dirs.is_empty() {
            inject_context_message(
                &mut messages,
                format!(
                    "## Output Contract\nThe user requested persisted script files under these output directories:\n{}\nFor each run_generated_code call, generate exactly one level script. Put the exact absolute target file path in the first comment line, use the filename format 'level-N-solution.py', and do not paste fenced code or prose instead of calling run_generated_code.",
                    explicit_output_dirs
                        .iter()
                        .map(|path| format!("- {}", path))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            );
        }

        // Load long term memory dynamically and inject into messages (preserves system_prompt cache)
        let mut memory_context_for_log: Option<String> = None;
        if should_skip_memory_for_prompt(prompt) {
            loop_state.record_prefetch_memory(None);
        } else if let Ok(ms) = self.memory_store.lock() {
            if let Some(store) = ms.as_ref() {
                let mem_str = store.load_relevant_for_prompt(prompt, 5, 0.1);
                if !mem_str.is_empty() {
                    let memory_context = format!(
                        "## Context from Long-Term Memory\n<long_term_memory>\n{}\n</long_term_memory>",
                        mem_str
                    );
                    loop_state
                        .record_prefetch_memory(Some(utf8_safe_preview(&mem_str, 240).to_string()));
                    inject_context_message(&mut messages, memory_context.clone());
                    memory_context_for_log = Some(memory_context);
                } else {
                    loop_state.record_prefetch_memory(None);
                }
            }
        }

        messages = sanitize_messages_for_transport(messages);

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
                    cached_hash,
                    new_hash
                );
                let be_guard = self.backend.read().await;
                if let Some(be) = be_guard.as_ref() {
                    let cached = be.prepare_cache(&system_prompt).await;
                    if cached {
                        log::info!(
                            "[PromptCache] Cache ready — subsequent rounds will reference cached content"
                        );
                    } else {
                        log::debug!(
                            "[PromptCache] Backend does not support caching or prompt too short; using inline system_instruction"
                        );
                    }
                }
                drop(be_guard);
                // Always update the stored hash so we do not retry on every call
                *self.prompt_hash.write().await = new_hash;
            } else {
                log::debug!(
                    "[PromptCache] Prompt unchanged (hash={}), reusing cached content",
                    cached_hash
                );
            }
        }

        // ── Phase 3: Planning (Cognitive Plan-and-Solve & compaction) ────
        loop_state.transition(AgentPhase::Planning);
        let context_engine = SizedContextEngine::new().with_threshold(loop_state.compact_threshold);

        log_payload_breakdown(
            session_id,
            prompt,
            &history,
            skill_context.as_deref(),
            dynamic_context.as_deref(),
            memory_context_for_log.as_deref(),
            &system_prompt,
            &tools,
            &messages,
            &context_engine,
        );

        let mut matched_workflow_id = None;
        {
            let we = self.workflow_engine.read().await;
            for wf_val in we.list_workflows() {
                if let (Some(w_id), Some(trigger)) = (
                    wf_val.get("id").and_then(|v| v.as_str()),
                    wf_val.get("trigger").and_then(|v| v.as_str()),
                ) {
                    if trigger != "manual" && (prompt.contains(trigger) || trigger == prompt) {
                        matched_workflow_id = Some(w_id.to_string());
                        break;
                    }
                }
            }
        }

        if let Some(wf_id) = matched_workflow_id {
            log::info!(
                "[Planning] Matched workflow trigger '{}', entering Workflow Mode.",
                wf_id
            );
            loop_state.active_workflow_id = Some(wf_id);
        } else {
            // Optional LLM Cognitive Step for Complex Prompts
            if crate::core::intent_analyzer::IntentAnalyzer::is_complex_task(prompt) {
                log::debug!(
                    "[AgentLoop] Complex prompt detected. Triggering explicit Plan-and-Solve..."
                );
                let plan_sys = "You are a precise planner. Outline the distinct steps to solve the user's request. Output only a list of concise steps.";

                // Release writer locks safely for LLM call
                let plan_resp_opt = {
                    let be_guard = self.backend.read().await;
                    if let Some(be) = be_guard.as_ref() {
                        Some(
                            be.chat(
                                &sanitize_messages_for_transport(vec![LlmMessage::user(prompt)]),
                                &[],
                                None,
                                plan_sys,
                                Some(1024),
                            )
                            .await,
                        )
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
        if context_engine.should_compact(&messages, loop_state.token_budget)
            || loop_state.needs_compaction()
        {
            log::debug!(
                "[AgentLoop] Pre-loop compaction triggered ({}% used)",
                (loop_state.token_used as f32 / loop_state.token_budget as f32 * 100.0) as u32
            );
            messages = context_engine.compact(messages, loop_state.token_budget);
            loop_state.token_used = context_engine.estimate_tokens(&messages);
            self.persist_compacted_messages(session_id, &messages);
        }

        // ── Phases 4–13: Main agentic loop ───────────────────────────────
        loop {
            // ── Phase 4: DecisionMaking / LLM call ──────────────────────
            loop_state.transition(AgentPhase::DecisionMaking);
            log::debug!(
                "[AgentLoop] Round {} | session='{}' phase=DecisionMaking msgs={}",
                loop_state.round,
                session_id,
                messages.len()
            );

            log::debug!(
                "[AgentLoop] Round {} dispatching {} transport messages with {} tools",
                loop_state.round,
                messages.len(),
                tools.len()
            );

            let mut response = LlmResponse::default();
            let mut is_workflow_tool = false;

            if let Some(wf_id) = loop_state.active_workflow_id.clone() {
                let we = self.workflow_engine.read().await;
                if let Some(wf) = we.get_workflow(&wf_id) {
                    if loop_state.current_workflow_step >= wf.steps.len() {
                        log::info!("[Workflow] All steps completed for {}", wf.name);
                        loop_state.active_workflow_id = None;
                        loop_state.transition(AgentPhase::ResultReporting);
                        let text = format!(
                            "Workflow '{}' completed successfully.\nVariables:\n{:?}",
                            wf.name,
                            loop_state.workflow_vars.keys().collect::<Vec<_>>()
                        );
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
                            if crate::core::workflow_engine::WorkflowEngine::eval_condition(
                                &step.condition,
                                &loop_state.workflow_vars,
                            ) {
                                log::debug!(
                                    "Condition evaluated to TRUE. Branching to '{}'",
                                    step.then_step
                                );
                                loop_state.current_workflow_step += 1;
                            } else {
                                log::debug!(
                                    "Condition evaluated to FALSE. Branching to '{}'",
                                    step.else_step
                                );
                                loop_state.current_workflow_step += 1;
                            }
                            continue;
                        }
                        WorkflowStepType::Tool => {
                            let resolved_args =
                                crate::core::workflow_engine::WorkflowEngine::interpolate_json(
                                    &step.args,
                                    &loop_state.workflow_vars,
                                );
                            response.success = true;
                            // Add randomness so observe_output Doesn't see identical strings and trigger Stuck
                            response.text = format!(
                                "Executing workflow tool '{}' (Round {})",
                                step.tool_name, loop_state.round
                            );
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
                            let already_injected =
                                messages.iter().any(|m| m.text.contains(&step_marker));

                            if !already_injected {
                                let resolved_instruction =
                                    crate::core::workflow_engine::WorkflowEngine::interpolate(
                                        &step.instruction,
                                        &loop_state.workflow_vars,
                                    );
                                messages.push(LlmMessage {
                                    role: "system".into(),
                                    text: format!("{}\n{}", step_marker, resolved_instruction),
                                    ..Default::default()
                                });
                            }
                            response = self
                                .chat_with_fallback(
                                    &sanitize_messages_for_transport(messages.clone()),
                                    &tools,
                                    on_chunk,
                                    &system_prompt,
                                    None,
                                )
                                .await;
                        }
                    }
                }
            } else {
                response = self
                    .chat_with_fallback(
                        &sanitize_messages_for_transport(messages.clone()),
                        &tools,
                        on_chunk,
                        &system_prompt,
                        None,
                    )
                    .await;
            }

            // ── Phase 6: ObservationCollect ──────────────────────────────
            loop_state.transition(AgentPhase::ObservationCollect);
            log::debug!(
                "[AgentLoop] Round {} Response: success={} text_len={}",
                loop_state.round,
                response.success,
                response.text.len()
            );

            // ── Phase 11: SafetyCheck — handle LLM error ─────────────────
            if !response.success {
                loop_state.transition(AgentPhase::ErrorRecovery);
                loop_state.error_count += 1;
                let err = format!(
                    "LLM error (HTTP {}): {}",
                    response.http_status, response.error_message
                );
                loop_state.last_error = Some(err.clone());
                log::error!("[AgentLoop] {}", err);
                // `chat_with_fallback()` already consumes the per-turn recovery
                // budget by trying the primary backend plus configured
                // fallbacks. Align with OpenClaw/HermesAgentLoop style and
                // surface the failure here instead of replaying the same turn
                // multiple times with a fixed hardcoded retry count.
                loop_state.transition(AgentPhase::ResultReporting);
                self.persist_loop_snapshot(&loop_state);
                return err;
            }

            // Extract reasoning
            let mut reasoning_text = response.reasoning_text.clone();
            if reasoning_text.is_empty() {
                if let Some(cap) = THINK_RE.captures(&response.text) {
                    reasoning_text = cap[1].trim().to_string();
                }
            }

            // Fallback parser
            let mut detected_tool_calls = response.tool_calls.clone();
            if detected_tool_calls.is_empty() {
                detected_tool_calls = FallbackParser::parse(&response.text);
                if !detected_tool_calls.is_empty() {
                    log::debug!(
                        "[AgentLoop] FallbackParser detected {} tool call(s)",
                        detected_tool_calls.len()
                    );
                }
            }

            // Record token usage
            {
                let be_name = self
                    .backend
                    .read()
                    .await
                    .as_ref()
                    .map(|be| be.get_name().to_string())
                    .unwrap_or_else(|| "unknown".into());
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.record_usage(
                            session_id,
                            response.prompt_tokens,
                            response.completion_tokens,
                            response.cache_creation_input_tokens,
                            response.cache_read_input_tokens,
                            &be_name,
                        );
                        let usage = store.load_token_usage(session_id);
                        log::debug!(
                            "[TokenUsage] Round: P{}+C{}={} | Cache write/read: {}/{} | Session cumulative: {} | Session cache read: {}",
                            response.prompt_tokens,
                            response.completion_tokens,
                            response.prompt_tokens + response.completion_tokens,
                            response.cache_creation_input_tokens,
                            response.cache_read_input_tokens,
                            usage.total_prompt_tokens + usage.total_completion_tokens,
                            usage.total_cache_read_input_tokens
                        );
                        if response.cache_read_input_tokens > 0
                            || response.cache_creation_input_tokens > 0
                        {
                            log::info!(
                                "[TokenUsage] Cache telemetry for {}: write={} read={}",
                                be_name,
                                response.cache_creation_input_tokens,
                                response.cache_read_input_tokens
                            );
                        }
                        loop_state.token_used = usage.total_prompt_tokens as usize
                            + context_engine.estimate_tokens(&messages);
                    }
                }
            }

            if !detected_tool_calls.is_empty() {
                // ── Phase 5: ToolDispatching ─────────────────────────────
                loop_state.transition(AgentPhase::ToolDispatching);
                loop_state.total_tool_calls += detected_tool_calls.len();
                loop_state.mark_follow_up(
                    LoopTransitionReason::ToolCallsRequested,
                    format!(
                        "assistant requested {} tool call(s)",
                        detected_tool_calls.len()
                    ),
                );
                log::debug!(
                    "[AgentLoop] Round {} dispatching {} tool(s)",
                    loop_state.round,
                    detected_tool_calls.len()
                );

                // Enforce reasoning extraction if not provided by backend
                let final_text = extract_final_text(&response.text);

                // Add assistant message
                messages.push(LlmMessage {
                    role: "assistant".into(),
                    text: final_text.clone(),
                    reasoning_text: reasoning_text.clone(),
                    tool_calls: detected_tool_calls.clone(),
                    ..Default::default()
                });

                let canonical_tool_calls: Vec<Value> = detected_tool_calls
                    .iter()
                    .map(canonical_tool_trace)
                    .collect();
                let canonical_tool_names: HashMap<String, String> = detected_tool_calls
                    .iter()
                    .zip(canonical_tool_calls.iter())
                    .map(|(tc, trace)| {
                        (
                            tc.id.clone(),
                            trace["name"]
                                .as_str()
                                .unwrap_or(tc.name.as_str())
                                .to_string(),
                        )
                    })
                    .collect();
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        if !final_text.trim().is_empty() {
                            store.add_structured_assistant_text_message(session_id, &final_text);
                        }
                        store.add_structured_tool_call_message(session_id, canonical_tool_calls);
                    }
                }

                // Parallel tool execution
                let td_guard = self.tool_dispatcher.read().await;
                let mut futures_list = Vec::new();
                let mem_store_opt = self
                    .memory_store
                    .lock()
                    .ok()
                    .and_then(|ms| ms.as_ref().cloned());
                let llm_doc = llm_config_store::load(&self.platform.paths.config_dir)
                    .unwrap_or_else(|_| llm_config_store::default_document());
                let search_config_dir = self.platform.paths.config_dir.clone();
                let grounded_paths_snapshot = collect_grounded_paths(&messages);
                let grounded_csv_headers_snapshot = collect_grounded_csv_headers(&messages);

                for tc in detected_tool_calls.iter() {
                    let skills_dir = self.platform.paths.skills_dir.clone();
                    let skill_roots = skill_capability_manager::load_snapshot(
                        &self.platform.paths,
                        &RegisteredPaths::load(&self.platform.paths.config_dir),
                    )
                    .roots
                    .into_iter()
                    .map(|root| root.path)
                    .collect::<Vec<_>>();
                    let docs_dir = self.platform.paths.docs_dir.clone();
                    let td_guard_ref = &*td_guard;
                    let tc_name = tc.name.clone();
                    let tc_args = tc.args.clone();
                    let tc_id = tc.id.clone();
                    let bridge_ref = &self.action_bridge;
                    let ms_clone = mem_store_opt.clone();
                    let session_workdir = session_workdir.clone();
                    let llm_doc = llm_doc.clone();
                    let search_config_dir = search_config_dir.clone();
                    let grounded_paths_snapshot = grounded_paths_snapshot.clone();
                    let grounded_csv_headers_snapshot = grounded_csv_headers_snapshot.clone();

                    // ── Phase 11: SafetyCheck per tool ───────────────────
                    let policy_block_reason = if let Ok(tp) = self.tool_policy.lock() {
                        tp.check_policy(session_id, &tc_name, &tc_args).err()
                    } else {
                        None
                    };
                    let safety_block_reason = if let Ok(safety_guard) = self.safety_guard.lock() {
                        let side_effect = td_guard_ref
                            .side_effect_for_tool(&tc_name)
                            .map(SideEffect::from_str)
                            .unwrap_or(SideEffect::Reversible);
                        safety_guard
                            .check_tool_call(
                                &tc_name,
                                &tc_args,
                                &side_effect,
                                loop_state.total_tool_calls,
                            )
                            .err()
                    } else {
                        None
                    };
                    let block_reason = policy_block_reason.or(safety_block_reason);

                    futures_list.push(async move {
                        if let Some(reason) = block_reason {
                            log::warn!("[SafetyCheck] Tool '{}' blocked: {}", tc_name, reason);
                            return LlmMessage::tool_result(&tc_id, &tc_name, serde_json::json!({"error": reason}));
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
                        } else if tc_name == "search_tools" {
                            let query = tc_args.get("query").and_then(|v| v.as_str()).unwrap_or("ALL");

                            let mut all_tools = td_guard_ref.get_tool_declarations();
                            crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_all_builtin_tools(
                                &mut all_tools,
                            );
                            if let Ok(bridge) = bridge_ref.lock() {
                                all_tools.extend(bridge.get_action_declarations());
                            }

                            let mut results = Vec::new();
                            for t in all_tools {
                                if query == "ALL" || t.name.to_lowercase().contains(&query.to_lowercase()) || t.description.to_lowercase().contains(&query.to_lowercase()) {
                                    results.push(serde_json::json!({
                                        "name": t.name,
                                        "description": t.description,
                                        "parameters": t.parameters,
                                    }));
                                }
                            }
                            if results.is_empty() {
                                serde_json::json!({"error": format!("No tools found matching '{}'", query)})
                            } else {
                                serde_json::json!({"tools": results})
                            }
                        } else if tc_name == "create_skill" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed_skill");
                            let description = tc_args
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let content = tc_args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            match crate::core::skill_support::prepare_skill_document(
                                name,
                                description,
                                content,
                            ) {
                                Ok(prepared) => {
                                    let skill_dir_path = skills_dir.join(&prepared.normalized_name);
                                    if let Err(e) = std::fs::create_dir_all(&skill_dir_path) {
                                        serde_json::json!({"error": format!("Failed to create skill directory: {}", e)})
                                    } else {
                                        let skill_md_path = skill_dir_path.join("SKILL.md");
                                        if skill_md_path.is_dir() {
                                            let _ = std::fs::remove_dir_all(&skill_md_path);
                                        }
                                        match std::fs::write(&skill_md_path, prepared.document) {
                                            Ok(_) => serde_json::json!({
                                                "status": "success",
                                                "name": prepared.normalized_name,
                                                "path": skill_md_path.to_string_lossy().to_string(),
                                                "warnings": prepared.warnings,
                                            }),
                                            Err(e) => serde_json::json!({"error": format!("Failed to write skill: {}", e)})
                                        }
                                    }
                                }
                                Err(err) => serde_json::json!({"error": err}),
                            }
                        } else if tc_name == "read_skill" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let normalized_name =
                                crate::core::skill_support::normalize_skill_name(name);
                            if normalized_name.is_empty() {
                                serde_json::json!({"error": "Skill name is required"})
                            } else {
                                match resolve_skill_file(&skill_roots, &normalized_name) {
                                    Some(skill_md_path) => {
                                        match std::fs::read_to_string(&skill_md_path) {
                                            Ok(content) => {
                                                let snapshot = skill_capability_manager::load_snapshot(
                                                    &self.platform.paths,
                                                    &RegisteredPaths::load(&self.platform.paths.config_dir),
                                                );
                                                if let Some(metadata) = snapshot.find_skill(&normalized_name) {
                                                    if !metadata.enabled {
                                                        serde_json::json!({
                                                            "error": format!(
                                                                "Skill '{}' is disabled or missing dependencies. Check '{}'.",
                                                                normalized_name,
                                                                snapshot.config_path
                                                            ),
                                                            "skill": {
                                                                "name": metadata.skill.file_name,
                                                                "dependency_ready": metadata.dependency_ready,
                                                                "missing_requires": metadata.missing_requires,
                                                                "install_hints": metadata.skill.openclaw_install,
                                                                "enabled": metadata.enabled,
                                                            }
                                                        })
                                                    } else {
                                                        serde_json::json!({
                                                            "status": "success",
                                                            "name": normalized_name,
                                                            "path": skill_md_path.to_string_lossy().to_string(),
                                                            "content": content,
                                                            "openclaw": {
                                                                "requires": metadata.skill.openclaw_requires.clone(),
                                                                "install": metadata.skill.openclaw_install.clone(),
                                                            }
                                                        })
                                                    }
                                                } else {
                                                    serde_json::json!({
                                                        "status": "success",
                                                        "name": normalized_name,
                                                        "path": skill_md_path.to_string_lossy().to_string(),
                                                        "content": content,
                                                        "openclaw": {
                                                            "requires": Vec::<String>::new(),
                                                            "install": Vec::<String>::new(),
                                                        }
                                                    })
                                                }
                                            }
                                            Err(e) => serde_json::json!({"error": format!("Failed to read skill '{}': {}", normalized_name, e)})
                                        }
                                    }
                                    None => serde_json::json!({
                                        "error": format!(
                                            "Failed to read skill '{}': not found in managed or registered roots",
                                            normalized_name
                                        )
                                    }),
                                }
                            }
                        } else if tc_name == "list_skill_references" {
                            let docs = crate::core::skill_support::list_skill_reference_docs(&docs_dir);
                            serde_json::json!({
                                "status": "success",
                                "references": docs.into_iter().map(|doc| serde_json::json!({
                                    "name": doc.name,
                                    "path": doc.absolute_path,
                                    "description": doc.description,
                                })).collect::<Vec<_>>()
                            })
                        } else if tc_name == "read_skill_reference" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            match crate::core::skill_support::read_skill_reference_doc(&docs_dir, name) {
                                Ok(doc) => serde_json::json!({
                                    "status": "success",
                                    "name": doc.name,
                                    "path": doc.absolute_path,
                                    "description": doc.description,
                                    "content": doc.content,
                                }),
                                Err(err) => serde_json::json!({"error": err}),
                            }
                        } else if tc_name == "list_agent_roles" {
                            let roles = self.role_registry_snapshot();
                            serde_json::json!({
                                "status": "success",
                                "roles": roles.into_iter().map(|role| serde_json::json!({
                                    "name": role.name,
                                    "description": role.description,
                                "max_iterations": role.max_iterations,
                                "allowed_tools": role.allowed_tools,
                                "type": role.role_type,
                                "auto_start": role.auto_start,
                                "can_delegate_to": role.can_delegate_to,
                                "prompt_mode": role.prompt_mode.map(|mode| match mode {
                                    PromptMode::Full => "full",
                                    PromptMode::Minimal => "minimal",
                                    }),
                                    "reasoning_policy": role.reasoning_policy.map(|policy| match policy {
                                        ReasoningPolicy::Native => "native",
                                        ReasoningPolicy::Tagged => "tagged",
                                    }),
                                })).collect::<Vec<_>>()
                            })
                        } else if tc_name == "spawn_agent" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
                            let system_prompt = tc_args.get("system_prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
                            if name.is_empty() || system_prompt.is_empty() {
                                serde_json::json!({"error": "Missing name or system_prompt"})
                            } else {
                                let allowed_tools = tc_args
                                    .get("allowed_tools")
                                    .and_then(|v| v.as_array())
                                    .map(|items| items.iter().filter_map(|value| value.as_str().map(|value| value.to_string())).collect::<Vec<_>>())
                                    .unwrap_or_default();
                                let role = AgentRole {
                                    name: name.to_string(),
                                    system_prompt: system_prompt.to_string(),
                                    allowed_tools,
                                    max_iterations: tc_args.get("max_iterations").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                    description: tc_args.get("description").and_then(|v| v.as_str()).unwrap_or("Dynamic role").to_string(),
                                    role_type: tc_args.get("type").and_then(|v| v.as_str()).unwrap_or("worker").to_string(),
                                    auto_start: tc_args.get("auto_start").and_then(|v| v.as_bool()).unwrap_or(false),
                                    can_delegate_to: tc_args
                                        .get("can_delegate_to")
                                        .and_then(|v| v.as_array())
                                        .map(|items| items.iter().filter_map(|value| value.as_str().map(|value| value.to_string())).collect::<Vec<_>>())
                                        .unwrap_or_default(),
                                    prompt_mode: prompt_mode_from_str(tc_args.get("prompt_mode").and_then(|v| v.as_str())),
                                    reasoning_policy: reasoning_policy_from_str(tc_args.get("reasoning_policy").and_then(|v| v.as_str())),
                                };
                                if let Ok(mut registry) = self.agent_roles.write() {
                                    registry.add_dynamic_role(role.clone());
                                }
                                serde_json::json!({
                                    "status": "success",
                                    "role": role.name,
                                    "type": role.role_type,
                                    "auto_start": role.auto_start,
                                    "can_delegate_to": role.can_delegate_to,
                                    "prompt_mode": role.prompt_mode.map(|mode| match mode {
                                        PromptMode::Full => "full",
                                        PromptMode::Minimal => "minimal",
                                    }),
                                    "reasoning_policy": role.reasoning_policy.map(|policy| match policy {
                                        ReasoningPolicy::Native => "native",
                                        ReasoningPolicy::Tagged => "tagged",
                                    }),
                                })
                            }
                        } else if tc_name == "create_session" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
                            if name.is_empty() {
                                serde_json::json!({"error": "Missing session name"})
                            } else {
                                let role_name = tc_args.get("role").and_then(|v| v.as_str());
                                match self.build_session_profile(
                                    role_name,
                                    tc_args.get("system_prompt").and_then(|v| v.as_str()),
                                    prompt_mode_from_str(tc_args.get("prompt_mode").and_then(|v| v.as_str())),
                                    reasoning_policy_from_str(tc_args.get("reasoning_policy").and_then(|v| v.as_str())),
                                    None,
                                    None,
                                ) {
                                    Ok(profile) => {
                                        let base_prompt = profile
                                            .system_prompt
                                            .clone()
                                            .unwrap_or_else(|| "You are a TizenClaw sub-session.".into());
                                        let session_id = crate::core::agent_factory::AgentFactory::create_agent_session(name, &base_prompt);
                                        if let Ok(ss) = self.session_store.lock() {
                                            if let Some(store) = ss.as_ref() {
                                                store.ensure_session(&session_id);
                                            }
                                        }
                                        if let Ok(mut profiles) = self.session_profiles.lock() {
                                            profiles.insert(session_id.clone(), profile.clone());
                                        }
                                        serde_json::json!({
                                            "status": "success",
                                            "session_id": session_id,
                                            "role": profile.role_name,
                                            "prompt_mode": profile.prompt_mode.map(|mode| match mode {
                                                PromptMode::Full => "full",
                                                PromptMode::Minimal => "minimal",
                                            }),
                                            "reasoning_policy": profile.reasoning_policy.map(|policy| match policy {
                                                ReasoningPolicy::Native => "native",
                                                ReasoningPolicy::Tagged => "tagged",
                                            }),
                                        })
                                    }
                                    Err(err) => serde_json::json!({"error": err}),
                                }
                            }
                        } else if tc_name == "list_sessions" {
                            let known_sessions = list_known_sessions(&self.platform.paths);
                            let profile_snapshot = self
                                .session_profiles
                                .lock()
                                .ok()
                                .map(|profiles| profiles.clone())
                                .unwrap_or_default();
                            serde_json::json!({
                                "status": "success",
                                "sessions": known_sessions.into_iter().map(|session_id| {
                                    let profile = profile_snapshot.get(&session_id);
                                    serde_json::json!({
                                        "session_id": session_id,
                                        "role": profile.and_then(|profile| profile.role_name.clone()),
                                        "prompt_mode": profile.and_then(|profile| profile.prompt_mode).map(|mode| match mode {
                                            PromptMode::Full => "full",
                                            PromptMode::Minimal => "minimal",
                                        }),
                                        "reasoning_policy": profile.and_then(|profile| profile.reasoning_policy).map(|policy| match policy {
                                            ReasoningPolicy::Native => "native",
                                            ReasoningPolicy::Tagged => "tagged",
                                        }),
                                    })
                                }).collect::<Vec<_>>()
                            })
                        } else if tc_name == "send_to_session" {
                            let target_session = tc_args.get("target_session").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                            let message = tc_args.get("message").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                            if target_session.is_empty() || message.is_empty() {
                                serde_json::json!({"error": "Missing target_session or message"})
                            } else {
                                let reply = Box::pin(self.process_prompt(&target_session, &message, None)).await;
                                serde_json::json!({
                                    "status": "success",
                                    "session_id": target_session,
                                    "response": reply
                                })
                            }
                        } else if tc_name == "run_supervisor" {
                            let goal = tc_args.get("goal").and_then(|v| v.as_str()).unwrap_or("").trim();
                            let strategy = tc_args.get("strategy").and_then(|v| v.as_str()).unwrap_or("sequential");
                            if goal.is_empty() {
                                serde_json::json!({"error": "Missing goal"})
                            } else {
                                let current_profile = self.resolve_session_profile(session_id);
                                let delegated_role_names = current_profile
                                    .as_ref()
                                    .and_then(|profile| profile.can_delegate_to.clone())
                                    .unwrap_or_default();
                                let mut candidate_roles = self
                                    .role_registry_snapshot()
                                    .into_iter()
                                    .filter(|role| !role.is_supervisor())
                                    .filter(|role| !matches!(role.name.as_str(), "default" | "subagent" | "local-reasoner"))
                                    .collect::<Vec<_>>();
                                if !delegated_role_names.is_empty() {
                                    candidate_roles.retain(|role| {
                                        delegated_role_names.iter().any(|name| name == &role.name)
                                    });
                                }

                                let selected_roles = select_delegate_roles(
                                    goal,
                                    &candidate_roles,
                                    if strategy == "parallel" { 3 } else { 2 },
                                );

                                if selected_roles.is_empty() {
                                    serde_json::json!({
                                        "error": "No worker roles are available for supervisor delegation"
                                    })
                                } else {
                                    let mut delegated_sessions = Vec::new();
                                    for role in &selected_roles {
                                        if let Ok(profile) = self.build_session_profile(
                                            Some(&role.name),
                                            None,
                                            None,
                                            None,
                                            None,
                                            None,
                                        ) {
                                            let base_prompt = profile
                                                .system_prompt
                                                .clone()
                                                .unwrap_or_else(|| "You are a TizenClaw sub-session.".into());
                                            let session_name = format!("{}_delegate", role.name);
                                            let delegated_session_id =
                                                crate::core::agent_factory::AgentFactory::create_agent_session(
                                                    &session_name,
                                                    &base_prompt,
                                                );
                                            if let Ok(ss) = self.session_store.lock() {
                                                if let Some(store) = ss.as_ref() {
                                                    store.ensure_session(&delegated_session_id);
                                                }
                                            }
                                            if let Ok(mut profiles) = self.session_profiles.lock() {
                                                profiles.insert(delegated_session_id.clone(), profile);
                                            }
                                            delegated_sessions.push((role.clone(), delegated_session_id));
                                        }
                                    }

                                    if delegated_sessions.is_empty() {
                                        serde_json::json!({
                                            "error": "Failed to create delegated sessions for supervisor execution"
                                        })
                                    } else {
                                        let results = if strategy == "parallel" {
                                            join_all(delegated_sessions.iter().map(|(role, delegated_session_id)| {
                                                let supervisor_hint =
                                                    build_role_supervisor_hint(session_id, goal, role);
                                                async move {
                                                let response = Box::pin(self.process_prompt(
                                                    delegated_session_id,
                                                    &supervisor_hint,
                                                    None,
                                                ))
                                                .await;
                                                serde_json::json!({
                                                    "role": role.name.clone(),
                                                    "session_id": delegated_session_id,
                                                    "response": response,
                                                })
                                            }}))
                                            .await
                                        } else {
                                            let mut sequential_results = Vec::new();
                                            for (role, delegated_session_id) in &delegated_sessions {
                                                let supervisor_hint =
                                                    build_role_supervisor_hint(session_id, goal, role);
                                                let response = Box::pin(self.process_prompt(
                                                    delegated_session_id,
                                                    &supervisor_hint,
                                                    None,
                                                ))
                                                .await;
                                                sequential_results.push(serde_json::json!({
                                                    "role": role.name.clone(),
                                                    "session_id": delegated_session_id,
                                                    "response": response,
                                                }));
                                            }
                                            sequential_results
                                        };

                                        let summary = results
                                            .iter()
                                            .map(|item| {
                                                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                let response = item.get("response").and_then(|v| v.as_str()).unwrap_or("");
                                                format!("[{}] {}", role, response.trim())
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n\n");

                                        serde_json::json!({
                                            "status": "success",
                                            "goal": goal,
                                            "strategy": strategy,
                                            "delegated_count": results.len(),
                                            "results": results,
                                            "summary": summary,
                                        })
                                    }
                                }
                            }
                        } else if tc_name == "file_manager" {
                            file_manager_tool(&tc_args, &session_workdir).await
                        } else if tc_name == "file_write" {
                            let path_str = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let content = tc_args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            if path_str.is_empty() {
                                json!({"error": "Missing required parameter: path"})
                            } else {
                                let file_path = match resolve_workspace_path(&session_workdir, path_str) {
                                    Ok(path) => path,
                                    Err(error) => return LlmMessage::tool_result(
                                        &tc_id,
                                        &tc_name,
                                        json!({"error": error}),
                                    ),
                                };
                                if let Some(parent) = file_path.parent() {
                                    if let Err(e) = std::fs::create_dir_all(parent) {
                                        return LlmMessage::tool_result(
                                            &tc_id,
                                            &tc_name,
                                            json!({"error": format!("Failed to create directory: {}", e)}),
                                        );
                                    }
                                }
                                match std::fs::write(&file_path, content) {
                                    Ok(()) => {
                                        log::info!("[file_write] Wrote {} bytes to {}", content.len(), file_path.display());
                                        json!({
                                            "success": true,
                                            "path": file_path.to_string_lossy(),
                                            "bytes_written": content.len()
                                        })
                                    }
                                    Err(e) => json!({"error": format!("Failed to write file: {}", e)}),
                                }
                            }
                        } else if tc_name == "run_generated_code" {
                            let runtime = tc_args.get("runtime").and_then(|v| v.as_str()).unwrap_or("");
                            let name = tc_args.get("name").and_then(|v| v.as_str());
                            let code = tc_args.get("code").and_then(|v| v.as_str()).unwrap_or("");
                            let args = tc_args.get("args").and_then(|v| v.as_str()).unwrap_or("");
                            match validate_generated_code_grounding(
                                prompt,
                                &grounded_paths_snapshot,
                                &grounded_csv_headers_snapshot,
                                code,
                                args,
                            ) {
                                Err(reason) => serde_json::json!({ "error": reason }),
                                Ok(grounding) => {
                                    let base_dir = self.platform.paths.data_dir.clone();
                                    run_generated_code_tool(
                                        runtime,
                                        name,
                                        code,
                                        args,
                                        &base_dir,
                                        Some(&session_workdir),
                                        grounding.declared_output_path.as_deref(),
                                        grounding.declared_output_level.as_deref(),
                                        prompt_requires_atomic_level_answer(prompt),
                                    )
                                    .await
                                }
                            }
                        } else if tc_name == "run_coding_agent" {
                            let prompt = tc_args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                            if prompt.trim().is_empty() {
                                json!({"error": "Missing required parameter: prompt"})
                            } else {
                                let request = crate::channel::telegram_client::CodingAgentToolRequest {
                                    prompt: prompt.to_string(),
                                    backend: tc_args
                                        .get("backend")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    project_dir: tc_args
                                        .get("project_dir")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    model: tc_args
                                        .get("model")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    execution_mode: tc_args
                                        .get("execution_mode")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    auto_approve: tc_args
                                        .get("auto_approve")
                                        .and_then(|v| v.as_bool()),
                                    timeout_secs: tc_args
                                        .get("timeout_secs")
                                        .and_then(|v| v.as_u64()),
                                };
                                match crate::channel::telegram_client::TelegramClient::run_coding_agent_tool(
                                    &self.platform.paths.config_dir,
                                    &request,
                                )
                                .await
                                {
                                    Ok(result) => result,
                                    Err(error) => json!({ "error": error }),
                                }
                            }
                        } else if tc_name == "manage_generated_code" {
                            let operation = tc_args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
                            let name = tc_args.get("name").and_then(|v| v.as_str());
                            manage_generated_code_tool(operation, name, &session_workdir)
                        } else if tc_name == "list_tasks" {
                            let base_dir = self.platform.paths.data_dir.clone();
                            list_tasks_tool(&base_dir)
                        } else if tc_name == "create_task" {
                            let schedule = tc_args.get("schedule").and_then(|v| v.as_str()).unwrap_or("");
                            let prompt = tc_args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                            let project_dir = tc_args.get("project_dir").and_then(|v| v.as_str());
                            let coding_backend =
                                tc_args.get("coding_backend").and_then(|v| v.as_str());
                            let coding_model =
                                tc_args.get("coding_model").and_then(|v| v.as_str());
                            let execution_mode =
                                tc_args.get("execution_mode").and_then(|v| v.as_str());
                            let auto_approve = tc_args
                                .get("auto_approve")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let base_dir = self.platform.paths.data_dir.clone();
                            create_task_tool(
                                &base_dir,
                                schedule,
                                prompt,
                                project_dir,
                                coding_backend,
                                coding_model,
                                execution_mode,
                                auto_approve,
                            )
                        } else if tc_name == "cancel_task" {
                            let task_id = tc_args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                            let base_dir = self.platform.paths.data_dir.clone();
                            cancel_task_tool(&base_dir, task_id)
                        } else if tc_name == "generate_image" {
                            let prompt = tc_args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                            let path = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let size = tc_args.get("size").and_then(|v| v.as_str());
                            let background = tc_args.get("background").and_then(|v| v.as_str());
                            feature_tools::generate_image(
                                prompt,
                                path,
                                size,
                                background,
                                &session_workdir,
                                &llm_doc,
                            ).await
                        } else if tc_name == "send_outbound_message" {
                            self.send_outbound_message(&tc_args, Some(session_id)).await
                        } else if tc_name == "generate_web_app" {
                            self.generate_web_app(&tc_args).await
                        } else if tc_name == "extract_document_text" {
                            let path = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let output_path = tc_args.get("output_path").and_then(|v| v.as_str());
                            let max_chars = tc_args
                                .get("max_chars")
                                .and_then(|v| v.as_u64())
                                .map(|value| value as usize);
                            feature_tools::extract_document_text(
                                path,
                                output_path,
                                max_chars,
                                &session_workdir,
                            ).await
                        } else if tc_name == "inspect_tabular_data" {
                            let path = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let preview_rows = tc_args
                                .get("preview_rows")
                                .and_then(|v| v.as_u64())
                                .map(|value| value as usize)
                                .unwrap_or(5);
                            feature_tools::inspect_tabular_data(
                                path,
                                preview_rows,
                                &session_workdir,
                            ).await
                        } else if tc_name == "validate_web_search" {
                            let engine = tc_args.get("engine").and_then(|v| v.as_str());
                            feature_tools::validate_web_search(&search_config_dir, engine)
                        } else if tc_name == "web_search" {
                            let query = tc_args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                            let engine = tc_args.get("engine").and_then(|v| v.as_str());
                            let limit = tc_args
                                .get("limit")
                                .and_then(|v| v.as_u64())
                                .map(|value| value as usize)
                                .unwrap_or(5);
                            feature_tools::web_search(
                                query,
                                engine,
                                limit,
                                &session_workdir,
                                &search_config_dir,
                            ).await
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
                        } else if tc_name == "clear_agent_data" {
                            let include_memory = tc_args
                                .get("include_memory")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            let include_sessions = tc_args
                                .get("include_sessions")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            match self.clear_agent_data(include_memory, include_sessions) {
                                Ok(result) => result,
                                Err(error) => serde_json::json!({ "error": error }),
                            }
                        } else {
                            match td_guard_ref
                                .execute_in_dir(&tc_name, &tc_args, None, Some(&session_workdir))
                                .await
                            {
                                Ok(value) => value,
                                Err(error) => serde_json::json!({ "error": error }),
                            }
                        };

                        log::debug!("[ObservationCollect] Tool '{}' result: {} chars",
                            tc_name, result.to_string().len());

                        LlmMessage::tool_result(&tc_id, &tc_name, result)
                    });
                }

                let results = futures_util::future::join_all(futures_list).await;
                let cleared_agent_data = results.iter().any(|result| {
                    result.tool_name == "clear_agent_data"
                        && result.tool_result.get("error").is_none()
                });
                if cleared_agent_data {
                    skip_memory_extraction = true;
                }
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        for result in &results {
                            let trace_name = canonical_tool_names
                                .get(&result.tool_call_id)
                                .map(String::as_str)
                                .unwrap_or(result.tool_name.as_str());
                            store.add_structured_tool_result_message(
                                session_id,
                                trace_name,
                                &result.tool_name,
                                &result.tool_call_id,
                                &result.tool_result,
                            );
                        }
                    }
                }
                if loop_state.token_budget > 0 {
                    let (budgeted_results, budgeted_count) = context_engine
                        .budget_tool_result_messages(results, DEFAULT_TOOL_RESULT_BUDGET_CHARS);
                    if budgeted_count > 0 {
                        loop_state.record_budget_events(budgeted_count);
                        log::info!(
                            "[ToolBudget] Round {} budgeted {} oversized tool result(s)",
                            loop_state.round,
                            budgeted_count
                        );
                    }
                    messages.extend(budgeted_results);
                } else {
                    messages.extend(results);
                }
                if let Err(err) =
                    self.check_context_message_limit(session_id, &messages, &mut loop_state)
                {
                    return format!("Error: {}", err);
                }

                // ── Phase 7: Evaluating (partial progress) ───────────────
                loop_state.transition(AgentPhase::Evaluating);
                let progress_marker =
                    build_progress_marker(&response.text, &reasoning_text, &detected_tool_calls);
                let verdict = loop_state.observe_output(&progress_marker);
                log::debug!(
                    "[Evaluating] Round {} verdict={}",
                    loop_state.round,
                    verdict.as_str()
                );

                if verdict == EvalVerdict::Stuck {
                    loop_state.stuck_retry_count += 1;
                    if loop_state.stuck_retry_count > 2 {
                        log::warn!(
                            "[AgentLoop] Idle loop detected (round {}) - Terminating.",
                            loop_state.round
                        );
                        loop_state.transition(AgentPhase::TerminationCheck);
                        loop_state.transition(AgentPhase::ResultReporting);

                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                store.add_message(
                                    session_id,
                                    "assistant",
                                    "Task aborted (terminal idle loop).",
                                );
                                store.add_structured_assistant_text_message(
                                    session_id,
                                    "Task aborted (terminal idle loop).",
                                );
                            }
                        }
                        loop_state.last_error = Some("Agent is stuck in an execution loop.".into());
                        loop_state.mark_terminal(
                            LoopTransitionReason::StuckLoopAbort,
                            format!(
                                "idle loop detected after {} repeated retries",
                                loop_state.stuck_retry_count
                            ),
                        );
                        self.persist_loop_snapshot(&loop_state);
                        return "Error: Agent is stuck in an execution loop.".into();
                    } else {
                        log::warn!(
                            "[AgentLoop] Idle loop detected (round {}) - Triggering Dynamic Fallback RePlanning.",
                            loop_state.round
                        );
                        loop_state.mark_follow_up(
                            LoopTransitionReason::IdleRecovery,
                            format!(
                                "stuck verdict triggered retry {}",
                                loop_state.stuck_retry_count
                            ),
                        );
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
                    let output_val = if last_msg.role == "tool" {
                        last_msg.tool_result.clone()
                    } else {
                        serde_json::from_str(&last_msg.text)
                            .unwrap_or(Value::String(last_msg.text.clone()))
                    };

                    let we = self.workflow_engine.read().await;
                    if let Some(wf_id) = loop_state.active_workflow_id.clone() {
                        if let Some(wf) = we.get_workflow(&wf_id) {
                            let step = &wf.steps[loop_state.current_workflow_step];
                            loop_state
                                .workflow_vars
                                .insert(step.output_var.clone(), output_val);
                            loop_state.current_workflow_step += 1;
                            loop_state.mark_follow_up(
                                LoopTransitionReason::WorkflowStepAdvance,
                                format!(
                                    "workflow '{}' advanced to step {}",
                                    wf_id, loop_state.current_workflow_step
                                ),
                            );
                        }
                    }
                    continue; // Immediately start next round to pick up next workflow step
                }
            } else {
                if is_file_management_request
                    && collect_successful_file_management_actions(&messages) == 0
                {
                    loop_state.mark_follow_up(
                        LoopTransitionReason::FileActionRequired,
                        "file task still requires direct file_manager or file_write actions",
                    );
                    messages.push(LlmMessage {
                        role: "assistant".into(),
                        text: response.text.clone(),
                        ..Default::default()
                    });
                    messages.push(LlmMessage::user(
                        "The task is not complete yet. Manage the requested files directly with \
                         file_manager or file_write in the working directory. Do not answer with \
                         prose only, and do not use run_generated_code unless an executable script \
                         was explicitly requested.",
                    ));
                    loop_state.transition(AgentPhase::RePlanning);
                    continue;
                }
                if is_file_management_request {
                    let missing_targets =
                        missing_file_management_targets(prompt, &session_workdir, &messages);
                    if !missing_targets.is_empty() {
                        loop_state.mark_follow_up(
                            LoopTransitionReason::FileTargetsMissing,
                            format!(
                                "{} requested file target(s) are still missing",
                                missing_targets.len()
                            ),
                        );
                        messages.push(LlmMessage {
                            role: "assistant".into(),
                            text: response.text.clone(),
                            ..Default::default()
                        });
                        messages.push(LlmMessage::user(&format!(
                            "The task is not complete yet. The following requested files are still missing in the working directory:\n{}\nUse file_manager or file_write to create exactly those files. Do not switch to run_generated_code unless an executable script was explicitly requested.",
                            missing_targets.join("\n")
                        )));
                        loop_state.transition(AgentPhase::RePlanning);
                        continue;
                    }
                }

                let expected_output_paths = expected_persisted_level_script_paths(prompt);
                if !expected_output_paths.is_empty() {
                    let saved_output_paths = collect_successful_saved_output_paths(&messages);
                    let missing_output_paths = expected_output_paths
                        .into_iter()
                        .filter(|path| !saved_output_paths.contains(path))
                        .collect::<Vec<_>>();

                    if !missing_output_paths.is_empty() {
                        loop_state.mark_follow_up(
                            LoopTransitionReason::PersistedOutputsMissing,
                            format!(
                                "{} persisted output file(s) are still missing",
                                missing_output_paths.len()
                            ),
                        );
                        messages.push(LlmMessage {
                            role: "assistant".into(),
                            text: response.text.clone(),
                            ..Default::default()
                        });
                        messages.push(LlmMessage::user(&format!(
                            "The task is not complete yet. Do not respond with prose or fenced code. Use run_generated_code to create the missing files exactly at these paths:\n{}\nIf needed, inspect /tmp/ds_olympiad/problem.md first. Generate exactly one level per run_generated_code call and continue until every file is saved.",
                            missing_output_paths.join("\n")
                        )));
                        loop_state.transition(AgentPhase::RePlanning);
                        continue;
                    }
                }

                let mut advance_workflow = false;
                if let Some(wf_id) = loop_state.active_workflow_id.as_ref() {
                    let we = self.workflow_engine.read().await;
                    if let Some(wf) = we.get_workflow(wf_id) {
                        let step = &wf.steps[loop_state.current_workflow_step];
                        loop_state.workflow_vars.insert(
                            step.output_var.clone(),
                            serde_json::Value::String(response.text.clone()),
                        );
                        loop_state.current_workflow_step += 1;
                        advance_workflow = true;
                    }
                }
                if advance_workflow {
                    loop_state.mark_follow_up(
                        LoopTransitionReason::WorkflowStepAdvance,
                        format!(
                            "workflow text step advanced to {}",
                            loop_state.current_workflow_step
                        ),
                    );
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

                log::debug!(
                    "[Evaluating] Round {} verdict=GoalAchieved (no tool calls)",
                    loop_state.round
                );
                loop_state.mark_terminal(
                    LoopTransitionReason::GoalAchieved,
                    "assistant produced a terminal response without tool calls",
                );

                // Enforce reasoning extraction for final user response
                let final_text = extract_final_text(&response.text);

                let mut text = final_text;
                if is_dashboard_web_app_request {
                    if let Some(args) = generated_web_app_args_from_text(&text) {
                        let generated = self.generate_web_app(&args).await;
                        if generated.get("error").is_none() {
                            text = generated.to_string();
                        } else {
                            log::warn!(
                                "[WebAppFallback] Parsed web app response but generation failed: {}",
                                generated
                            );
                        }
                    }
                }
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.add_message(session_id, "assistant", &text);
                        store.add_structured_assistant_text_message(session_id, &text);
                    }
                }

                // ── Phase 14: ResultReporting ────────────────────────────
                loop_state.transition(AgentPhase::ResultReporting);

                // Trigger auto-extraction (small overhead at end of conversation)
                if !skip_memory_extraction {
                    self.extract_and_save_memory(&messages, &text).await;
                }

                loop_state.transition(AgentPhase::Complete);
                loop_state.log_self_inspection();
                self.persist_loop_snapshot(&loop_state);

                log_conversation("Assistant", &text);
                return text;
            }

            // ── Phase 8: RePlanning / Phase 12: StateTracking ────────────
            loop_state.transition(AgentPhase::StateTracking);

            // ── Phase 13: SelfInspection ─────────────────────────────────
            loop_state.transition(AgentPhase::SelfInspection);
            loop_state.log_self_inspection();
            self.persist_loop_snapshot(&loop_state);

            // In-loop size-based compaction
            loop_state.token_used = context_engine.estimate_tokens(&messages);
            if context_engine.should_compact(&messages, loop_state.token_budget)
                || loop_state.needs_compaction()
            {
                log::debug!(
                    "[ContextEngine] In-loop compaction triggered (round {})",
                    loop_state.round
                );
                messages = context_engine.compact(messages, loop_state.token_budget);
                loop_state.token_used = context_engine.estimate_tokens(&messages);
                self.persist_compacted_messages(session_id, &messages);
            }

            // ── Phase 9: TerminationCheck ─────────────────────────────────
            loop_state.round += 1;
            loop_state.transition(AgentPhase::TerminationCheck);

            if loop_state.is_round_limit_reached() {
                log::warn!(
                    "[AgentLoop] Max rounds ({}) reached for session '{}'",
                    loop_state.max_tool_rounds,
                    session_id
                );
                loop_state.mark_terminal(
                    LoopTransitionReason::RoundLimitReached,
                    format!("max tool rounds {} reached", loop_state.max_tool_rounds),
                );
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
        self.event_bus.stop();
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

    fn llm_config_path_affects_backends(path: &str) -> bool {
        matches!(
            path.split('.').next(),
            Some("active_backend") | Some("fallback_backends") | Some("backends")
        )
    }

    pub fn get_llm_config(&self, path: Option<&str>) -> Result<Value, String> {
        let doc = llm_config_store::load(&self.platform.paths.config_dir)?;
        llm_config_store::get_value(&doc, path)
    }

    pub fn resolve_backend_api_key(&self, backend_name: &str) -> Option<String> {
        let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
        guard.get(backend_name)
    }

    pub fn list_keys(&self) -> Result<Value, String> {
        let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
        Ok(json!({
            "stored": guard.list_stored(),
            "from_env": guard.list_from_env(),
        }))
    }

    pub async fn set_key(&self, key: &str, value: &str) -> Result<Value, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("Key value must not be empty".into());
        }

        {
            let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
            guard.set(key, trimmed)?;
        }

        self.reload_backends().await;
        Ok(json!({
            "key": key,
            "stored": true,
        }))
    }

    pub async fn delete_key(&self, key: &str) -> Result<Value, String> {
        {
            let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
            guard.delete(key)?;
        }

        self.reload_backends().await;
        Ok(json!({
            "key": key,
            "deleted": true,
        }))
    }

    pub async fn test_key(&self, key: &str) -> Result<Value, String> {
        let api_key = self
            .resolve_backend_api_key(key)
            .ok_or_else(|| format!("No API key configured for '{}'", key))?;

        self.ping_backend_key(key, &api_key).await
    }

    pub async fn set_llm_config(&self, path: &str, value: Value) -> Result<Value, String> {
        let mut doc = llm_config_store::load(&self.platform.paths.config_dir)?;
        llm_config_store::set_value(&mut doc, path, value)?;
        llm_config_store::save(&self.platform.paths.config_dir, &doc)?;

        if Self::llm_config_path_affects_backends(path) {
            self.reload_backends().await;
        }

        llm_config_store::get_value(&doc, Some(path))
    }

    pub async fn unset_llm_config(&self, path: &str) -> Result<Value, String> {
        let mut doc = llm_config_store::load(&self.platform.paths.config_dir)?;
        let removed = llm_config_store::unset_value(&mut doc, path)?;
        llm_config_store::save(&self.platform.paths.config_dir, &doc)?;

        if Self::llm_config_path_affects_backends(path) {
            self.reload_backends().await;
        }

        Ok(removed)
    }

    pub async fn reload_llm_backends(&self) -> Result<Value, String> {
        self.reload_backends().await;
        self.get_llm_config(None)
    }

    async fn ping_backend_key(&self, backend_name: &str, api_key: &str) -> Result<Value, String> {
        let doc = llm_config_store::load(&self.platform.paths.config_dir)
            .unwrap_or_else(|_| llm_config_store::default_document());
        let null_value = Value::Null;
        let backend_cfg = doc
            .get("backends")
            .and_then(|value| value.get(backend_name))
            .unwrap_or(&null_value);

        let (url, headers) = match backend_name {
            "anthropic" => {
                let endpoint = config_string(backend_cfg, &["endpoint"])
                    .unwrap_or("https://api.anthropic.com/v1")
                    .trim_end_matches('/')
                    .to_string();
                (
                    format!("{}/models", endpoint),
                    vec![
                        ("x-api-key".to_string(), api_key.to_string()),
                        (
                            "anthropic-version".to_string(),
                            "2023-06-01".to_string(),
                        ),
                    ],
                )
            }
            "openai" => {
                let endpoint = config_string(backend_cfg, &["endpoint"])
                    .unwrap_or("https://api.openai.com/v1")
                    .trim_end_matches('/')
                    .to_string();
                (
                    format!("{}/models", endpoint),
                    vec![(
                        "Authorization".to_string(),
                        format!("Bearer {}", api_key),
                    )],
                )
            }
            "gemini" => {
                let endpoint = config_string(backend_cfg, &["endpoint"])
                    .unwrap_or("https://generativelanguage.googleapis.com/v1beta")
                    .trim_end_matches('/')
                    .to_string();
                (format!("{}/models?key={}", endpoint, api_key), Vec::new())
            }
            "groq" => (
                "https://api.groq.com/openai/v1/models".to_string(),
                vec![(
                    "Authorization".to_string(),
                    format!("Bearer {}", api_key),
                )],
            ),
            _ => return Err(format!("Backend '{}' does not support key.test", backend_name)),
        };

        let owned_headers = headers;
        let borrowed_headers = owned_headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let response = crate::infra::http_client::http_get(&url, &borrowed_headers, 0, 20).await;
        if response.success && (200..300).contains(&response.status_code) {
            return Ok(json!({
                "key": backend_name,
                "reachable": true,
                "status_code": response.status_code,
            }));
        }

        let summary = Self::summarize_backend_test_error(&response);
        Err(format!(
            "Backend '{}' test failed with status {}: {}",
            backend_name, response.status_code, summary
        ))
    }

    fn summarize_backend_test_error(response: &crate::infra::http_client::HttpResponse) -> String {
        if !response.error.trim().is_empty() {
            return response.error.trim().to_string();
        }

        if let Ok(value) = serde_json::from_str::<Value>(&response.body) {
            if let Some(message) = value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
            {
                let trimmed = message.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }

        let trimmed = response.body.trim();
        if trimmed.is_empty() {
            "request failed".to_string()
        } else {
            utf8_safe_preview(trimmed, 160).to_string()
        }
    }

    pub fn get_llm_runtime(&self) -> Value {
        let (configured_active_backend, configured_fallback_backends) = match self.llm_config.lock()
        {
            Ok(config) => (
                config.active_backend.clone(),
                config.fallback_backends.clone(),
            ),
            Err(err) => {
                let config = err.into_inner();
                (
                    config.active_backend.clone(),
                    config.fallback_backends.clone(),
                )
            }
        };
        let runtime_primary_backend = match self.backend_name.read() {
            Ok(name) => name.clone(),
            Err(err) => err.into_inner().clone(),
        };

        json!({
            "configured_active_backend": configured_active_backend,
            "configured_fallback_backends": configured_fallback_backends,
            "runtime_primary_backend": runtime_primary_backend,
            "runtime_has_primary_backend": !runtime_primary_backend.is_empty()
        })
    }

    pub fn clear_agent_data(
        &self,
        include_memory: bool,
        include_sessions: bool,
    ) -> Result<Value, String> {
        if !include_memory && !include_sessions {
            return Err("Nothing selected to clear".to_string());
        }

        let mut result = json!({
            "status": "success",
            "cleared": {
                "memory": false,
                "sessions": false
            }
        });

        if include_memory {
            let deleted_rows = self
                .memory_store
                .lock()
                .map_err(|_| "MemoryStore lock failed".to_string())?
                .as_ref()
                .cloned()
                .ok_or_else(|| "MemoryStore not initialized".to_string())?
                .clear_all()?;
            result["cleared"]["memory"] = Value::Bool(true);
            result["memory"] = json!({
                "records_deleted": deleted_rows
            });
        }

        if include_sessions {
            let cleanup = self
                .session_store
                .lock()
                .map_err(|_| "SessionStore lock failed".to_string())?
                .as_ref()
                .cloned()
                .ok_or_else(|| "SessionStore not initialized".to_string())?
                .clear_all()?;
            result["cleared"]["sessions"] = Value::Bool(true);
            result["sessions"] = cleanup;
        }

        if let Ok(policy) = self.tool_policy.lock() {
            policy.reset_session("default");
        }
        self.reset_circuit_breakers();

        Ok(result)
    }

    pub fn list_registered_paths(&self) -> RegisteredPaths {
        RegisteredPaths::load(&self.platform.paths.config_dir)
    }

    fn runtime_topology(&self) -> crate::core::runtime_paths::RuntimeTopology {
        crate::core::runtime_paths::RuntimeTopology::from_data_dir(
            self.platform.paths.data_dir.clone(),
        )
    }

    pub fn runtime_topology_summary(&self) -> Value {
        self.runtime_topology().summary_json()
    }

    fn persist_loop_snapshot(&self, state: &AgentLoopState) {
        let topology = self.runtime_topology();
        if let Err(err) = std::fs::create_dir_all(&topology.loop_state_dir) {
            log::warn!(
                "[AgentLoop] Failed to create loop state dir '{}': {}",
                topology.loop_state_dir.display(),
                err
            );
            return;
        }

        let snapshot = state.snapshot();
        let payload = json!({
            "session_id": snapshot.session_id,
            "phase": snapshot.phase,
            "original_goal": snapshot.original_goal,
            "plan_step_count": snapshot.plan_step_count,
            "current_step": snapshot.current_step,
            "round": snapshot.round,
            "error_count": snapshot.error_count,
            "tool_retry_count": snapshot.tool_retry_count,
            "max_tool_rounds": snapshot.max_tool_rounds,
            "last_eval_verdict": snapshot.last_eval_verdict,
            "needs_follow_up": snapshot.needs_follow_up,
            "last_transition_reason": snapshot.last_transition_reason,
            "last_transition_detail": snapshot.last_transition_detail,
            "last_error": snapshot.last_error,
            "total_tool_calls": snapshot.total_tool_calls,
            "stuck_retry_count": snapshot.stuck_retry_count,
            "tool_budget_events": snapshot.tool_budget_events,
            "active_workflow_id": snapshot.active_workflow_id,
            "current_workflow_step": snapshot.current_workflow_step,
            "updated_at_unix_secs": unix_timestamp_secs(),
            "resumable": state.phase != AgentPhase::Complete,
        });

        let path = topology.loop_state_path(&state.session_id);
        let tmp_path = path.with_extension("json.tmp");
        let serialized = match serde_json::to_vec_pretty(&payload) {
            Ok(serialized) => serialized,
            Err(err) => {
                log::warn!(
                    "[AgentLoop] Failed to serialize loop snapshot for session '{}': {}",
                    state.session_id,
                    err
                );
                return;
            }
        };

        if let Err(err) =
            std::fs::write(&tmp_path, serialized).and_then(|_| std::fs::rename(&tmp_path, &path))
        {
            log::warn!(
                "[AgentLoop] Failed to persist loop snapshot '{}': {}",
                path.display(),
                err
            );
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }

        log::debug!(
            "[AgentLoop] Snapshot saved: session='{}' phase={} path='{}'",
            state.session_id,
            state.phase.as_str(),
            path.display()
        );
    }

    fn load_loop_snapshot(&self, session_id: &str) -> Value {
        let path = self.runtime_topology().loop_state_path(session_id);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .unwrap_or_else(|| {
                json!({
                    "session_id": session_id,
                    "phase": AgentPhase::GoalParsing.as_str(),
                    "original_goal": "",
                    "plan_step_count": 0,
                    "current_step": 0,
                    "round": 0,
                    "error_count": 0,
                    "tool_retry_count": 0,
                    "max_tool_rounds": AgentLoopState::DEFAULT_MAX_TOOL_ROUNDS,
                    "last_eval_verdict": EvalVerdict::NotStarted.as_str(),
                    "needs_follow_up": false,
                    "last_transition_reason": LoopTransitionReason::LoopInitialized.as_str(),
                    "last_transition_detail": "",
                    "last_error": Value::Null,
                    "total_tool_calls": 0,
                    "stuck_retry_count": 0,
                    "tool_budget_events": 0,
                    "active_workflow_id": Value::Null,
                    "current_workflow_step": 0,
                    "updated_at_unix_secs": unix_timestamp_secs(),
                    "resumable": false,
                })
            })
    }

    pub fn session_runtime_status(&self, session_id: &str) -> Value {
        let registrations = self.list_registered_paths();
        let skill_capabilities =
            skill_capability_manager::load_snapshot(&self.platform.paths, &registrations);
        let tool_audit = self
            .tool_dispatcher
            .try_read()
            .map(|dispatcher| dispatcher.audit_summary())
            .unwrap_or_else(|_| {
                json!({
                    "status": "busy",
                    "reason": "tool dispatcher is currently being updated",
                })
            });
        let session = self
            .session_store
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|store| store.session_runtime_summary(session_id))
            })
            .unwrap_or_else(|| {
                let topology = self.runtime_topology();
                let session_dir = topology.sessions_dir.join(session_id);
                json!({
                    "session_dir": session_dir,
                    "today_path": session_dir.join("today.md"),
                    "compacted_path": session_dir.join("compacted.md"),
                    "compacted_structured_path": session_dir.join("compacted.jsonl"),
                    "transcript_path": session_dir.join("transcript.jsonl"),
                    "workdir_path": topology.data_dir.join("workdirs").join(session_id),
                    "session_exists": false,
                    "message_file_count": 0,
                    "compacted_snapshot_exists": false,
                    "structured_compaction_exists": false,
                    "transcript_exists": false,
                    "resume_ready": false,
                })
            });
        let memory = self
            .memory_store
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|store| store.runtime_summary()))
            .unwrap_or_else(|| {
                let topology = self.runtime_topology();
                let summary_path = topology.memory_dir.join("memory.md");
                json!({
                    "base_dir": topology.memory_dir,
                    "summary_path": summary_path,
                    "short_term_dir": topology.memory_dir.join("short-term"),
                    "long_term_dir": topology.memory_dir.join("long-term"),
                    "episodic_dir": topology.memory_dir.join("episodic"),
                    "summary_exists": false,
                    "prompt_ready": false,
                    "embedding_available": false,
                    "total_entries": 0,
                    "categories": {
                        "general": 0,
                        "facts": 0,
                        "preferences": 0,
                        "episodic": 0,
                    }
                })
            });
        let message_file_count = session
            .get("message_file_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let resume_ready = session
            .get("resume_ready")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let memory_prompt_ready = memory
            .get("prompt_ready")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let memory_total_entries = memory
            .get("total_entries")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);

        json!({
            "status": "ok",
            "session_id": session_id,
            "control_plane": {
                "idle_window": AgentLoopState::IDLE_WINDOW,
                "default_max_tool_rounds": AgentLoopState::DEFAULT_MAX_TOOL_ROUNDS,
                "default_token_budget": AgentLoopState::DEFAULT_TOKEN_BUDGET,
                "default_compact_threshold": AgentLoopState::DEFAULT_COMPACT_THRESHOLD,
            },
            "runtime_topology": self.runtime_topology_summary(),
            "execution": runtime_capabilities::summarize(&self.platform.paths, &registrations),
            "tool_audit": tool_audit.clone(),
            "skills": skill_capabilities.summary_json(),
            "session": session,
            "memory": memory,
            "context_flow": {
                "session_resume_ready": resume_ready,
                "history_message_files": message_file_count,
                "memory_prompt_ready": memory_prompt_ready,
                "memory_total_entries": memory_total_entries,
            },
            "loop_snapshot": self.load_loop_snapshot(session_id),
        })
    }

    pub fn tool_audit_status(&self) -> Value {
        let tool_audit = self
            .tool_dispatcher
            .try_read()
            .map(|dispatcher| dispatcher.audit_summary())
            .unwrap_or_else(|_| {
                json!({
                    "status": "busy",
                    "reason": "tool dispatcher is currently being updated",
                })
            });
        json!({
            "status": "ok",
            "tools": tool_audit,
        })
    }

    pub fn skill_capability_status(&self) -> Value {
        let registrations = self.list_registered_paths();
        let snapshot =
            skill_capability_manager::load_snapshot(&self.platform.paths, &registrations);
        json!({
            "status": "ok",
            "skills": snapshot.summary_json(),
        })
    }

    pub async fn register_external_path(
        &self,
        kind: RegistrationKind,
        raw_path: &str,
    ) -> Result<RegisteredPaths, String> {
        let (registrations, _) =
            registration_store::register_path(&self.platform.paths.config_dir, kind, raw_path)?;
        self.reload_tools().await;
        self.run_startup_indexing().await;
        self.publish_runtime_event(
            "register_external_path",
            json!({"kind": kind.as_str(), "path": raw_path}),
        );
        Ok(registrations)
    }

    pub async fn unregister_external_path(
        &self,
        kind: RegistrationKind,
        raw_path: &str,
    ) -> Result<(RegisteredPaths, bool), String> {
        let (registrations, removed, _) =
            registration_store::unregister_path(&self.platform.paths.config_dir, kind, raw_path)?;
        self.reload_tools().await;
        self.run_startup_indexing().await;
        self.publish_runtime_event(
            "unregister_external_path",
            json!({"kind": kind.as_str(), "path": raw_path, "removed": removed}),
        );
        Ok((registrations, removed))
    }

    pub async fn reload_tools(&self) {
        {
            let mut td = self.tool_dispatcher.write().await;
            *td = ToolDispatcher::new();
            let tool_roots = collect_tool_roots(&self.platform.paths);
            td.load_tools_from_paths(tool_roots.iter().map(|root| root.as_str()));
            log::info!(
                "Tools reloaded: {} declarations from {:?}",
                td.get_tool_declarations().len(),
                tool_roots
            );
        }
        self.publish_runtime_event(
            "reload_tools",
            json!({"tool_roots": collect_tool_roots(&self.platform.paths)}),
        );
    }

    pub async fn run_startup_indexing(&self) {
        use crate::core::tool_indexer;

        self.reload_tools().await;
        let root_dir = self.platform.paths.tools_dir.to_string_lossy().to_string();
        // Embedded descriptors are documentation/indexing metadata for
        // code-defined built-in tools. They are not the execution source.
        let embedded_dir = self
            .platform
            .paths
            .embedded_tools_dir
            .to_string_lossy()
            .to_string();
        let scan_roots = [root_dir.as_str(), embedded_dir.as_str()];
        let skill_roots = collect_skill_roots(&self.platform.paths);
        let skill_count = crate::core::textual_skill_scanner::scan_textual_skills_from_roots(
            &skill_roots.iter().map(|root| root.as_str()).collect::<Vec<_>>(),
        )
        .len();
        let tool_count = self
            .tool_dispatcher
            .read()
            .await
            .get_tool_declarations()
            .len();
        log::info!(
            "[Startup Indexing] Registered {} tools and {} skills from runtime paths.",
            tool_count,
            skill_count
        );

        // Phase 1: Hash-based change detection (fast, no I/O beyond stat)
        if !tool_indexer::needs_reindex_for_roots(&root_dir, &scan_roots) {
            log::info!("[Startup Indexing] No changes detected (hash match). Skipping.");
            return;
        }

        // Phase 2: Local filesystem scan — collect all tool metadata
        log::info!(
            "[Startup Indexing] Scanning tool metadata from {} and {}...",
            root_dir,
            embedded_dir
        );
        let metadata =
            tool_indexer::scan_tools_metadata_with_embedded(&root_dir, Some(&embedded_dir));

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
            let response = self
                .chat_with_fallback(&msgs, &[], None, system_prompt, Some(8192))
                .await;

            if response.success {
                let written =
                    tool_indexer::apply_llm_index_result(&response.text, &root_dir, &metadata);
                if written > 0 {
                    tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
                    log::info!("[Startup Indexing] LLM generated {} index files.", written,);
                } else {
                    log::warn!(
                        "[Startup Indexing] LLM response parsed but 0 files \
                         written. Falling back to template."
                    );
                    tool_indexer::generate_fallback_index(&metadata, &root_dir);
                    tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
                }
            } else {
                log::warn!("[Startup Indexing] LLM call failed. Using fallback template.");
                tool_indexer::generate_fallback_index(&metadata, &root_dir);
                tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
            }
        } else {
            // No LLM available — generate a basic template
            log::info!("[Startup Indexing] No LLM available. Generating fallback index.");
            tool_indexer::generate_fallback_index(&metadata, &root_dir);
            tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
        }

        log::info!("[Startup Indexing] Completed.");
        self.publish_runtime_event(
            "startup_indexing",
            json!({
                "tool_count": tool_count,
                "skill_count": skill_count,
                "tool_roots": scan_roots,
                "skill_roots": skill_roots,
            }),
        );
    }

    /// Extractor sub-task logic. Invokes the LLM to glean long-term knowledge.
    async fn extract_and_save_memory(&self, history: &[LlmMessage], final_response: &str) {
        let ms_clone = self
            .memory_store
            .lock()
            .ok()
            .and_then(|ms| ms.as_ref().cloned());
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
        let response = self
            .chat_with_fallback(&msgs, &[], None, system_prompt, Some(8192))
            .await;

        if response.success {
            let text = response.text.trim();
            // clean potential markdown code block
            let clean_json = if text.starts_with("```json") {
                text.trim_start_matches("```json")
                    .trim_end_matches("```")
                    .trim()
            } else if text.starts_with("```") {
                text.trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
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
                        item.get("value").and_then(|v| v.as_str()),
                    ) {
                        store.set(k, v, cat);
                        count += 1;
                        log::debug!("[MemoryExtractor] Saved memory -> {}: {}", k, v);
                    }
                }
                if count > 0 {
                    log::debug!(
                        "[MemoryExtractor] Successfully saved {} extracted memories.",
                        count
                    );
                }
            } else {
                log::warn!(
                    "[MemoryExtractor] Failed to parse JSON response: {}",
                    clean_json
                );
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

#[cfg(test)]
mod tests {
    use super::{
        append_dashboard_outbound_message, backend_has_preferred_auth,
        build_authoritative_problem_requirements_context, build_backend_candidates,
        build_prefetched_prompt_file_context, build_prefetched_prompt_file_messages,
        build_progress_marker, build_role_supervisor_hint, build_skill_prefetch_message,
        canonical_tool_trace, collect_grounded_csv_headers, collect_grounded_paths,
        dashboard_outbound_queue_path, expected_file_management_targets,
        expected_persisted_level_script_paths, extract_explicit_directory_paths,
        extract_explicit_file_paths, extract_explicit_paths, extract_final_text,
        extract_level_number_from_output_path, file_manager_tool, generated_code_runtime_spec,
        generated_code_script_path, is_simple_file_management_request, manage_generated_code_tool,
        missing_file_management_targets, normalize_conversation_log_text, parse_shell_like_args,
        persist_generated_code_copy, prompt_mode_from_doc, reasoning_policy_from_doc,
        role_relevance_score, sanitize_generated_code_name, select_delegate_roles,
        select_relevant_skills, should_skip_memory_for_prompt, utf8_safe_preview,
        validate_generated_code_execution_output, validate_generated_code_grounding, AgentRole,
        LlmConfig, AUTHENTICATED_BACKEND_PRIORITY_BOOST, MAX_OUTBOUND_DASHBOARD_MESSAGES,
    };
    use crate::core::prompt_builder::{PromptMode, ReasoningPolicy};
    use crate::core::textual_skill_scanner::TextualSkill;
    use crate::infra::key_store::KeyStore;
    use crate::llm::backend::{LlmMessage, LlmToolCall};
    use crate::llm::plugin_manager::PluginManager;
    use serde_json::json;
    use std::collections::HashSet;
    use tempfile::tempdir;

    #[test]
    fn utf8_safe_preview_returns_full_short_ascii_text() {
        let text = "battery";
        assert_eq!(utf8_safe_preview(text, 80), text);
    }

    #[test]
    fn utf8_safe_preview_truncates_on_char_boundary() {
        let text = "Check battery status and summarize it in one line.";
        let preview = utf8_safe_preview(text, 12);

        assert_eq!(preview.chars().count(), 12);
        assert!(text.starts_with(preview));
    }

    #[test]
    fn utf8_safe_preview_handles_zero_length() {
        assert_eq!(utf8_safe_preview("hello", 0), "");
    }

    #[test]
    fn normalize_conversation_log_text_preserves_meaningful_line_breaks() {
        let text = "  First line.\n\n   Return only the result.  ";

        let normalized = normalize_conversation_log_text(text);

        assert_eq!(
            normalized.as_deref(),
            Some("First line.\n\nReturn only the result.")
        );
    }

    #[test]
    fn normalize_conversation_log_text_skips_empty_content() {
        assert_eq!(normalize_conversation_log_text(" \n\t "), None);
    }

    #[test]
    fn select_relevant_skills_prefers_matching_entries() {
        let skills = vec![
            TextualSkill {
                file_name: "battery_monitor".into(),
                absolute_path: "/tmp/battery/SKILL.md".into(),
                description: "Inspect battery and power telemetry".into(),
                tags: vec!["battery".into(), "power".into()],
                triggers: vec!["check battery".into()],
                examples: vec!["check battery status".into()],
                openclaw_requires: Vec::new(),
                openclaw_install: Vec::new(),
                prelude_excerpt: String::new(),
                code_fence_languages: Vec::new(),
                shell_prelude: false,
                searchable_text:
                    "battery_monitor inspect battery and power telemetry battery power check battery check battery status inspect device power".into(),
            },
            TextualSkill {
                file_name: "calendar_sync".into(),
                absolute_path: "/tmp/calendar/SKILL.md".into(),
                description: "Handle schedule sync tasks".into(),
                tags: Vec::new(),
                triggers: Vec::new(),
                examples: Vec::new(),
                openclaw_requires: Vec::new(),
                openclaw_install: Vec::new(),
                prelude_excerpt: String::new(),
                code_fence_languages: Vec::new(),
                shell_prelude: false,
                searchable_text: "calendar_sync handle schedule sync tasks".into(),
            },
        ];

        let selected = select_relevant_skills("please check battery status battery", &skills, 2);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].file_name, "battery_monitor");
    }

    #[test]
    fn backend_has_preferred_auth_accepts_codex_auth_file() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"token-a","refresh_token":"token-b"}}"#,
        )
        .unwrap();

        let cfg = json!({
            "oauth": {
                "auth_path": auth_path
            }
        });

        assert!(backend_has_preferred_auth("openai-codex", &cfg));
    }

    #[test]
    fn backend_has_preferred_auth_rejects_empty_codex_auth_file() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"","refresh_token":""}}"#,
        )
        .unwrap();

        let cfg = json!({
            "oauth": {
                "auth_path": auth_path
            }
        });

        assert!(!backend_has_preferred_auth("openai-codex", &cfg));
    }

    #[test]
    fn authenticated_codex_backend_is_promoted_above_defaults() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"token-a","refresh_token":"token-b"}}"#,
        )
        .unwrap();

        let config = LlmConfig {
            active_backend: "gemini".into(),
            fallback_backends: vec!["openai".into()],
            backends: json!({
                "gemini": {
                    "model": "gemini-2.5-flash"
                },
                "openai": {
                    "api_key": ""
                },
                "openai-codex": {
                    "oauth": {
                        "auth_path": auth_path
                    }
                }
            }),
        };
        let dir = tempdir().unwrap();
        let key_store = KeyStore::new(dir.path());
        let candidates = build_backend_candidates(&config, &PluginManager::new(), &key_store);

        assert_eq!(
            candidates.first().map(|candidate| candidate.name.as_str()),
            Some("openai-codex")
        );
        assert!(candidates
            .iter()
            .find(|candidate| candidate.name == "openai-codex")
            .map(|candidate| candidate.priority >= AUTHENTICATED_BACKEND_PRIORITY_BOOST)
            .unwrap_or(false));
    }

    #[test]
    fn llm_config_default_includes_expected_fallbacks() {
        let config = LlmConfig::default();

        assert_eq!(config.active_backend, "gemini");
        assert_eq!(
            config.fallback_backends,
            vec!["openai".to_string(), "ollama".to_string()]
        );
    }

    #[test]
    fn llm_config_load_uses_default_fallbacks_when_file_missing() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("missing-llm-config.json");

        let config = LlmConfig::load(&config_path.to_string_lossy());

        assert_eq!(config.active_backend, "gemini");
        assert_eq!(
            config.fallback_backends,
            vec!["openai".to_string(), "ollama".to_string()]
        );
    }

    #[test]
    fn build_skill_prefetch_message_returns_snapshot_block() {
        let skills = vec![TextualSkill {
            file_name: "battery_monitor".into(),
            absolute_path: "/tmp/battery/SKILL.md".into(),
            description: "Inspect battery and power telemetry".into(),
            tags: vec!["battery".into()],
            triggers: vec!["check battery".into()],
            examples: Vec::new(),
            openclaw_requires: vec!["upower".into()],
            openclaw_install: vec!["apt install upower".into()],
            prelude_excerpt: String::new(),
            code_fence_languages: Vec::new(),
            shell_prelude: false,
            searchable_text: "battery_monitor inspect battery and power telemetry".into(),
        }];

        let message = build_skill_prefetch_message(&skills).unwrap_or_default();

        assert!(message.contains("Prefetched Skill Snapshot"));
        assert!(message.contains("battery_monitor"));
        assert!(message.contains("tags: battery"));
        assert!(message.contains("requires: upower"));
    }

    #[test]
    fn build_role_supervisor_hint_includes_role_scope() {
        let role = AgentRole {
            name: "device_monitor".into(),
            system_prompt: "monitor device".into(),
            allowed_tools: Vec::new(),
            max_iterations: 4,
            description: "Inspect device health metrics".into(),
            role_type: "worker".into(),
            auto_start: false,
            can_delegate_to: Vec::new(),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Native),
        };

        let hint = build_role_supervisor_hint("sess-1", "check battery health", &role);

        assert!(hint.contains("sess-1"));
        assert!(hint.contains("device_monitor"));
        assert!(hint.contains("Inspect device health metrics"));
    }

    #[test]
    fn build_progress_marker_uses_tool_calls_for_tool_only_rounds() {
        let marker = build_progress_marker(
            "",
            "",
            &[LlmToolCall {
                id: "call_1".into(),
                name: "search_tools".into(),
                args: json!({"query": "ALL"}),
            }],
        );

        assert!(marker.contains("search_tools"));
        assert!(marker.contains("\"ALL\""));
    }

    #[test]
    fn parse_shell_like_args_preserves_quoted_groups() {
        let parsed = parse_shell_like_args("--name \"hello world\" 'alpha beta'");

        assert_eq!(
            parsed,
            vec![
                "--name".to_string(),
                "hello world".to_string(),
                "alpha beta".to_string(),
            ]
        );
    }

    #[test]
    fn extract_explicit_file_paths_finds_absolute_files() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and '/tmp/ds_olympiad/answer.md'.";

        let paths = extract_explicit_file_paths(prompt);

        assert_eq!(
            paths,
            vec![
                "/tmp/ds_olympiad/problem.md".to_string(),
                "/tmp/ds_olympiad/answer.md".to_string(),
            ]
        );
    }

    #[test]
    fn extract_explicit_paths_includes_output_directories() {
        let prompt = "Write files into /tmp/ds_olympiad/result/library-used and /tmp/ds_olympiad/result/library-not-used.";

        let paths = extract_explicit_paths(prompt);
        let directories = extract_explicit_directory_paths(prompt);

        assert!(paths.contains(&"/tmp/ds_olympiad/result/library-used".to_string()));
        assert!(paths.contains(&"/tmp/ds_olympiad/result/library-not-used".to_string()));
        assert_eq!(directories.len(), 2);
    }

    #[test]
    fn collect_grounded_paths_reads_paths_from_tool_stdout_json() {
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];

        let grounded = collect_grounded_paths(&messages);

        assert!(grounded.contains("/tmp/ds_olympiad/problem.md"));
    }

    #[test]
    fn build_prefetched_prompt_file_messages_prefetches_markdown_specs() {
        let dir = tempdir().unwrap();
        let spec_path = dir.path().join("problem.md");
        std::fs::write(&spec_path, "# Problem\nUse the real data.\n").unwrap();
        let csv_path = dir.path().join("data.csv");
        std::fs::write(&csv_path, "x,y\n1,2\n").unwrap();

        let messages = build_prefetched_prompt_file_messages(&[
            spec_path.to_string_lossy().to_string(),
            csv_path.to_string_lossy().to_string(),
        ]);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tool_name, "read_file");
        assert_eq!(
            messages[0]
                .tool_result
                .get("path")
                .and_then(|value| value.as_str()),
            Some(spec_path.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn build_prefetched_prompt_file_context_includes_authoritative_excerpt() {
        let dir = tempdir().unwrap();
        let spec_path = dir.path().join("problem.md");
        std::fs::write(&spec_path, "# Problem\nUse the exact CSV files.\n").unwrap();

        let context =
            build_prefetched_prompt_file_context(&[spec_path.to_string_lossy().to_string()])
                .unwrap_or_default();

        assert!(context.contains("Prompt File Excerpts"));
        assert!(context.contains(spec_path.to_string_lossy().as_ref()));
        assert!(context.contains("Use the exact CSV files."));
    }

    #[test]
    fn build_authoritative_problem_requirements_context_summarizes_levels() {
        let dir = tempdir().unwrap();
        let spec_path = dir.path().join("problem.md");
        std::fs::write(
            &spec_path,
            "## [1단계] 초등학생\n**문제:** `level1_fruit_sales.csv` 파일에서 가장 많이 팔린 과일을 찾으세요.\n## [2단계] 초등학생\n**문제:** `level2_study_time.csv` 파일에서 하루 평균 공부 시간을 구하세요.\n",
        )
        .unwrap();

        let context = build_authoritative_problem_requirements_context(&[spec_path
            .to_string_lossy()
            .to_string()])
        .unwrap_or_default();

        assert!(context.contains("Level 1 uses"));
        assert!(context.contains("level1_fruit_sales.csv"));
        assert!(context.contains("Level 2 uses"));
        assert!(context.contains("exactly one final answer value"));
    }

    #[test]
    fn extract_level_number_from_output_path_reads_level_suffix() {
        assert_eq!(
            extract_level_number_from_output_path(
                "/tmp/ds_olympiad/result/library-used/level-4-solution.py"
            )
            .as_deref(),
            Some("4")
        );
    }

    #[test]
    fn extract_explicit_file_paths_finds_paths_inside_function_calls() {
        let script = "with open('/tmp/ds_olympiad/data.csv') as fh:\n    print(fh.read())";

        let paths = extract_explicit_file_paths(script);

        assert_eq!(paths, vec!["/tmp/ds_olympiad/data.csv".to_string()]);
    }

    #[test]
    fn validate_generated_code_grounding_requires_prompt_files_to_be_read_first() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("problem.md");
        std::fs::write(&input_path, "demo").unwrap();
        let prompt = format!("Read {} and solve it.", input_path.display());
        let grounded_paths = HashSet::new();
        let grounded_csv_headers = HashSet::new();

        let result = validate_generated_code_grounding(
            &prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "print('hello')",
            "",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains(input_path.to_string_lossy().as_ref()));
    }

    #[test]
    fn validate_generated_code_grounding_allows_nonexistent_prompt_output_paths() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("new_result.txt");
        let prompt = format!("Write the answer to {}.", output_path.display());
        let grounded_paths = HashSet::new();
        let grounded_csv_headers = HashSet::new();

        let result = validate_generated_code_grounding(
            &prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "print('hello')",
            "",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn validate_generated_code_grounding_rejects_unverified_paths() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/data.csv') as fh:\n    print(fh.read())",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unverified file paths"));
    }

    #[test]
    fn validate_generated_code_grounding_accepts_declared_output_paths() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and save the script to /tmp/ds_olympiad/result/library-used.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "# /tmp/ds_olympiad/result/library-used/level-1-solution.py\nwith open('/tmp/ds_olympiad/problem.md') as fh:\n    print(fh.read())",
            "",
        )
        .unwrap();

        assert_eq!(
            result.declared_output_path.as_deref(),
            Some("/tmp/ds_olympiad/result/library-used/level-1-solution.py")
        );
    }

    #[test]
    fn validate_generated_code_grounding_requires_leading_comment_output_path() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and save the script to /tmp/ds_olympiad/result/library-used using the filename level-{problem level}-solution.py.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/problem.md') as source:\n    print(source.read())\nwith open('/tmp/ds_olympiad/result/library-used/level-1-solution.py', 'w') as fh:\n    fh.write('demo')\nprint('[Level 1] Answer: demo')",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("leading comment line"));
    }

    #[test]
    fn validate_generated_code_grounding_requires_declared_output_path_when_prompt_demands_files() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/problem.md') as fh:\n    print(fh.read())",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("leading comment line"));
    }

    #[test]
    fn validate_generated_code_grounding_rejects_multi_level_script_when_one_file_is_required() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "# /tmp/ds_olympiad/result/library-used/level-1-solution.py\nwith open('/tmp/ds_olympiad/problem.md') as source:\n    print(source.read())\nprint('[Level 1] Answer: 1')\nprint('[Level 2] Answer: 2')",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly one level per script"));
    }

    #[test]
    fn validate_generated_code_grounding_accepts_known_inputs() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/problem.md') as fh:\n    print(fh.read())",
            "",
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().declared_output_path, None);
    }

    #[test]
    fn validate_generated_code_grounding_rejects_unverified_csv_columns() {
        let prompt = "Read /tmp/ds_olympiad/level1_fruit_sales.csv and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "extract_document_text",
            json!({
                "path": "/tmp/ds_olympiad/level1_fruit_sales.csv",
                "text_preview": "Fruit\nApple\nBanana\n",
                "status": "success"
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "import csv\nwith open('/tmp/ds_olympiad/level1_fruit_sales.csv') as fh:\n    rows = [row['Sales'] for row in csv.DictReader(fh)]\nprint(rows)",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unverified CSV columns"));
    }

    #[test]
    fn validate_generated_code_grounding_rejects_speculative_placeholders() {
        let prompt = "Read /tmp/ds_olympiad/level5_multiple_regression.csv and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "extract_document_text",
            json!({
                "path": "/tmp/ds_olympiad/level5_multiple_regression.csv",
                "text_preview": "X1,X2,Y\n1,2,3\n",
                "status": "success"
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/level5_multiple_regression.csv') as fh:\n    print('placeholder value')",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("speculative assumptions"));
    }

    #[test]
    fn validate_generated_code_grounding_rejects_cross_level_inputs_for_saved_scripts() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used and /tmp/ds_olympiad/result/library-not-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md, /tmp/ds_olympiad/level1_fruit_sales.csv, /tmp/ds_olympiad/level2_study_time.csv.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "read_file",
                json!({
                    "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                    "success": true
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "extract_document_text",
                json!({
                    "path": "/tmp/ds_olympiad/level1_fruit_sales.csv",
                    "text_preview": "Fruit\nApple\nBanana\n",
                    "status": "success"
                }),
            ),
            LlmMessage::tool_result(
                "call_3",
                "extract_document_text",
                json!({
                    "path": "/tmp/ds_olympiad/level2_study_time.csv",
                    "text_preview": "Day,Hours\nMon,2.0\n",
                    "status": "success"
                }),
            ),
        ];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "# /tmp/ds_olympiad/result/library-used/level-1-solution.py\nimport csv\nwith open('/tmp/ds_olympiad/level2_study_time.csv') as fh:\n    print(next(csv.DictReader(fh))['Hours'])",
            "",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("matching inspected input file"));
    }

    #[test]
    fn validate_generated_code_execution_output_rejects_multi_token_answers() {
        let result = validate_generated_code_execution_output(
            "[Level 4] Answer: 81.2 8 12.03\n",
            Some("4"),
            true,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("multiple answer tokens"));
    }

    #[test]
    fn validate_generated_code_execution_output_accepts_single_value_answers() {
        let result = validate_generated_code_execution_output(
            "[Level 2] Answer: 2.2시간\n",
            Some("2"),
            true,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn expected_persisted_level_script_paths_expands_all_levels_and_dirs() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used and /tmp/ds_olympiad/result/library-not-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md, /tmp/ds_olympiad/level1_fruit_sales.csv, /tmp/ds_olympiad/level2_study_time.csv, /tmp/ds_olympiad/level3_student_physical.csv.";

        let expected = expected_persisted_level_script_paths(prompt);

        assert_eq!(
            expected,
            vec![
                "/tmp/ds_olympiad/result/library-not-used/level-1-solution.py",
                "/tmp/ds_olympiad/result/library-not-used/level-2-solution.py",
                "/tmp/ds_olympiad/result/library-not-used/level-3-solution.py",
                "/tmp/ds_olympiad/result/library-used/level-1-solution.py",
                "/tmp/ds_olympiad/result/library-used/level-2-solution.py",
                "/tmp/ds_olympiad/result/library-used/level-3-solution.py",
            ]
        );
    }

    #[test]
    fn persist_generated_code_copy_writes_requested_output_file() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("result/library-used/level-1-solution.py");

        persist_generated_code_copy("print('ok')\n", &output_path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&output_path).unwrap(),
            "print('ok')\n"
        );
    }

    #[test]
    fn generated_code_runtime_spec_maps_supported_runtimes() {
        assert_eq!(
            generated_code_runtime_spec("python"),
            Some(("python3", ".py"))
        );
        assert_eq!(
            generated_code_runtime_spec("python3"),
            Some(("python3", ".py"))
        );
        assert_eq!(generated_code_runtime_spec("node"), Some(("node", ".js")));
        assert_eq!(generated_code_runtime_spec("bash"), Some(("bash", ".sh")));
        assert_eq!(generated_code_runtime_spec("ruby"), None);
    }

    #[test]
    fn sanitize_generated_code_name_normalizes_user_input() {
        assert_eq!(
            sanitize_generated_code_name("Hello World.py"),
            "hello-world-py"
        );
        assert_eq!(sanitize_generated_code_name("   "), "script");
        assert_eq!(
            sanitize_generated_code_name("battery_probe"),
            "battery-probe"
        );
    }

    #[test]
    fn generated_code_script_path_uses_codes_directory() {
        let base_dir = std::path::Path::new("/opt/usr/share/tizenclaw");
        let script_path = generated_code_script_path(base_dir, "python", "Battery Probe").unwrap();

        assert!(script_path.starts_with(base_dir.join("codes")));
        let file_name = script_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap();
        assert!(file_name.contains("-generated-battery-probe"));
        assert_eq!(
            script_path.extension().and_then(|ext| ext.to_str()),
            Some("py")
        );
    }

    #[test]
    fn manage_generated_code_tool_deletes_only_named_file() {
        let unique = format!(
            "tizenclaw-generated-code-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base_dir = std::env::temp_dir().join(unique);
        let codes_dir = base_dir.join("codes");
        std::fs::create_dir_all(&codes_dir).unwrap();
        let keep_path = codes_dir.join("keep.py");
        let delete_path = codes_dir.join("delete.py");
        std::fs::write(&keep_path, "print('keep')").unwrap();
        std::fs::write(&delete_path, "print('delete')").unwrap();

        let result = manage_generated_code_tool("delete", Some("delete.py"), &base_dir);

        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("success")
        );
        assert!(keep_path.exists());
        assert!(!delete_path.exists());

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn append_dashboard_outbound_message_keeps_recent_entries() {
        let unique = format!(
            "tizenclaw-outbound-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base_dir = std::env::temp_dir().join(unique);

        for idx in 0..205 {
            append_dashboard_outbound_message(
                &base_dir,
                Some("Alert"),
                &format!("message-{}", idx),
                Some("sess-1"),
            )
            .unwrap();
        }

        let queue_path = dashboard_outbound_queue_path(&base_dir);
        let content = std::fs::read_to_string(&queue_path).unwrap();
        let lines = content.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), MAX_OUTBOUND_DASHBOARD_MESSAGES);
        assert!(content.contains("message-204"));
        assert!(!content.contains("message-0"));

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn extract_final_text_prefers_final_block() {
        let text = "<think>plan</think>\n<final>Visible answer</final>";
        assert_eq!(extract_final_text(text), "Visible answer");
    }

    #[test]
    fn extract_final_text_strips_think_block_without_final_tag() {
        let text = "<think>private plan</think>\nVisible answer";
        assert_eq!(extract_final_text(text), "Visible answer");
    }

    #[test]
    fn simple_file_management_requests_are_detected() {
        assert!(is_simple_file_management_request(
            "Create a project structure with README.md and src/app.py in the current working directory."
        ));
        assert!(should_skip_memory_for_prompt(
            "Create a project structure with README.md and src/app.py in the current working directory."
        ));
        assert!(!is_simple_file_management_request(
            "Write and execute a Python script that prints the fibonacci sequence."
        ));
        assert!(is_simple_file_management_request(
            "config/app.toml 파일을 열어 내용을 보여줘."
        ));
    }

    #[tokio::test]
    async fn file_manager_tool_can_force_rust_fallback_for_reads() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("note.txt");
        std::fs::write(&file_path, "alpha\n").unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "note.txt",
                "backend_preference": "rust_fallback"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["backend"], json!("rust_fallback"));
        assert_eq!(result["content"], json!("alpha\n"));
    }

    #[tokio::test]
    async fn file_manager_tool_can_force_rust_fallback_for_moves() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("from.txt");
        std::fs::write(&src, "alpha\n").unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "move",
                "src": "from.txt",
                "dst": "to.txt",
                "backend_preference": "rust_fallback"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["backend"], json!("rust_fallback"));
        assert!(!src.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("to.txt")).unwrap(),
            "alpha\n"
        );
    }

    #[test]
    fn wrapped_telegram_prompt_uses_user_request_for_file_detection() {
        let wrapped = "You are handling a Telegram request through TizenClaw.\n\nTelegram development preferences:\n- Coding backend: codex\n- Coding model: gpt-5.4\n- Project directory: /home/hjhun/samba/github/tizenclaw\n- Coding execution mode: plan\n- Coding auto approve: on\n\nTelegram user request:\n지금 상태를 요약해서 알려줘.";

        assert!(!is_simple_file_management_request(wrapped));

        let wrapped_file_request = "You are handling a Telegram request through TizenClaw.\n\nTelegram development preferences:\n- Coding backend: codex\n- Coding model: gpt-5.4\n- Project directory: /home/hjhun/samba/github/tizenclaw\n- Coding execution mode: plan\n- Coding auto approve: on\n\nTelegram user request:\nconfig/app.toml 파일을 열어 내용을 보여줘.";

        assert!(is_simple_file_management_request(wrapped_file_request));
    }

    #[test]
    fn canonical_tool_trace_maps_file_manager_writes() {
        let trace = canonical_tool_trace(&LlmToolCall {
            id: "call_1".to_string(),
            name: "file_manager".to_string(),
            args: json!({
                "operation": "write",
                "path": "README.md",
                "content": "# Demo"
            }),
        });

        assert_eq!(trace["name"], json!("write_file"));
        assert_eq!(trace["arguments"]["path"], json!("README.md"));
    }

    #[test]
    fn expected_file_management_targets_tracks_relative_files_and_readme() {
        let targets = expected_file_management_targets(
            "Create a project structure with README and src/app.py in the current working directory.",
        );

        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "src/app.py")));
        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "README.md")));
    }

    #[test]
    fn missing_file_management_targets_reports_uncreated_relative_files() {
        let dir = tempdir().unwrap();
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "file_write",
            json!({
                "success": true,
                "path": dir.path().join("README.md").to_string_lossy(),
            }),
        )];

        let missing = missing_file_management_targets(
            "Create README and src/app.py in the current working directory.",
            dir.path(),
            &messages,
        );

        assert_eq!(missing, vec!["src/app.py".to_string()]);
    }

    #[test]
    fn expected_file_management_targets_ignore_telegram_wrapper_metadata() {
        let wrapped = "You are handling a Telegram request through TizenClaw.\n\nTelegram development preferences:\n- Coding backend: codex\n- Coding model: gpt-5.4\n- Project directory: /home/hjhun/samba/github/tizenclaw\n- Coding execution mode: plan\n- Coding auto approve: on\n\nTelegram user request:\ndata/sample/llm_config.json.sample 파일을 열어 active_backend 값이 무엇인지 보여줘.";

        let targets = expected_file_management_targets(wrapped);

        assert!(targets.iter().any(|group| group
            .iter()
            .any(|path| path == "data/sample/llm_config.json.sample")));
        assert!(!targets
            .iter()
            .any(|group| group.iter().any(|path| path == "gpt-5.4")));
    }

    #[test]
    fn prompt_policy_auto_selects_ollama_defaults() {
        let doc = json!({"prompt": {"mode": "auto", "reasoning_policy": "auto"}});

        assert_eq!(prompt_mode_from_doc(&doc, "ollama"), PromptMode::Minimal);
        assert_eq!(
            reasoning_policy_from_doc(&doc, "ollama"),
            ReasoningPolicy::Tagged
        );
    }

    #[test]
    fn prompt_policy_auto_selects_hosted_backend_defaults() {
        let doc = json!({"prompt": {"mode": "auto", "reasoning_policy": "auto"}});

        assert_eq!(prompt_mode_from_doc(&doc, "anthropic"), PromptMode::Full);
        assert_eq!(
            reasoning_policy_from_doc(&doc, "anthropic"),
            ReasoningPolicy::Native
        );
    }

    fn sample_agent_role(name: &str, desc: &str) -> AgentRole {
        AgentRole {
            name: name.into(),
            system_prompt: format!("You are {}.", name),
            allowed_tools: Vec::new(),
            max_iterations: 4,
            description: desc.into(),
            role_type: "worker".into(),
            auto_start: false,
            can_delegate_to: Vec::new(),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Native),
        }
    }

    #[test]
    fn role_relevance_score_prefers_matching_roles() {
        let role = sample_agent_role(
            "device_monitor",
            "Monitor battery temperature cpu memory and storage",
        );

        assert!(role_relevance_score("battery temperature status", &role) > 0);
        assert_eq!(role_relevance_score("calendar sync", &role), 0);
    }

    #[test]
    fn select_delegate_roles_falls_back_when_no_keyword_matches() {
        let roles = vec![
            sample_agent_role("device_monitor", "Monitor device health"),
            sample_agent_role("knowledge_retriever", "Search documents"),
        ];

        let selected = select_delegate_roles("completely unrelated request", &roles, 1);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "device_monitor");
    }
}
