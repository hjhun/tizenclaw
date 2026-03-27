//! Audit logger — records security and operational events to SQLite.

use rusqlite::params;
use serde_json::Value;

use super::sqlite;

pub struct AuditLogger {
    db: rusqlite::Connection,
}

impl AuditLogger {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let db = sqlite::open_database(db_path).map_err(|e| format!("DB open: {}", e))?;
        let logger = AuditLogger { db };
        logger.init_tables().map_err(|e| format!("DB init: {}", e))?;
        Ok(logger)
    }

    fn init_tables(&self) -> rusqlite::Result<()> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                session_id TEXT DEFAULT '',
                details TEXT DEFAULT '{}',
                timestamp TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_audit_type ON audit_events(event_type);
            CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_events(timestamp);"
        )
    }

    pub fn log(&self, event_type: &str, session_id: &str, details: &Value) {
        let details_str = details.to_string();
        let _ = self.db.execute(
            "INSERT INTO audit_events (event_type, session_id, details) VALUES (?1, ?2, ?3)",
            params![event_type, session_id, details_str],
        );
    }

    pub fn log_ipc_auth(&self, uid: u32, pid: u32, allowed: bool) {
        let details = serde_json::json!({
            "uid": uid, "pid": pid, "allowed": allowed
        });
        self.log("ipc_auth", "", &details);
    }

    pub fn log_tool_exec(&self, session_id: &str, tool_name: &str, exit_code: i32) {
        let details = serde_json::json!({
            "tool": tool_name, "exit_code": exit_code
        });
        self.log("tool_exec", session_id, &details);
    }

    pub fn log_llm_call(&self, session_id: &str, backend: &str, prompt_tokens: i32, completion_tokens: i32) {
        let details = serde_json::json!({
            "backend": backend,
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens
        });
        self.log("llm_call", session_id, &details);
    }
}
