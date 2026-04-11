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
        logger
            .init_tables()
            .map_err(|e| format!("DB init: {}", e))?;
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
            CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_events(timestamp);",
        )
    }

    pub fn log(&self, event_type: &str, session_id: &str, details: &Value) {
        let details_str = details.to_string();
        let _ = self.db.execute(
            "INSERT INTO audit_events (event_type, session_id, details) VALUES (?1, ?2, ?3)",
            params![event_type, session_id, details_str],
        );
    }

    pub fn query_recent(&self, limit: usize) -> Vec<Value> {
        let mut stmt = match self.db.prepare(
            "SELECT id, event_type, session_id, details, timestamp
             FROM audit_events
             ORDER BY timestamp DESC, id DESC
             LIMIT ?1",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![limit as i64], map_audit_event_row)
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect())
            .unwrap_or_default()
    }

    pub fn query_by_type(&self, event_type: &str, limit: usize) -> Vec<Value> {
        let mut stmt = match self.db.prepare(
            "SELECT id, event_type, session_id, details, timestamp
             FROM audit_events
             WHERE event_type = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT ?2",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![event_type, limit as i64], map_audit_event_row)
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect())
            .unwrap_or_default()
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

    pub fn log_llm_call(
        &self,
        session_id: &str,
        backend: &str,
        prompt_tokens: i32,
        completion_tokens: i32,
    ) {
        let details = serde_json::json!({
            "backend": backend,
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens
        });
        self.log("llm_call", session_id, &details);
    }
}

fn map_audit_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let details_raw: String = row.get(3)?;
    let details = serde_json::from_str(&details_raw).unwrap_or_else(|_| Value::String(details_raw));

    Ok(serde_json::json!({
        "id": row.get::<_, i64>(0)?,
        "event_type": row.get::<_, String>(1)?,
        "session_id": row.get::<_, String>(2)?,
        "details": details,
        "timestamp": row.get::<_, String>(4)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_and_query_recent_roundtrip() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("audit.db");
        let logger = AuditLogger::new(db_path.to_str().unwrap()).unwrap();

        logger.log(
            "tool_call",
            "sess-1",
            &serde_json::json!({"tool": "read_file", "status": "ok"}),
        );

        let rows = logger.query_recent(5);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["event_type"], "tool_call");
        assert_eq!(rows[0]["session_id"], "sess-1");
        assert_eq!(rows[0]["details"]["tool"], "read_file");
    }

    #[test]
    fn test_query_by_type_filters_rows() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("audit.db");
        let logger = AuditLogger::new(db_path.to_str().unwrap()).unwrap();

        logger.log("tool_call", "sess-1", &serde_json::json!({"tool": "read_file"}));
        logger.log("error", "sess-2", &serde_json::json!({"message": "boom"}));

        let rows = logger.query_by_type("error", 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["event_type"], "error");
        assert_eq!(rows[0]["details"]["message"], "boom");
    }
}
