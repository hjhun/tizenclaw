//! Memory store — Hybrid Persistent Key-Value memory for the agent.
//! Uses SQLite for fast indexing/queries and synchronizes content to
//! Markdown files for Long-Term Memory injection into LLM prompts.

use rusqlite::params;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::sqlite;
use crate::core::on_device_embedding::OnDeviceEmbedding;

#[derive(Clone)]
pub struct MemoryStore {
    base_dir: PathBuf,
    db: Arc<Mutex<rusqlite::Connection>>,
    file_lock: Arc<RwLock<()>>,
    embedding_engine: Arc<Mutex<OnDeviceEmbedding>>,
}

impl MemoryStore {
    pub fn new(base_dir: &str, db_path: &str, model_dir: &str) -> Result<Self, String> {
        let base_path = PathBuf::from(base_dir);
        fs::create_dir_all(&base_path).map_err(|e| format!("Failed to create memory dir: {}", e))?;

        let db = sqlite::open_database(db_path).map_err(|e| format!("DB open: {}", e))?;
        
        let mut embedding = OnDeviceEmbedding::new();
        embedding.initialize(model_dir, None);

        let store = MemoryStore {
            base_dir: base_path,
            db: Arc::new(Mutex::new(db)),
            file_lock: Arc::new(RwLock::new(())),
            embedding_engine: Arc::new(Mutex::new(embedding)),
        };
        
        store.init_tables().map_err(|e| format!("DB init: {}", e))?;
        Ok(store)
    }

    fn init_tables(&self) -> rusqlite::Result<()> {
        let conn = self.db.lock().unwrap();
        conn.execute_batch(
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

    /// Set a memory. Updates SQLite and exports to Markdown.
    pub fn set(&self, key: &str, value: &str, category: &str) {
        {
            let conn = self.db.lock().unwrap();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO memories (key, value, category, updated_at)
                 VALUES (?1, ?2, ?3, datetime('now'))",
                params![key, value, category],
            );
        }
        self.sync_markdown(category);
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT value FROM memories WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ).ok()
    }

    pub fn get_by_category(&self, category: &str, limit: usize) -> Vec<(String, String)> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
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
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
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
        // First get the category so we can sync its markdown later
        let category: Option<String> = {
            let conn = self.db.lock().unwrap();
            conn.query_row("SELECT category FROM memories WHERE key = ?1", params![key], |row| row.get(0)).ok()
        };

        if let Some(cat) = category {
            let success = {
                let conn = self.db.lock().unwrap();
                conn.execute("DELETE FROM memories WHERE key = ?1", params![key]).map(|n| n > 0).unwrap_or(false)
            };
            if success {
                self.sync_markdown(&cat);
            }
            success
        } else {
            false
        }
    }

    /// Loads subset of memory files by semantics using RAG OnDeviceEmbedding
    pub fn load_relevant_for_prompt(&self, prompt: &str, top_k: usize, threshold: f32) -> String {
        let mut engine_guard = self.embedding_engine.lock().unwrap();
        if !engine_guard.is_available() {
            // Fallback: load everything
            return self.load_for_prompt();
        }
        let prompt_emb = engine_guard.encode(prompt);
        if prompt_emb.is_empty() {
            return self.load_for_prompt();
        }

        let mut combined = String::new();
        let _g = self.file_lock.read().unwrap();
        
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
                .collect();
            
            paths.sort();

            let mut scored_memories = Vec::new();

            for path in paths {
                if let Ok(content) = fs::read_to_string(&path) {
                    let cat_name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let emb = engine_guard.encode(&content);
                    if emb.is_empty() { continue; }
                    
                    // Cosine similarity
                    let similarity: f32 = prompt_emb.iter().zip(emb.iter()).map(|(a, b)| a * b).sum();
                    if similarity >= threshold {
                        scored_memories.push((similarity, cat_name, content));
                    }
                }
            }

            scored_memories.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            
            for (_, cat_name, content) in scored_memories.into_iter().take(top_k) {
                combined.push_str(&format!("### Category: {}\n", cat_name));
                combined.push_str(&content);
                combined.push_str("\n\n");
            }
        }
        
        combined.trim_end().to_string()
    }

    /// Loads all markdown files from the `base_dir` and concatenates them for LLM injection.
    pub fn load_for_prompt(&self) -> String {
        let _g = self.file_lock.read().unwrap();
        let mut combined = String::new();

        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
                .collect();
            
            paths.sort(); // Consistent ordering

            for path in paths {
                if let Ok(content) = fs::read_to_string(&path) {
                    let cat_name = path.file_stem().unwrap_or_default().to_string_lossy();
                    combined.push_str(&format!("### Category: {}\n", cat_name));
                    combined.push_str(&content);
                    combined.push_str("\n\n");
                }
            }
        }
        
        combined.trim_end().to_string()
    }

    /// Synchronizes a specific category from SQLite to its Markdown file.
    fn sync_markdown(&self, category: &str) {
        let entries = self.get_by_category(category, 1000); // 1000 items max per category
        let filepath = self.base_dir.join(format!("{}.md", category));
        
        let _g = self.file_lock.write().unwrap();
        if entries.is_empty() {
            let _ = fs::remove_file(filepath);
            return;
        }

        let mut content = format!("---\ncategory: {}\nupdated_at: {}\n---\n\n", 
            category,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
        );

        for (key, value) in entries {
            content.push_str(&format!("## {}\n{}\n\n", key, value));
        }

        let _ = fs::write(filepath, content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_memory_store_hybrid() {
        let tmp = tempdir().unwrap();
        let md_dir = tmp.path().join("memory");
        let db_path = tmp.path().join("mem.db");
        let model_dir = tmp.path().join("models");
        
        let store = MemoryStore::new(
            md_dir.to_str().unwrap(), 
            db_path.to_str().unwrap(),
            model_dir.to_str().unwrap()
        ).unwrap();

        // 1. Write memories
        store.set("fact::light", "Living room light is GPIO 17", "facts");
        store.set("pref::lang", "Use Korean", "preferences");

        // 2. SQL Check
        assert_eq!(store.get("fact::light").unwrap(), "Living room light is GPIO 17");
        
        // 3. Markdowns generated?
        let facts_md = std::fs::read_to_string(md_dir.join("facts.md")).unwrap();
        assert!(facts_md.contains("## fact::light"));
        assert!(facts_md.contains("Living room light is GPIO 17"));

        let pref_md = std::fs::read_to_string(md_dir.join("preferences.md")).unwrap();
        assert!(pref_md.contains("## pref::lang"));

        // 4. Load for prompt combines all
        let all_memories = store.load_for_prompt();
        assert!(all_memories.contains("### Category: facts"));
        assert!(all_memories.contains("### Category: preferences"));
        assert!(all_memories.contains("Use Korean"));

        // 5. Delete an item syncs the MD file
        store.delete("pref::lang");
        assert!(!md_dir.join("preferences.md").exists(), "Empty category MD should be deleted");
        
        let updated_memories = store.load_for_prompt();
        assert!(!updated_memories.contains("### Category: preferences"));
    }
}
