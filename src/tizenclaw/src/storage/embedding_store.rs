//! Embedding store — RAG vector storage for semantic search.

use rusqlite::{params, Connection};
use serde_json::{json, Value};

pub struct EmbeddingStore {
    conn: Option<Connection>,
    knowledge_dbs: Vec<String>,
}

impl Default for EmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingStore {
    pub fn new() -> Self {
        EmbeddingStore {
            conn: None,
            knowledge_dbs: vec![],
        }
    }

    pub fn initialize(&mut self, db_path: &str) -> bool {
        match super::sqlite::open_database(db_path) {
            Ok(conn) => {
                if let Err(err) = initialize_tables(&conn) {
                    log::error!("EmbeddingStore: failed to initialize {}: {}", db_path, err);
                    return false;
                }
                self.conn = Some(conn);
                true
            }
            Err(err) => {
                log::error!("EmbeddingStore: failed to open {}: {}", db_path, err);
                false
            }
        }
    }

    pub fn register_knowledge_db(&mut self, path: &str) {
        if !self.knowledge_dbs.iter().any(|entry| entry == path) {
            self.knowledge_dbs.push(path.to_string());
        }
    }

    pub fn get_pending_knowledge_count(&self) -> usize {
        self.knowledge_dbs.len()
    }

    pub fn detach_knowledge_dbs(&self) {
        // External knowledge DBs are opened on demand during search and closed
        // immediately after query execution, so there is nothing to detach.
    }

    pub fn ingest(&self, source: &str, text: &str) -> Result<usize, String> {
        let conn = self.conn.as_ref().ok_or("Not initialized")?;
        let chunks: Vec<&str> = text
            .as_bytes()
            .chunks(500)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
            .collect();

        let mut count = 0usize;
        for chunk in chunks {
            if chunk.trim().is_empty() {
                continue;
            }

            conn.execute(
                "INSERT INTO embeddings (source, chunk_text) VALUES (?1, ?2)",
                params![source, chunk],
            )
            .map_err(|e| e.to_string())?;
            count += 1;
        }

        log::debug!("EmbeddingStore: ingested {} chunks from '{}'", count, source);
        Ok(count)
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<Value> {
        let pattern = format!("%{}%", query);
        let mut results = Vec::new();

        if let Some(conn) = &self.conn {
            results.extend(search_text_rows(conn, &pattern, top_k));
        }

        if results.len() < top_k {
            for db_path in &self.knowledge_dbs {
                if let Ok(conn) = super::sqlite::open_database(db_path) {
                    let remaining = top_k.saturating_sub(results.len());
                    results.extend(search_text_rows(&conn, &pattern, remaining));
                    if results.len() >= top_k {
                        break;
                    }
                }
            }
        }

        results.truncate(top_k);
        results
    }

    pub fn upsert(&self, key: &str, text: &str, embedding: &[f32]) -> Result<(), String> {
        let conn = self.conn.as_ref().ok_or("Not initialized")?;
        let blob = encode_embedding(embedding);

        conn.execute(
            "INSERT INTO embedding_vectors (key, text, embedding, dimension, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
                 text = excluded.text,
                 embedding = excluded.embedding,
                 dimension = excluded.dimension,
                 updated_at = datetime('now')",
            params![key, text, blob, embedding.len() as i64],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn search_similar(&self, query_embedding: &[f32], limit: usize) -> Vec<(String, f32)> {
        let Some(conn) = &self.conn else {
            return Vec::new();
        };

        if query_embedding.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut stmt = match conn.prepare(
            "SELECT key, embedding, dimension
             FROM embedding_vectors",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return Vec::new(),
        };

        let mut scored: Vec<(String, f32)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .ok()
            .into_iter()
            .flat_map(|rows| rows.filter_map(|row| row.ok()))
            .filter_map(|(key, bytes, dimension)| {
                if dimension <= 0 || dimension as usize != query_embedding.len() {
                    return None;
                }

                let stored = decode_embedding(&bytes)?;
                let score = cosine_similarity(query_embedding, &stored)?;
                Some((key, score))
            })
            .collect();

        scored.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        scored
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        let conn = self.conn.as_ref().ok_or("Not initialized")?;
        conn.execute("DELETE FROM embedding_vectors WHERE key = ?1", params![key])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn close(&mut self) {
        self.conn = None;
    }
}

fn initialize_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS embeddings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            chunk_text TEXT NOT NULL,
            embedding BLOB,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS idx_emb_source ON embeddings(source);
        CREATE TABLE IF NOT EXISTS embedding_vectors (
            key TEXT PRIMARY KEY,
            text TEXT NOT NULL,
            embedding BLOB NOT NULL,
            dimension INTEGER NOT NULL,
            updated_at TEXT DEFAULT (datetime('now'))
        );",
    )
}

fn search_text_rows(conn: &Connection, pattern: &str, limit: usize) -> Vec<Value> {
    let mut stmt = match conn.prepare(
        "SELECT source, chunk_text
         FROM embeddings
         WHERE chunk_text LIKE ?1
         ORDER BY id DESC
         LIMIT ?2",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    stmt.query_map(params![pattern, limit as i64], |row| {
        Ok(json!({
            "source": row.get::<_, String>(0)?,
            "text": row.get::<_, String>(1)?,
        }))
    })
    .ok()
    .map(|rows| rows.filter_map(|row| row.ok()).collect())
    .unwrap_or_default()
}

fn encode_embedding(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * std::mem::size_of::<f32>());
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.len().is_multiple_of(std::mem::size_of::<f32>()) {
        return None;
    }

    Some(
        bytes.chunks_exact(std::mem::size_of::<f32>())
            .map(|chunk| {
                let array: [u8; 4] = chunk.try_into().ok().unwrap_or([0; 4]);
                f32::from_le_bytes(array)
            })
            .collect(),
    )
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f32> {
    if left.len() != right.len() || left.is_empty() {
        return None;
    }

    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;

    for (l, r) in left.iter().zip(right.iter()) {
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }

    let denominator = left_norm.sqrt() * right_norm.sqrt();
    if denominator == 0.0 {
        None
    } else {
        Some(dot / denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_upsert_and_search_similar_roundtrip() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("embeddings.db");

        let mut store = EmbeddingStore::new();
        assert!(store.initialize(db_path.to_str().unwrap()));

        store
            .upsert("skill.read_file", "Read file skill", &[1.0, 0.0, 0.0])
            .unwrap();
        store
            .upsert("skill.write_file", "Write file skill", &[0.9, 0.1, 0.0])
            .unwrap();
        store
            .upsert("skill.weather", "Weather lookup skill", &[0.0, 1.0, 0.0])
            .unwrap();

        let results = store.search_similar(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "skill.read_file");
        assert!(results[0].1 >= results[1].1);
    }

    #[test]
    fn test_delete_removes_vector_entry() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("embeddings.db");

        let mut store = EmbeddingStore::new();
        assert!(store.initialize(db_path.to_str().unwrap()));

        store
            .upsert("skill.read_file", "Read file skill", &[1.0, 0.0])
            .unwrap();
        store.delete("skill.read_file").unwrap();

        assert!(store.search_similar(&[1.0, 0.0], 5).is_empty());
    }

    #[test]
    fn test_ingest_and_search_text_chunks() {
        let tmp = tempdir().unwrap();
        let db_path = tmp.path().join("embeddings.db");

        let mut store = EmbeddingStore::new();
        assert!(store.initialize(db_path.to_str().unwrap()));

        store
            .ingest("skill-docs", "tool_call events are searchable in the audit layer")
            .unwrap();

        let results = store.search("audit", 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["source"], "skill-docs");
    }
}
