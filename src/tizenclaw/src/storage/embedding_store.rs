//! Embedding store — RAG vector storage for semantic search.

use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub struct EmbeddingStore {
    conn: Option<Connection>,
    knowledge_dbs: Vec<String>,
}

impl EmbeddingStore {
    pub fn new() -> Self {
        EmbeddingStore { conn: None, knowledge_dbs: vec![] }
    }

    pub fn initialize(&mut self, db_path: &str) -> bool {
        match Connection::open(db_path) {
            Ok(conn) => {
                let _ = conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS embeddings (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        source TEXT NOT NULL,
                        chunk_text TEXT NOT NULL,
                        embedding BLOB,
                        created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                    );
                    CREATE INDEX IF NOT EXISTS idx_emb_source ON embeddings(source);",
                );
                self.conn = Some(conn);
                true
            }
            Err(e) => {
                log::error!("EmbeddingStore: failed to open {}: {}", db_path, e);
                false
            }
        }
    }

    pub fn register_knowledge_db(&mut self, path: &str) {
        self.knowledge_dbs.push(path.to_string());
    }

    pub fn get_pending_knowledge_count(&self) -> usize {
        self.knowledge_dbs.len()
    }

    pub fn detach_knowledge_dbs(&self) {
        // Detach any attached DBs to reclaim file cache
        if let Some(conn) = &self.conn {
            for (i, _) in self.knowledge_dbs.iter().enumerate() {
                let alias = format!("kb_{}", i);
                let _ = conn.execute_batch(&format!("DETACH DATABASE IF EXISTS {}", alias));
            }
        }
    }

    pub fn ingest(&self, source: &str, text: &str) -> Result<usize, String> {
        let conn = self.conn.as_ref().ok_or("Not initialized")?;
        // Chunk text into ~500 char segments
        let chunks: Vec<&str> = text.as_bytes()
            .chunks(500)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();

        let mut count = 0;
        for chunk in &chunks {
            if chunk.trim().is_empty() { continue; }
            conn.execute(
                "INSERT INTO embeddings (source, chunk_text) VALUES (?1, ?2)",
                params![source, chunk],
            ).map_err(|e| e.to_string())?;
            count += 1;
        }
        log::info!("EmbeddingStore: ingested {} chunks from '{}'", count, source);
        Ok(count)
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<Value> {
        let conn = match &self.conn {
            Some(c) => c,
            None => return vec![],
        };
        // Simple keyword search (full vector search requires embedding model)
        let sql = "SELECT source, chunk_text FROM embeddings WHERE chunk_text LIKE ?1 LIMIT ?2";
        let pattern = format!("%{}%", query);
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let results = stmt.query_map(params![pattern, top_k as i64], |row| {
            Ok(json!({
                "source": row.get::<_, String>(0).unwrap_or_default(),
                "text": row.get::<_, String>(1).unwrap_or_default(),
            }))
        }).ok().map(|rows| rows.flatten().collect()).unwrap_or_default();
        results
    }

    pub fn close(&mut self) {
        self.conn = None;
    }
}
