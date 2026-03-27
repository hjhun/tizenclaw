//! Memory store — persistent key-value memory for the agent.

use rusqlite::params;
use super::sqlite;

pub struct MemoryStore {
    db: rusqlite::Connection,
}

impl MemoryStore {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let db = sqlite::open_database(db_path).map_err(|e| format!("DB open: {}", e))?;
        let store = MemoryStore { db };
        store.init_tables().map_err(|e| format!("DB init: {}", e))?;
        Ok(store)
    }

    fn init_tables(&self) -> rusqlite::Result<()> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                category TEXT DEFAULT 'general',
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_mem_category ON memories(category);"
        )
    }

    pub fn set(&self, key: &str, value: &str, category: &str) {
        let _ = self.db.execute(
            "INSERT OR REPLACE INTO memories (key, value, category, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![key, value, category],
        );
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.db.query_row(
            "SELECT value FROM memories WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ).ok()
    }

    pub fn get_by_category(&self, category: &str, limit: usize) -> Vec<(String, String)> {
        let mut stmt = match self.db.prepare(
            "SELECT key, value FROM memories WHERE category = ?1
             ORDER BY updated_at DESC LIMIT ?2"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![category, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<(String, String)> {
        let pattern = format!("%{}%", query);
        let mut stmt = match self.db.prepare(
            "SELECT key, value FROM memories
             WHERE key LIKE ?1 OR value LIKE ?1
             ORDER BY updated_at DESC LIMIT ?2"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![pattern, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn delete(&self, key: &str) -> bool {
        self.db.execute("DELETE FROM memories WHERE key = ?1", params![key])
            .map(|n| n > 0)
            .unwrap_or(false)
    }
}
