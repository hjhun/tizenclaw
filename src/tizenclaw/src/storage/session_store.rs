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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

// ─── Data Types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionMessage {
    pub role: String,
    pub text: String,
    pub timestamp: String,
}

#[derive(Clone, Debug, Default)]
pub struct TokenUsage {
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_requests: i64,
    pub entries: Vec<Value>,
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

impl SessionStore {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let base_dir = PathBuf::from("/opt/usr/share/tizenclaw");
        let sessions_root = base_dir.join("sessions");
        let audit_dir = base_dir.join("audit");

        fs::create_dir_all(&sessions_root).map_err(|e| e.to_string())?;
        fs::create_dir_all(&audit_dir).map_err(|e| e.to_string())?;

        let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS session_index (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                last_active TEXT NOT NULL
            )",
            [],
        ).map_err(|e| e.to_string())?;

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

    /// Compaction snapshot: `sessions/{session_id}/compacted.md`
    fn compacted_path(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("compacted.md")
    }

    // ── Session lifecycle ─────────────────────────────────────────────────────

    /// Ensure the session directory and today's file exist.
    pub fn ensure_session(&self, session_id: &str) {
        let dir = self.session_dir(session_id);
        let _ = fs::create_dir_all(&dir);

        let path = self.session_file_today(session_id);
        if !path.exists() {
            let _g = self.lock.write().unwrap();
            if let Ok(mut f) = OpenOptions::new().create(true).write(true).open(&path) {
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
        self.ensure_session(session_id);
        let path = self.session_file_today(session_id);
        let _g = self.lock.write().unwrap();
        if let Ok(mut file) = OpenOptions::new().append(true).open(&path) {
            let block = format!("## {}\n{}\n\n", role, content);
            let _ = file.write_all(block.as_bytes());
        }
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
        let compacted = self.load_compacted(session_id);
        let from_compacted = !compacted.is_empty();

        let merged = if from_compacted {
            // Load today's file; append only messages that came AFTER the
            // compaction (avoid re-adding messages already in the snapshot).
            let today = self.load_file(&self.session_file_today(session_id));
            let new_msgs = deduplicate_after_compacted(&compacted, &today);
            let mut out = compacted;
            out.extend(new_msgs);
            out
        } else {
            // No compaction snapshot — collect all day-files for this session
            self.load_all_historical(session_id, limit * 3) // generous pre-limit
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
            content.push_str(&format!("## {}\n{}\n\n", msg.role, msg.text));
        }

        // Write temp → rename (atomic on POSIX/Tizen)
        {
            let _g = self.lock.write().unwrap();
            fs::write(&tmp_path, content.as_bytes())
                .map_err(|e| format!("tmp write failed: {}", e))?;
            fs::rename(&tmp_path, &final_path)
                .map_err(|e| format!("rename failed: {}", e))?;
        }

        log::info!(
            "[SessionStore] compacted.md saved: session='{}' msgs={}",
            session_id,
            messages.len()
        );
        Ok(())
    }

    // ── Legacy API (kept for compatibility) ───────────────────────────────────

    /// Get recent messages (legacy single-file API; prefer `load_session_context`).
    pub fn get_messages(&self, session_id: &str, limit: usize) -> Vec<SessionMessage> {
        let (msgs, _) = self.load_session_context(session_id, limit);
        msgs
    }

    /// Remove today's session file. Does NOT delete the session directory
    /// or compacted.md — only wipes the current day's conversation.
    pub fn clear_session(&self, session_id: &str) {
        let path = self.session_file_today(session_id);
        let _g = self.lock.write().unwrap();
        let _ = fs::remove_file(path);
    }

    // ── Token usage (stub — audit integration pending) ────────────────────────

    pub fn record_usage(
        &self,
        _session_id: &str,
        _prompt_tokens: i32,
        _completion_tokens: i32,
        _model: &str,
    ) {
        // Future: write to audit/{session_id}.json
    }

    pub fn load_token_usage(&self, _session_id: &str) -> TokenUsage {
        TokenUsage::default()
    }

    pub fn load_daily_usage(&self, _date: &str) -> TokenUsage {
        TokenUsage::default()
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
        let skip = if all.len() > limit { all.len() - limit } else { 0 };
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
        if line.starts_with("## ") {
            // Flush previous block
            if !current_role.is_empty() {
                messages.push(SessionMessage {
                    role: current_role.clone(),
                    text: current_text.join("\n").trim().to_string(),
                    timestamp: String::new(),
                });
                current_text.clear();
            }
            current_role = line[3..].trim().to_string();
        } else if !current_role.is_empty() && !line.starts_with("---") {
            current_text.push(line);
        }
    }
    // Flush last block
    if !current_role.is_empty() {
        messages.push(SessionMessage {
            role: current_role,
            text: current_text.join("\n").trim().to_string(),
            timestamp: String::new(),
        });
    }
    messages
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
    let compacted_set: std::collections::HashSet<(String, String)> = compacted
        .iter()
        .map(|m| {
            let preview = m.text.chars().take(100).collect::<String>();
            (m.role.clone(), preview)
        })
        .collect();

    today
        .iter()
        .filter(|msg| {
            let preview = msg.text.chars().take(100).collect::<String>();
            !compacted_set.contains(&(msg.role.clone(), preview))
        })
        .cloned()
        .collect()
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
    let leap = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
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
            conn.execute(
                "CREATE TABLE IF NOT EXISTS session_index (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL,
                    last_active TEXT NOT NULL
                )",
                [],
            ).unwrap();
        }
        store
    }

    #[test]
    fn test_session_dir_structure() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        store.ensure_session("sess_abc");
        assert!(tmp.path().join("sessions").join("sess_abc").is_dir());
        let today = tmp.path().join("sessions").join("sess_abc")
            .join(format!("{}.md", today_date_str()));
        assert!(today.exists(), "today's file must exist after ensure_session");
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
            SessionMessage { role: "system".into(), text: "You are TizenClaw.".into(), timestamp: String::new() },
            SessionMessage { role: "user".into(), text: "Original goal".into(), timestamp: String::new() },
            SessionMessage { role: "assistant".into(), text: "Got it.".into(), timestamp: String::new() },
        ];

        store.save_compacted("s2", &messages).expect("save_compacted must succeed");
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
            SessionMessage { role: "user".into(), text: "old question".into(), timestamp: String::new() },
            SessionMessage { role: "assistant".into(), text: "old answer".into(), timestamp: String::new() },
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
        let compacted = vec![
            SessionMessage { role: "user".into(), text: "same message".into(), timestamp: String::new() },
        ];
        let today = vec![
            SessionMessage { role: "user".into(), text: "same message".into(), timestamp: String::new() },
            SessionMessage { role: "assistant".into(), text: "new reply".into(), timestamp: String::new() },
        ];
        let result = deduplicate_after_compacted(&compacted, &today);
        assert_eq!(result.len(), 1, "duplicate removed, only new reply kept");
        assert_eq!(result[0].role, "assistant");
    }

    #[test]
    fn test_atomic_write_no_partial_file() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        let msgs = vec![
            SessionMessage { role: "user".into(), text: "test".into(), timestamp: String::new() },
        ];
        // Tmp file should not persist after successful save
        store.save_compacted("s4", &msgs).unwrap();
        let tmp_file = tmp.path().join("sessions").join("s4").join(".compacted.tmp");
        assert!(!tmp_file.exists(), ".compacted.tmp must be cleaned up after rename");
    }

    #[test]
    fn test_load_compacted_returns_empty_if_not_exists() {
        let tmp = tempdir().unwrap();
        let store = make_store(tmp.path());
        let loaded = store.load_compacted("nonexistent_session");
        assert!(loaded.is_empty());
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
        let content = "---\nid: test\ndate: 2026-04-03\n---\n\n## user\nHello\n\n## assistant\nHi!\n\n";
        let msgs = parse_session_markdown(content);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].text, "Hello");
    }
}
