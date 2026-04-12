//! Session Store — disk-first conversation persistence.
//!
//! ## Directory Structure
//! ```
//! /opt/usr/share/tizenclaw/sessions/
//! └── {session_id}/
//!     ├── compacted.md      ← compact snapshot (atomic overwrite on compaction)
//!     ├── 2026-04-01.md     ← day-1 conversation (append-only)
//!     ├── 2026-04-02.md     ← day-2 conversation
//!     └── 2026-04-03.md     ← today's conversation (active append target)
//! ```
//!
//! ## Load Strategy (per conversation start)
//! 1. If `compacted.md` exists → use it as the base context snapshot.
//! 2. Load today's `{date}.md` for new messages since the compaction.
//! 3. Deduplicate: merge compacted + today's new messages, apply limit.
//! 4. If no `compacted.md` → load all historical day-files up to `limit`.
//!
//! ## Compaction Persistence
//! When `SizedContextEngine` compacts the in-memory message list, the caller
//! persists the result via `save_compacted()`. The file is written atomically
//! (`.compacted.tmp` → rename) to protect against partial writes on flash.

use crate::llm::backend::{LlmMessage, LlmToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionMessage {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub reasoning_text: String,
    #[serde(default)]
    pub tool_calls: Vec<LlmToolCall>,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub tool_call_id: String,
    #[serde(default)]
    pub tool_result: Value,
    pub timestamp: String,
}

impl SessionMessage {
    pub fn from_llm_message(message: &LlmMessage) -> Self {
        Self {
            role: message.role.clone(),
            text: message.text.clone(),
            reasoning_text: message.reasoning_text.clone(),
            tool_calls: message.tool_calls.clone(),
            tool_name: message.tool_name.clone(),
            tool_call_id: message.tool_call_id.clone(),
            tool_result: message.tool_result.clone(),
            timestamp: String::new(),
        }
    }

    pub fn into_llm_message(self) -> LlmMessage {
        LlmMessage {
            role: self.role,
            text: self.text,
            reasoning_text: self.reasoning_text,
            tool_calls: self.tool_calls,
            tool_name: self.tool_name,
            tool_call_id: self.tool_call_id,
            tool_result: self.tool_result,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TokenUsage {
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_cache_creation_input_tokens: i64,
    pub total_cache_read_input_tokens: i64,
    pub total_requests: i64,
    pub entries: Vec<Value>,
}

impl TokenUsage {
    pub fn to_json(&self) -> Value {
        json!({
            "prompt_tokens": self.total_prompt_tokens,
            "completion_tokens": self.total_completion_tokens,
            "cache_creation_input_tokens": self.total_cache_creation_input_tokens,
            "cache_read_input_tokens": self.total_cache_read_input_tokens,
            "total_requests": self.total_requests
        })
    }

    pub fn from_json(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };

        let read_i64 = |name: &str| value.get(name).and_then(|v| v.as_i64()).unwrap_or(0);

        Self {
            total_prompt_tokens: read_i64("prompt_tokens"),
            total_completion_tokens: read_i64("completion_tokens"),
            total_cache_creation_input_tokens: read_i64("cache_creation_input_tokens"),
            total_cache_read_input_tokens: read_i64("cache_read_input_tokens"),
            total_requests: read_i64("total_requests"),
            entries: Vec::new(),
        }
    }

    pub fn diff_from(&self, baseline: &TokenUsage) -> Self {
        Self {
            total_prompt_tokens: self
                .total_prompt_tokens
                .saturating_sub(baseline.total_prompt_tokens),
            total_completion_tokens: self
                .total_completion_tokens
                .saturating_sub(baseline.total_completion_tokens),
            total_cache_creation_input_tokens: self
                .total_cache_creation_input_tokens
                .saturating_sub(baseline.total_cache_creation_input_tokens),
            total_cache_read_input_tokens: self
                .total_cache_read_input_tokens
                .saturating_sub(baseline.total_cache_read_input_tokens),
            total_requests: self.total_requests.saturating_sub(baseline.total_requests),
            entries: Vec::new(),
        }
    }
}

// ─── SessionStore ─────────────────────────────────────────────────────────────

/// Disk-first session store for conversation history and compaction snapshots.
///
/// `Send + Sync`: uses `Arc<RwLock<()>>` for path-level locking.
/// SQLite connection is retained only for session index queries.
#[derive(Clone)]
pub struct SessionStore {
    base_dir: PathBuf,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    lock: Arc<RwLock<()>>,
}

fn normalize_markdown_block(content: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut blank_run = 0usize;

    for raw_line in content.lines() {
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

fn utf8_prefix(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn utf8_suffix(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    text.chars().skip(total.saturating_sub(max_chars)).collect()
}

fn summarize_large_argument_fields(value: &Value) -> Value {
    const MAX_INLINE_ARG_CHARS: usize = 1600;
    const MAX_INLINE_ARG_HEAD_CHARS: usize = 1000;
    const MAX_INLINE_ARG_TAIL_CHARS: usize = 400;

    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, entry) in map {
                let summarized = match entry {
                    Value::String(text)
                        if matches!(key.as_str(), "content" | "code" | "text")
                            && text.chars().count() > MAX_INLINE_ARG_CHARS =>
                    {
                        json!({
                            "preview": utf8_prefix(text, MAX_INLINE_ARG_HEAD_CHARS),
                            "tail_preview": utf8_suffix(text, MAX_INLINE_ARG_TAIL_CHARS),
                            "char_count": text.chars().count(),
                            "truncated": true
                        })
                    }
                    _ => summarize_large_argument_fields(entry),
                };
                out.insert(key.clone(), summarized);
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(summarize_large_argument_fields).collect())
        }
        other => other.clone(),
    }
}

fn summarize_structured_tool_call_items(content: &[Value]) -> Vec<Value> {
    content
        .iter()
        .map(|item| {
            let Some(object) = item.as_object() else {
                return item.clone();
            };
            let mut normalized = object.clone();
            if let Some(arguments) = normalized.get("arguments").cloned() {
                normalized.insert(
                    "arguments".to_string(),
                    summarize_large_argument_fields(&arguments),
                );
            }
            if let Some(params) = normalized.get("params").cloned() {
                normalized.insert("params".to_string(), summarize_large_argument_fields(&params));
            }
            Value::Object(normalized)
        })
        .collect()
}

impl SessionStore {
    pub fn new(base_dir: &Path, db_path: &str) -> Result<Self, String> {
        let base_dir = base_dir.to_path_buf();
        let sessions_root = base_dir.join("sessions");
        let audit_dir = base_dir.join("audit");

        fs::create_dir_all(&sessions_root).map_err(|e| e.to_string())?;
        fs::create_dir_all(&audit_dir).map_err(|e| e.to_string())?;

        let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_index (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                last_active TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS token_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                date TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER DEFAULT 0,
                completion_tokens INTEGER DEFAULT 0,
                cache_creation_input_tokens INTEGER DEFAULT 0,
                cache_read_input_tokens INTEGER DEFAULT 0
            );",
        )
        .map_err(|e| e.to_string())?;
        Self::ensure_token_usage_columns(&conn).map_err(|e| e.to_string())?;

        Ok(SessionStore {
            base_dir,
            db: Arc::new(std::sync::Mutex::new(conn)),
            lock: Arc::new(RwLock::new(())),
        })
    }

    // ── Path helpers ─────────────────────────────────────────────────────────

    /// Stable session directory: `sessions/{session_id}/`
    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.base_dir.join("sessions").join(session_id)
    }

    /// Today's append-only conversation file: `sessions/{session_id}/{date}.md`
    fn session_file_today(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id)
            .join(format!("{}.md", today_date_str()))
    }

    /// Structured transcript file used by benchmark-style evaluators.
    fn transcript_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("transcript.jsonl")
    }

    /// Compaction snapshot: `sessions/{session_id}/compacted.md`
    fn compacted_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("compacted.md")
    }

    /// Structured compaction snapshot preserving tool-call history.
    fn compacted_structured_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("compacted.jsonl")
    }

    /// Session-scoped working directory for file-oriented tasks.
    pub fn session_workdir(&self, session_id: &str) -> PathBuf {
        let dir = self.base_dir.join("workdirs").join(session_id);
        let _ = fs::create_dir_all(&dir);
        dir
    }

    // ── Session lifecycle ─────────────────────────────────────────────────────

    /// Ensure the session directory and today's file exist.
    pub fn ensure_session(&self, session_id: &str) {
        let dir = self.session_dir(session_id);
        let _ = fs::create_dir_all(&dir);
        let _ = fs::create_dir_all(self.session_workdir(session_id));

        let path = self.session_file_today(session_id);
        if !path.exists() {
            let _g = self.lock.write().unwrap();
            if let Ok(mut f) = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&path)
            {
                let front = format!(
                    "---\nid: {}\ndate: {}\n---\n\n",
                    session_id,
                    today_date_str()
                );
                let _ = f.write_all(front.as_bytes());
            }
        }

        // Update session index
        if let Ok(conn) = self.db.lock() {
            let now = get_timestamp();
            let _ = conn.execute(
                "INSERT INTO session_index (id, created_at, last_active)
                 VALUES (?1, ?2, ?2)
                 ON CONFLICT(id) DO UPDATE SET last_active = ?2",
                rusqlite::params![session_id, now],
            );
        }
    }

    // ── Message persistence ───────────────────────────────────────────────────

    /// Append a message to today's session file.
    pub fn add_message(&self, session_id: &str, role: &str, content: &str) {
        let Some(normalized) = normalize_markdown_block(content) else {
            return;
        };
        self.ensure_session(session_id);
        let path = self.session_file_today(session_id);
        let _g = self.lock.write().unwrap();
        if let Ok(mut file) = OpenOptions::new().append(true).open(&path) {
            let block = format!("## {}\n{}\n\n", role, normalized);
            let _ = file.write_all(block.as_bytes());
        }
    }

    pub fn append_message(&self, session_id: &str, message: &SessionMessage) -> Result<(), String> {
        self.ensure_session(session_id);

        let rendered = render_session_message_body(message);
        let Some(normalized) = normalize_markdown_block(&rendered) else {
            return Ok(());
        };

        let path = self.session_file_today(session_id);
        let block = format!("## {}\n{}\n\n", message.role, normalized);

        let _g = self.lock.write().unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(block.as_bytes()))
            .map_err(|e| e.to_string())
    }

    pub fn add_structured_user_message(&self, session_id: &str, content: &str) {
        let Some(normalized) = normalize_markdown_block(content) else {
            return;
        };
        self.append_structured_event(
            session_id,
            &json!({
                "type": "message",
                "message": {
                    "role": "user",
                    "content": [normalized]
                }
            }),
        );
    }

    pub fn add_structured_assistant_text_message(&self, session_id: &str, content: &str) {
        let Some(normalized) = normalize_markdown_block(content) else {
            return;
        };
        self.append_structured_event(
            session_id,
            &json!({
                "type": "message",
                "message": {
                    "role": "assistant",
                    "content": [{
                        "type": "text",
                        "text": normalized
                    }]
                }
            }),
        );
    }

    pub fn add_structured_tool_call_message(&self, session_id: &str, content: Vec<Value>) {
        if content.is_empty() {
            return;
        }
        let summarized_content = summarize_structured_tool_call_items(&content);
        self.append_structured_event(
            session_id,
            &json!({
                "type": "message",
                "message": {
                    "role": "assistant",
                    "content": summarized_content
                }
            }),
        );
    }

    pub fn add_structured_tool_result_message(
        &self,
        session_id: &str,
        tool_name: &str,
        actual_tool_name: &str,
        tool_call_id: &str,
        result: &Value,
    ) {
        let rendered = if let Some(text) = result.as_str() {
            text.to_string()
        } else {
            result.to_string()
        };

        self.append_structured_event(
            session_id,
            &json!({
                "type": "message",
                "message": {
                    "role": "toolResult",
                    "tool_name": tool_name,
                    "actual_tool_name": actual_tool_name,
                    "tool_call_id": tool_call_id,
                    "content": [rendered]
                }
            }),
        );
    }

    // ── Context loading (primary API) ────────────────────────────────────────

    /// Load session context with compaction-aware deduplication.
    ///
    /// Returns `(messages, from_compacted)`:
    /// - `from_compacted = true`  → `compacted.md` was used as the base
    /// - `from_compacted = false` → full historical load (no snapshot yet)
    pub fn load_session_context(
        &self,
        session_id: &str,
        limit: usize,
    ) -> (Vec<SessionMessage>, bool) {
        let compacted = {
            let structured = self.load_compacted_structured(session_id);
            if structured.is_empty() {
                self.load_compacted(session_id)
            } else {
                structured
            }
        };
        let from_compacted = !compacted.is_empty();

        let merged = if from_compacted {
            // Load the structured transcript and append only messages that came
            // after the compaction snapshot. Fall back to today's markdown file
            // for legacy sessions created before transcript logging existed.
            let transcript = self.load_transcript_messages(session_id);
            let new_msgs = if transcript.is_empty() {
                let today = self.load_file(&self.session_file_today(session_id));
                deduplicate_after_compacted(&compacted, &today)
            } else {
                deduplicate_after_compacted(&compacted, &transcript)
            };
            let mut out = compacted;
            out.extend(new_msgs);
            out
        } else {
            // Prefer the structured transcript when available because it keeps
            // tool calls/results as first-class conversation history.
            let transcript = self.load_transcript_messages(session_id);
            if transcript.is_empty() {
                self.load_all_historical(session_id, limit * 3) // generous pre-limit
            } else {
                let skip = transcript.len().saturating_sub(limit * 3);
                transcript.into_iter().skip(skip).collect()
            }
        };

        // Apply tail-limit
        let skip = if merged.len() > limit {
            merged.len() - limit
        } else {
            0
        };
        let result: Vec<SessionMessage> = merged.into_iter().skip(skip).collect();
        (result, from_compacted)
    }

    pub fn load(&self, session_id: &str, limit: usize) -> Vec<SessionMessage> {
        self.load_session_context(session_id, limit).0
    }

    // ── Compaction persistence ────────────────────────────────────────────────

    /// Load `compacted.md` snapshot. Returns empty Vec if not present.
    pub fn load_compacted(&self, session_id: &str) -> Vec<SessionMessage> {
        let path = self.compacted_path(session_id);
        if !path.exists() {
            return vec![];
        }
        let _g = self.lock.read().unwrap();
        self.load_file(&path)
    }

    /// Atomically persist a compacted message list to `compacted.md`.
    ///
    /// Uses `.compacted.tmp` → `rename()` to prevent partial-write corruption
    /// on embedded flash storage.
    pub fn save_compacted(
        &self,
        session_id: &str,
        messages: &[SessionMessage],
    ) -> Result<(), String> {
        let dir = self.session_dir(session_id);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let final_path = self.compacted_path(session_id);
        let tmp_path = dir.join(".compacted.tmp");

        // Build Markdown content
        let mut content = format!(
            "---\ncompacted_at: {}\nsource_messages: {}\n---\n\n",
            get_timestamp(),
            messages.len()
        );
        for msg in messages {
            let Some(normalized) = normalize_markdown_block(&msg.text) else {
                continue;
            };
            content.push_str(&format!("## {}\n{}\n\n", msg.role, normalized));
        }

        // Write temp → rename (atomic on POSIX/Tizen)
        {
            let _g = self.lock.write().unwrap();
            atomic_write(&tmp_path, &final_path, content.as_bytes())?;
        }

        log::debug!(
            "[SessionStore] compacted.md saved: session='{}' msgs={}",
            session_id,
            messages.len()
        );
        Ok(())
    }

    pub fn load_compacted_structured(&self, session_id: &str) -> Vec<SessionMessage> {
        let path = self.compacted_structured_path(session_id);
        self.read_structured_messages(&path)
    }

    pub fn save_compacted_structured(
        &self,
        session_id: &str,
        messages: &[SessionMessage],
    ) -> Result<(), String> {
        let dir = self.session_dir(session_id);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let final_path = self.compacted_structured_path(session_id);
        let tmp_path = dir.join(".compacted_structured.tmp");
        let serialized = messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
            .join("\n");

        {
            let _g = self.lock.write().unwrap();
            fs::write(&tmp_path, serialized.as_bytes())
                .map_err(|e| format!("structured tmp write failed: {}", e))?;
            fs::rename(&tmp_path, &final_path)
                .map_err(|e| format!("structured rename failed: {}", e))?;
        }

        Ok(())
    }

    // ── Legacy API (kept for compatibility) ───────────────────────────────────

    /// Get recent messages (legacy single-file API; prefer `load_session_context`).
    pub fn get_messages(&self, session_id: &str, limit: usize) -> Vec<SessionMessage> {
        let (msgs, _) = self.load_session_context(session_id, limit);
        msgs
    }

    pub fn list_sessions(&self) -> Vec<String> {
        let sessions_root = self.base_dir.join("sessions");
        let mut sessions: Vec<String> = fs::read_dir(&sessions_root)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
            .filter_map(|entry| {
                if entry.path().is_dir() {
                    entry.file_name().into_string().ok()
                } else {
                    None
                }
            })
            .collect();
        sessions.sort();
        sessions
    }

    /// Remove today's session file. Does NOT delete the session directory
    /// or compacted.md — only wipes the current day's conversation.
    pub fn clear_session(&self, session_id: &str) {
        let path = self.session_file_today(session_id);
        let _g = self.lock.write().unwrap();
        let _ = fs::remove_file(path);
    }

    pub fn session_runtime_summary(&self, session_id: &str) -> Value {
        let session_dir = self.session_dir(session_id);
        let today_path = self.session_file_today(session_id);
        let compacted_path = self.compacted_path(session_id);
        let compacted_structured_path = self.compacted_structured_path(session_id);
        let transcript_path = self.transcript_path(session_id);
        let workdir_path = self.base_dir.join("workdirs").join(session_id);

        let message_file_count = fs::read_dir(&session_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        entry
                            .path()
                            .file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| name.ends_with(".md") && name != "compacted.md")
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);

        let transcript_exists = transcript_path.exists();
        let compacted_exists = compacted_path.exists();
        let compacted_structured_exists = compacted_structured_path.exists();
        let resume_ready = compacted_exists
            || compacted_structured_exists
            || transcript_exists
            || message_file_count > 0;

        json!({
            "session_dir": session_dir,
            "today_path": today_path,
            "compacted_path": compacted_path,
            "compacted_structured_path": compacted_structured_path,
            "transcript_path": transcript_path,
            "workdir_path": workdir_path,
            "session_exists": session_dir.exists(),
            "message_file_count": message_file_count,
            "compacted_snapshot_exists": compacted_exists,
            "structured_compaction_exists": compacted_structured_exists,
            "transcript_exists": transcript_exists,
            "resume_ready": resume_ready,
        })
    }

    pub fn clear_all(&self) -> Result<Value, String> {
        let sessions_root = self.base_dir.join("sessions");
        let workdirs_root = self.base_dir.join("workdirs");
        let audit_root = self.base_dir.join("audit");

        let session_entries = fs::read_dir(&sessions_root)
            .ok()
            .map(|entries| entries.filter_map(|entry| entry.ok()).count())
            .unwrap_or(0);
        let workdir_entries = fs::read_dir(&workdirs_root)
            .ok()
            .map(|entries| entries.filter_map(|entry| entry.ok()).count())
            .unwrap_or(0);

        if let Ok(conn) = self.db.lock() {
            conn.execute("DELETE FROM session_index", [])
                .map_err(|err| format!("Failed to clear session_index: {}", err))?;
            conn.execute("DELETE FROM token_usage", [])
                .map_err(|err| format!("Failed to clear token_usage: {}", err))?;
        }

        {
            let _g = self.lock.write().unwrap();
            if sessions_root.exists() {
                fs::remove_dir_all(&sessions_root).map_err(|err| {
                    format!(
                        "Failed to remove sessions directory '{}': {}",
                        sessions_root.display(),
                        err
                    )
                })?;
            }
            if workdirs_root.exists() {
                fs::remove_dir_all(&workdirs_root).map_err(|err| {
                    format!(
                        "Failed to remove workdirs directory '{}': {}",
                        workdirs_root.display(),
                        err
                    )
                })?;
            }
        }

        fs::create_dir_all(&sessions_root).map_err(|err| {
            format!(
                "Failed to recreate sessions directory '{}': {}",
                sessions_root.display(),
                err
            )
        })?;
        fs::create_dir_all(&workdirs_root).map_err(|err| {
            format!(
                "Failed to recreate workdirs directory '{}': {}",
                workdirs_root.display(),
                err
            )
        })?;
        fs::create_dir_all(&audit_root).map_err(|err| {
            format!(
                "Failed to ensure audit directory '{}': {}",
                audit_root.display(),
                err
            )
        })?;

        Ok(json!({
            "sessions_deleted": session_entries,
            "workdirs_deleted": workdir_entries,
        }))
    }

    // ── Token usage ──────────────────────────────────────────────────────────

    pub fn record_usage(
        &self,
        session_id: &str,
        prompt_tokens: i32,
        completion_tokens: i32,
        cache_creation_input_tokens: i32,
        cache_read_input_tokens: i32,
        model: &str,
    ) {
        if let Ok(conn) = self.db.lock() {
            let today = today_date_str();
            let _ = conn.execute(
                "INSERT INTO token_usage (
                    session_id,
                    date,
                    model,
                    prompt_tokens,
                    completion_tokens,
                    cache_creation_input_tokens,
                    cache_read_input_tokens
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    session_id,
                    today,
                    model,
                    prompt_tokens,
                    completion_tokens,
                    cache_creation_input_tokens,
                    cache_read_input_tokens
                ],
            );
        }
    }

    pub fn load_token_usage(&self, session_id: &str) -> TokenUsage {
        let mut usage = TokenUsage::default();
        if let Ok(conn) = self.db.lock() {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        prompt_tokens,
                        completion_tokens,
                        cache_creation_input_tokens,
                        cache_read_input_tokens
                     FROM token_usage
                     WHERE session_id = ?1",
                )
                .unwrap();
            let iter = stmt
                .query_map(rusqlite::params![session_id], |row| {
                    let p: i64 = row.get(0)?;
                    let c: i64 = row.get(1)?;
                    let cache_create: i64 = row.get(2)?;
                    let cache_read: i64 = row.get(3)?;
                    Ok((p, c, cache_create, cache_read))
                })
                .unwrap();
            for item in iter.flatten() {
                usage.total_prompt_tokens += item.0;
                usage.total_completion_tokens += item.1;
                usage.total_cache_creation_input_tokens += item.2;
                usage.total_cache_read_input_tokens += item.3;
                usage.total_requests += 1;
            }
        }
        usage
    }

    pub fn load_daily_usage(&self, date: &str) -> TokenUsage {
        let mut usage = TokenUsage::default();
        let target_date = if date.is_empty() {
            today_date_str()
        } else {
            date.to_string()
        };
        if let Ok(conn) = self.db.lock() {
            let mut stmt = conn
                .prepare(
                    "SELECT
                        prompt_tokens,
                        completion_tokens,
                        cache_creation_input_tokens,
                        cache_read_input_tokens
                     FROM token_usage
                     WHERE date = ?1",
                )
                .unwrap();
            let iter = stmt
                .query_map(rusqlite::params![target_date], |row| {
                    let p: i64 = row.get(0)?;
                    let c: i64 = row.get(1)?;
                    let cache_create: i64 = row.get(2)?;
                    let cache_read: i64 = row.get(3)?;
                    Ok((p, c, cache_create, cache_read))
                })
                .unwrap();
            for item in iter.flatten() {
                usage.total_prompt_tokens += item.0;
                usage.total_completion_tokens += item.1;
                usage.total_cache_creation_input_tokens += item.2;
                usage.total_cache_read_input_tokens += item.3;
                usage.total_requests += 1;
            }
        }
        usage
    }

    fn ensure_token_usage_columns(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare("PRAGMA table_info(token_usage)")?;
        let column_names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .flatten()
            .collect();

        if !column_names
            .iter()
            .any(|name| name == "cache_creation_input_tokens")
        {
            conn.execute(
                "ALTER TABLE token_usage
                 ADD COLUMN cache_creation_input_tokens INTEGER DEFAULT 0",
                [],
            )?;
        }
        if !column_names
            .iter()
            .any(|name| name == "cache_read_input_tokens")
        {
            conn.execute(
                "ALTER TABLE token_usage
                 ADD COLUMN cache_read_input_tokens INTEGER DEFAULT 0",
                [],
            )?;
        }

        Ok(())
    }

    fn append_structured_event(&self, session_id: &str, event: &Value) {
        self.ensure_session(session_id);
        let path = self.transcript_path(session_id);
        let _g = self.lock.write().unwrap();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = file.write_all(event.to_string().as_bytes());
            let _ = file.write_all(b"\n");
            let _ = file.flush();
            let _ = file.sync_all();
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Parse a single Markdown session file into `Vec<SessionMessage>`.
    fn load_file(&self, path: &PathBuf) -> Vec<SessionMessage> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        parse_session_markdown(&content)
    }

    fn read_structured_messages(&self, path: &PathBuf) -> Vec<SessionMessage> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    serde_json::from_str::<SessionMessage>(trimmed).ok()
                }
            })
            .collect()
    }

    fn load_transcript_messages(&self, session_id: &str) -> Vec<SessionMessage> {
        let path = self.transcript_path(session_id);
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut messages = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            messages.extend(parse_transcript_event(&event));
        }

        messages
    }

    /// Load all day-files for a session, sorted oldest-first, up to `limit`.
    fn load_all_historical(&self, session_id: &str, limit: usize) -> Vec<SessionMessage> {
        let dir = self.session_dir(session_id);
        let mut day_files: Vec<PathBuf> = match fs::read_dir(&dir) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().map(|e| e == "md").unwrap_or(false)
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n != "compacted.md")
                            .unwrap_or(false)
                })
                .collect(),
            Err(_) => return vec![],
        };

        // Sort by filename (YYYY-MM-DD.md) lexicographically = chronological
        day_files.sort();

        let mut all: Vec<SessionMessage> = Vec::new();
        for path in &day_files {
            all.extend(self.load_file(path));
        }

        // Return last `limit` messages
        let skip = if all.len() > limit {
            all.len() - limit
        } else {
            0
        };
        all.into_iter().skip(skip).collect()
    }
}

// ─── Utility Functions ────────────────────────────────────────────────────────

/// Parse `## role\ncontent\n\n` blocks from a session Markdown file.
fn parse_session_markdown(content: &str) -> Vec<SessionMessage> {
    let mut messages = Vec::new();
    let mut current_role = String::new();
    let mut current_text: Vec<&str> = Vec::new();

    for line in content.lines() {
        if let Some(role_str) = line.strip_prefix("## ") {
            // Flush previous block
            if !current_role.is_empty() {
                messages.push(SessionMessage {
                    role: current_role.clone(),
                    text: current_text.join("\n").trim().to_string(),
                    reasoning_text: String::new(),
                    tool_calls: Vec::new(),
                    tool_name: String::new(),
                    tool_call_id: String::new(),
                    tool_result: Value::Null,
                    timestamp: String::new(),
                });
                current_text.clear();
            }
            current_role = role_str.trim().to_string();
        } else if !current_role.is_empty() && !line.starts_with("---") {
            current_text.push(line);
        }
    }
    // Flush last block
    if !current_role.is_empty() {
        messages.push(SessionMessage {
            role: current_role,
            text: current_text.join("\n").trim().to_string(),
            reasoning_text: String::new(),
            tool_calls: Vec::new(),
            tool_name: String::new(),
            tool_call_id: String::new(),
            tool_result: Value::Null,
            timestamp: String::new(),
        });
    }
    messages
}

fn flatten_transcript_content_text(content: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(items) = content.as_array() {
        for item in items {
            if let Some(text) = item.as_str() {
                if !text.trim().is_empty() {
                    parts.push(text.trim().to_string());
                }
            } else if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                if !text.trim().is_empty() {
                    parts.push(text.trim().to_string());
                }
            }
        }
    }
    parts.join("\n")
}

fn parse_transcript_tool_calls(content: &Value) -> Vec<LlmToolCall> {
    let Some(items) = content.as_array() else {
        return Vec::new();
    };

    items
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("toolCall"))
        .filter_map(|item| {
            let name = item
                .get("actual_tool_name")
                .or_else(|| item.get("name"))
                .and_then(|value| value.as_str())?;
            let args = item
                .get("arguments")
                .or_else(|| item.get("params"))
                .cloned()
                .unwrap_or(Value::Null);
            let id = item
                .get("tool_call_id")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            Some(LlmToolCall {
                id,
                name: name.to_string(),
                args,
            })
        })
        .collect()
}

fn parse_tool_result_content(content: &Value) -> Value {
    let rendered = content
        .as_array()
        .and_then(|items| items.first())
        .cloned()
        .unwrap_or_else(|| content.clone());

    match rendered {
        Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        other => other,
    }
}

fn parse_transcript_event(event: &Value) -> Vec<SessionMessage> {
    let Some(message) = event.get("message") else {
        return Vec::new();
    };
    let Some(role) = message.get("role").and_then(|value| value.as_str()) else {
        return Vec::new();
    };

    match role {
        "user" => {
            let text = flatten_transcript_content_text(&message["content"]);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![SessionMessage {
                    role: "user".to_string(),
                    text,
                    ..SessionMessage::default()
                }]
            }
        }
        "assistant" => {
            let text = flatten_transcript_content_text(&message["content"]);
            let tool_calls = parse_transcript_tool_calls(&message["content"]);
            let mut out = Vec::new();
            if !text.is_empty() {
                out.push(SessionMessage {
                    role: "assistant".to_string(),
                    text,
                    ..SessionMessage::default()
                });
            }
            if !tool_calls.is_empty() {
                out.push(SessionMessage {
                    role: "assistant".to_string(),
                    tool_calls,
                    ..SessionMessage::default()
                });
            }
            out
        }
        "toolResult" => vec![SessionMessage {
            role: "tool".to_string(),
            tool_name: message
                .get("actual_tool_name")
                .or_else(|| message.get("tool_name"))
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            tool_call_id: message
                .get("tool_call_id")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            tool_result: parse_tool_result_content(&message["content"]),
            ..SessionMessage::default()
        }],
        _ => Vec::new(),
    }
}

fn render_session_message_body(message: &SessionMessage) -> String {
    if let Some(normalized) = normalize_markdown_block(&message.text) {
        return normalized;
    }

    if !message.tool_calls.is_empty() {
        return serde_json::to_string(&message.tool_calls).unwrap_or_default();
    }

    if !message.tool_name.is_empty() || !message.tool_call_id.is_empty() || !message.tool_result.is_null()
    {
        return json!({
            "tool_name": message.tool_name,
            "tool_call_id": message.tool_call_id,
            "tool_result": message.tool_result,
        })
        .to_string();
    }

    message.timestamp.clone()
}

fn atomic_write(tmp_path: &Path, final_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut tmp_file =
        File::create(tmp_path).map_err(|e| format!("tmp write failed: {}", e))?;
    tmp_file
        .write_all(bytes)
        .map_err(|e| format!("tmp write failed: {}", e))?;
    tmp_file
        .sync_all()
        .map_err(|e| format!("tmp sync failed: {}", e))?;
    drop(tmp_file);

    fs::rename(tmp_path, final_path).map_err(|e| format!("rename failed: {}", e))?;

    if let Some(parent) = final_path.parent() {
        File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| format!("dir sync failed: {}", e))?;
    }

    Ok(())
}

/// Return only messages from `today` that are NOT already represented in
/// `compacted` — prevents re-adding messages already in the snapshot.
///
/// Strategy: scan today's messages from the end; drop any that exactly match
/// any message in compacted (role + text). This is a simple deduplication
/// heuristic suitable for append-only session files.
fn deduplicate_after_compacted(
    compacted: &[SessionMessage],
    today: &[SessionMessage],
) -> Vec<SessionMessage> {
    if compacted.is_empty() {
        return today.to_vec();
    }
    // Build a set of (role, first-100-chars) from compacted for fast lookup
    let compacted_set: std::collections::HashSet<(String, String)> =
        compacted.iter().map(session_message_dedup_key).collect();

    today
        .iter()
        .filter(|msg| !compacted_set.contains(&session_message_dedup_key(msg)))
        .cloned()
        .collect()
}

fn session_message_dedup_key(message: &SessionMessage) -> (String, String) {
    let preview_source = if !message.text.is_empty() {
        message.text.clone()
    } else if !message.tool_calls.is_empty() {
        serde_json::to_string(&message.tool_calls).unwrap_or_default()
    } else if !message.tool_name.is_empty() || !message.tool_call_id.is_empty() {
        format!(
            "{}:{}:{}",
            message.tool_name, message.tool_call_id, message.tool_result
        )
    } else {
        message.reasoning_text.clone()
    };

    (
        message.role.clone(),
        preview_source.chars().take(100).collect::<String>(),
    )
}

fn get_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn today_date_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let y = (days * 4 + 2) / 1461 + 1970;
    let mut doy = days - ((y - 1970) * 365 + (y - 1969) / 4);
    let leap = if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
        1
    } else {
        0
    };
    let months = [31u64, 28 + leap, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0u64;
    for (i, &ml) in months.iter().enumerate() {
        if doy < ml {
            m = i as u64 + 1;
            break;
        }
        doy -= ml;
    }
    if m == 0 {
        m = 12;
    }
    format!("{:04}-{:02}-{:02}", y, m, doy + 1)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_store(base: &Path) -> SessionStore {
        // Override base_dir for testing
        let db_path = base.join("test.db");
        let store = SessionStore {
            base_dir: base.to_path_buf(),
            db: Arc::new(std::sync::Mutex::new(
                rusqlite::Connection::open(&db_path).unwrap(),
            )),
            lock: Arc::new(RwLock::new(())),
        };
        // Create tables
        {
            let conn = store.db.lock().unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS session_index (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL,
                    last_active TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS token_usage (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id TEXT NOT NULL,
                    date TEXT NOT NULL,
                    model TEXT NOT NULL,
                    prompt_tokens INTEGER DEFAULT 0,
                    completion_tokens INTEGER DEFAULT 0,
                    cache_creation_input_tokens INTEGER DEFAULT 0,
                    cache_read_input_tokens INTEGER DEFAULT 0
                );",
            )
            .unwrap();
            SessionStore::ensure_token_usage_columns(&conn).unwrap();
        }
        store
    }

    #[test]
    fn test_session_dir_structure() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        store.ensure_session("sess_abc");
        assert!(tmp.path().join("sessions").join("sess_abc").is_dir());
        let today = tmp
            .path()
            .join("sessions")
            .join("sess_abc")
            .join(format!("{}.md", today_date_str()));
        assert!(
            today.exists(),
            "today's file must exist after ensure_session"
        );
    }

    #[test]
    fn test_add_and_load_messages() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        store.add_message("s1", "user", "Hello TizenClaw");
        store.add_message("s1", "assistant", "How can I help?");
        let (msgs, from_compacted) = store.load_session_context("s1", 10);
        assert!(!from_compacted, "no compacted.md yet");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].text, "Hello TizenClaw");
        assert_eq!(msgs[1].role, "assistant");
    }

    #[test]
    fn test_save_and_load_compacted() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        let messages = vec![
            SessionMessage {
                role: "system".into(),
                text: "You are TizenClaw.".into(),
                ..SessionMessage::default()
            },
            SessionMessage {
                role: "user".into(),
                text: "Original goal".into(),
                ..SessionMessage::default()
            },
            SessionMessage {
                role: "assistant".into(),
                text: "Got it.".into(),
                ..SessionMessage::default()
            },
        ];

        store
            .save_compacted("s2", &messages)
            .expect("save_compacted must succeed");
        let path = tmp.path().join("sessions").join("s2").join("compacted.md");
        assert!(path.exists(), "compacted.md must exist after save");

        let loaded = store.load_compacted("s2");
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].role, "system");
        assert_eq!(loaded[2].text, "Got it.");
    }

    #[test]
    fn test_load_session_context_prefers_compacted() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        // Save a compacted snapshot
        let compacted = vec![
            SessionMessage {
                role: "user".into(),
                text: "old question".into(),
                ..SessionMessage::default()
            },
            SessionMessage {
                role: "assistant".into(),
                text: "old answer".into(),
                ..SessionMessage::default()
            },
        ];
        store.save_compacted("s3", &compacted).unwrap();

        // Add new message today (not in compacted)
        store.add_message("s3", "user", "new question today");

        let (msgs, from_compacted) = store.load_session_context("s3", 20);
        assert!(from_compacted, "must load from compacted.md");
        // Should have: 2 compacted + 1 new
        assert_eq!(msgs.len(), 3, "compacted (2) + today new (1) = 3");
        assert_eq!(msgs[2].text, "new question today");
    }

    #[test]
    fn test_deduplicate_after_compacted_removes_overlap() {
        let compacted = vec![SessionMessage {
            role: "user".into(),
            text: "same message".into(),
            ..SessionMessage::default()
        }];
        let today = vec![
            SessionMessage {
                role: "user".into(),
                text: "same message".into(),
                ..SessionMessage::default()
            },
            SessionMessage {
                role: "assistant".into(),
                text: "new reply".into(),
                ..SessionMessage::default()
            },
        ];
        let result = deduplicate_after_compacted(&compacted, &today);
        assert_eq!(result.len(), 1, "duplicate removed, only new reply kept");
        assert_eq!(result[0].role, "assistant");
    }

    #[test]
    fn test_atomic_write_no_partial_file() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        let msgs = vec![SessionMessage {
            role: "user".into(),
            text: "test".into(),
            ..SessionMessage::default()
        }];
        // Tmp file should not persist after successful save
        store.save_compacted("s4", &msgs).unwrap();
        let tmp_file = tmp
            .path()
            .join("sessions")
            .join("s4")
            .join(".compacted.tmp");
        assert!(
            !tmp_file.exists(),
            ".compacted.tmp must be cleaned up after rename"
        );
    }

    #[test]
    fn test_append_message_and_load_alias_roundtrip() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store
            .append_message(
                "alias_s1",
                &SessionMessage {
                    role: "user".into(),
                    text: "hello from append_message".into(),
                    timestamp: get_timestamp(),
                    ..SessionMessage::default()
                },
            )
            .unwrap();

        let loaded = store.load("alias_s1", 10);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].role, "user");
        assert_eq!(loaded[0].text, "hello from append_message");
    }

    #[test]
    fn test_load_compacted_returns_empty_if_not_exists() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        let loaded = store.load_compacted("nonexistent_session");
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_session_context_reads_tool_history_from_transcript() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_structured_user_message("tool_hist", "방금 만든 파일을 보여줘");
        store.add_structured_tool_call_message(
            "tool_hist",
            vec![json!({
                "type": "toolCall",
                "tool_call_id": "call_1",
                "name": "read_file",
                "actual_tool_name": "file_manager",
                "arguments": {"path": ".tmp/demo.txt"}
            })],
        );
        store.add_structured_tool_result_message(
            "tool_hist",
            "read_file",
            "file_manager",
            "call_1",
            &json!({"path": ".tmp/demo.txt", "content": "demo"}),
        );

        let (msgs, from_compacted) = store.load_session_context("tool_hist", 10);
        assert!(!from_compacted);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].tool_calls.len(), 1);
        assert_eq!(msgs[1].tool_calls[0].id, "call_1");
        assert_eq!(msgs[2].role, "tool");
        assert_eq!(msgs[2].tool_name, "file_manager");
        assert_eq!(msgs[2].tool_call_id, "call_1");
        assert_eq!(msgs[2].tool_result["content"], json!("demo"));
    }

    #[test]
    fn test_load_session_context_prefers_actual_tool_name_for_tool_results() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_structured_tool_result_message(
            "tool_hist_alias",
            "list_files",
            "file_manager",
            "call_alias",
            &json!({"entries": []}),
        );

        let (msgs, from_compacted) = store.load_session_context("tool_hist_alias", 10);
        assert!(!from_compacted);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "tool");
        assert_eq!(msgs[0].tool_name, "file_manager");
        assert_eq!(msgs[0].tool_call_id, "call_alias");
    }

    #[test]
    fn test_save_and_load_compacted_structured_preserves_tool_fields() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        let messages = vec![
            SessionMessage {
                role: "assistant".into(),
                tool_calls: vec![LlmToolCall {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    args: json!({"path": ".tmp/demo.txt"}),
                }],
                ..SessionMessage::default()
            },
            SessionMessage {
                role: "tool".into(),
                tool_name: "read_file".into(),
                tool_call_id: "call_1".into(),
                tool_result: json!({"path": ".tmp/demo.txt", "content": "demo"}),
                ..SessionMessage::default()
            },
        ];

        store
            .save_compacted_structured("tool_hist2", &messages)
            .expect("save_compacted_structured must succeed");
        let loaded = store.load_compacted_structured("tool_hist2");
        assert_eq!(loaded, messages);
    }

    #[test]
    fn test_session_runtime_summary_reports_resume_artifacts() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_message("runtime", "user", "hello");
        store.add_structured_assistant_text_message("runtime", "world");

        let summary = store.session_runtime_summary("runtime");

        assert_eq!(summary["session_exists"], true);
        assert_eq!(summary["resume_ready"], true);
        assert_eq!(summary["message_file_count"], 1);
        assert_eq!(summary["transcript_exists"], true);
        assert!(summary["session_dir"]
            .as_str()
            .unwrap()
            .ends_with("/sessions/runtime"));
        assert!(summary["workdir_path"]
            .as_str()
            .unwrap()
            .ends_with("/workdirs/runtime"));
    }

    #[test]
    fn test_message_limit_applied() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        for i in 0..15 {
            store.add_message("s5", "user", &format!("msg {}", i));
        }
        let (msgs, _) = store.load_session_context("s5", 5);
        assert_eq!(msgs.len(), 5);
        assert_eq!(msgs[0].text, "msg 10"); // last 5 of 15
    }

    #[test]
    fn test_parse_session_markdown_skips_frontmatter() {
        let content =
            "---\nid: test\ndate: 2026-04-03\n---\n\n## user\nHello\n\n## assistant\nHi!\n\n";
        let msgs = parse_session_markdown(content);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].text, "Hello");
    }

    #[test]
    fn test_normalize_markdown_block_collapses_extra_blank_lines() {
        let normalized = normalize_markdown_block("  hello  \n\n\n world  ");
        assert_eq!(normalized.as_deref(), Some("hello\n\nworld"));
    }

    #[test]
    fn test_token_usage_tracks_cache_counters() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.record_usage("cache_s1", 100, 20, 300, 240, "anthropic");
        store.record_usage("cache_s1", 50, 10, 0, 128, "gemini");

        let usage = store.load_token_usage("cache_s1");
        assert_eq!(usage.total_prompt_tokens, 150);
        assert_eq!(usage.total_completion_tokens, 30);
        assert_eq!(usage.total_cache_creation_input_tokens, 300);
        assert_eq!(usage.total_cache_read_input_tokens, 368);
        assert_eq!(usage.total_requests, 2);
    }

    #[test]
    fn test_session_workdir_is_created_per_session() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        let workdir = store.session_workdir("bench_s1");

        assert!(workdir.exists());
        assert!(workdir.ends_with("workdirs/bench_s1"));
    }

    #[test]
    fn test_structured_transcript_writes_jsonl_events() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_structured_user_message("bench_s2", "hello");
        store.add_structured_assistant_text_message("bench_s2", "world");
        store.add_structured_tool_call_message(
            "bench_s2",
            vec![json!({
                "type": "toolCall",
                "name": "read_file",
                "params": {"path": "notes.md"}
            })],
        );
        store.add_structured_tool_result_message(
            "bench_s2",
            "read_file",
            "file_manager",
            "call_1",
            &json!({"content": "demo"}),
        );

        let transcript = tmp
            .path()
            .join("sessions")
            .join("bench_s2")
            .join("transcript.jsonl");
        let transcript_text = std::fs::read_to_string(transcript).unwrap();
        let lines: Vec<&str> = transcript_text.lines().collect();

        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("\"role\":\"user\""));
        assert!(lines[1].contains("\"role\":\"assistant\""));
        assert!(lines[2].contains("\"toolCall\""));
        assert!(lines[3].contains("\"role\":\"toolResult\""));
    }

    #[test]
    fn test_structured_transcript_summarizes_large_tool_call_content() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_structured_tool_call_message(
            "bench_s3",
            vec![json!({
                "type": "toolCall",
                "name": "file_write",
                "actual_tool_name": "file_write",
                "tool_call_id": "call_big",
                "arguments": {
                    "path": "daily_briefing.md",
                    "content": "A".repeat(2200)
                }
            })],
        );

        let transcript = tmp
            .path()
            .join("sessions")
            .join("bench_s3")
            .join("transcript.jsonl");
        let transcript_text = std::fs::read_to_string(transcript).unwrap();
        let event: Value = serde_json::from_str(transcript_text.lines().next().unwrap()).unwrap();
        let content = event["message"]["content"].as_array().unwrap();

        assert_eq!(content[0]["arguments"]["path"], json!("daily_briefing.md"));
        assert_eq!(content[0]["arguments"]["content"]["truncated"], json!(true));
        assert_eq!(content[0]["arguments"]["content"]["char_count"], json!(2200));
        assert!(content[0]["arguments"]["content"]["preview"]
            .as_str()
            .unwrap()
            .chars()
            .count() < 2200);
        assert_eq!(
            content[0]["arguments"]["content"]["tail_preview"],
            json!("A".repeat(400))
        );
    }

    #[test]
    fn test_structured_transcript_keeps_head_and_tail_for_large_mixed_content() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        let large = format!("{}MIDDLE{}", "A".repeat(1200), "Z".repeat(500));

        store.add_structured_tool_call_message(
            "bench_s4",
            vec![json!({
                "type": "toolCall",
                "name": "file_write",
                "actual_tool_name": "file_write",
                "tool_call_id": "call_mixed",
                "arguments": {
                    "path": "alpha_summary.md",
                    "content": large
                }
            })],
        );

        let transcript = tmp
            .path()
            .join("sessions")
            .join("bench_s4")
            .join("transcript.jsonl");
        let transcript_text = std::fs::read_to_string(transcript).unwrap();
        let event: Value = serde_json::from_str(transcript_text.lines().next().unwrap()).unwrap();
        let payload = &event["message"]["content"][0]["arguments"]["content"];

        assert_eq!(payload["truncated"], json!(true));
        assert!(payload["preview"]
            .as_str()
            .unwrap()
            .starts_with(&"A".repeat(200)));
        assert!(payload["tail_preview"]
            .as_str()
            .unwrap()
            .ends_with(&"Z".repeat(200)));
    }

    #[test]
    fn test_clear_all_removes_sessions_workdirs_and_usage() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_message("wipe_s1", "user", "hello");
        store.add_structured_user_message("wipe_s1", "hello");
        store.record_usage("wipe_s1", 10, 5, 0, 0, "demo");
        let workdir = store.session_workdir("wipe_s1");
        std::fs::write(workdir.join("sample.txt"), "demo").unwrap();

        let result = store.clear_all().unwrap();

        assert_eq!(result["sessions_deleted"].as_u64().unwrap_or(0), 1);
        assert_eq!(result["workdirs_deleted"].as_u64().unwrap_or(0), 1);
        assert!(tmp.path().join("sessions").exists());
        assert!(tmp.path().join("workdirs").exists());
        assert!(store.load_session_context("wipe_s1", 10).0.is_empty());
        assert_eq!(store.load_token_usage("wipe_s1").total_requests, 0);
    }

    #[test]
    fn test_list_sessions_reads_session_ids_from_disk() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());

        store.add_message("list_a", "user", "hello");
        store.add_message("list_b", "assistant", "world");

        let sessions = store.list_sessions();
        assert_eq!(sessions, vec!["list_a".to_string(), "list_b".to_string()]);
    }

    #[test]
    fn session_message_serialization_roundtrip() {
        let msg = SessionMessage {
            role: "user".into(),
            text: "hello".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: SessionMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg.text, restored.text);
    }

    #[test]
    fn test_token_usage_diff_subtracts_baseline() {
        let current = TokenUsage {
            total_prompt_tokens: 120,
            total_completion_tokens: 40,
            total_cache_creation_input_tokens: 80,
            total_cache_read_input_tokens: 20,
            total_requests: 3,
            entries: Vec::new(),
        };
        let baseline = TokenUsage {
            total_prompt_tokens: 100,
            total_completion_tokens: 10,
            total_cache_creation_input_tokens: 50,
            total_cache_read_input_tokens: 5,
            total_requests: 1,
            entries: Vec::new(),
        };

        let diff = current.diff_from(&baseline);

        assert_eq!(diff.total_prompt_tokens, 20);
        assert_eq!(diff.total_completion_tokens, 30);
        assert_eq!(diff.total_cache_creation_input_tokens, 30);
        assert_eq!(diff.total_cache_read_input_tokens, 15);
        assert_eq!(diff.total_requests, 2);
    }
}
