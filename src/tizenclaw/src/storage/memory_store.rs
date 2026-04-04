//! Memory store — Hybrid Persistent Key-Value memory for the agent.
//! Uses SQLite for fast indexing/queries and synchronizes content to
//! Markdown files for Long-Term Memory injection into LLM prompts.

use rusqlite::params;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::sqlite;
use crate::core::on_device_embedding::OnDeviceEmbedding;

/// Sanitizes a string for use as a filename
fn sanitize_filename(s: &str) -> String {
    let s = s.replace("::", "_").replace(" ", "-");
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

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
        
        // Ensure subdirectories exist
        store.ensure_subdirs().map_err(|e| format!("Subdir init: {}", e))?;

        // One-time migration: sync all and move old files to subdirs
        let _ = store.migrate_to_subdirs();
        
        // Initial summary generation
        store.regenerate_summary();
        
        Ok(store)
    }

    fn ensure_subdirs(&self) -> std::io::Result<()> {
        fs::create_dir_all(self.base_dir.join("short-term"))?;
        fs::create_dir_all(self.base_dir.join("long-term"))?;
        fs::create_dir_all(self.base_dir.join("episodic"))?;
        Ok(())
    }

    fn get_category_dir(&self, category: &str) -> PathBuf {
        match category {
            "episodic" => self.base_dir.join("episodic"),
            "facts" | "preferences" => self.base_dir.join("long-term"),
            _ => self.base_dir.join("short-term"),
        }
    }

    /// Migration: Syncs everything to the new subdirectory format and deletes legacy files.
    fn migrate_to_subdirs(&self) -> Result<(), String> {
        let legacy_files = ["facts.md", "general.md", "preferences.md", "episodic.md"];
        
        // 1. Sync all categories to new subdirectory format
        let categories = ["facts", "general", "preferences", "episodic"];
        for cat in categories {
            let entries = self.get_by_category(cat, 10000);
            for (key, value, updated_at) in entries {
                self.write_entry_markdown(&key, &value, cat, &updated_at);
            }
        }

        // 2. Scan base_dir for date-prefixed files and move them if they aren't subdirs
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    // If it's a date-prefixed file (not memory.md or legacy)
                    if name.contains('_') && !legacy_files.contains(&name.as_ref()) && name != "memory.md" {
                        // We use sqlite to find its correct category
                        let key_part = name.split('_').nth(1).unwrap_or_default().replace(".md", "");
                        let cat = {
                            let conn = self.db.lock().unwrap();
                            conn.query_row("SELECT category FROM memories WHERE key LIKE ?1", 
                                params![format!("%{}%", key_part)], 
                                |row| row.get::<_, String>(0)).ok()
                        }.unwrap_or_else(|| "general".to_string());
                        
                        let target = self.get_category_dir(&cat).join(name.as_ref());
                        let _ = fs::rename(path, target);
                    }
                }
            }
        }

        // 3. Remove legacy flat files
        let _g = self.file_lock.write().unwrap();
        for file in legacy_files {
            let path = self.base_dir.join(file);
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
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
        let updated_at = {
            let conn = self.db.lock().unwrap();
            let _ = conn.execute(
                "INSERT OR REPLACE INTO memories (key, value, category, updated_at)
                 VALUES (?1, ?2, ?3, datetime('now'))",
                params![key, value, category],
            );
            
            // Get the newly generated updated_at
            conn.query_row(
                "SELECT updated_at FROM memories WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0)
            ).unwrap_or_else(|_| "Unknown Date".to_string())
        };

        self.write_entry_markdown(key, value, category, &updated_at);
        self.regenerate_summary();
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT value FROM memories WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ).ok()
    }

    pub fn get_by_category(&self, category: &str, limit: usize) -> Vec<(String, String, String)> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT key, value, updated_at FROM memories WHERE category = ?1
             ORDER BY updated_at DESC LIMIT ?2"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![category, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<(String, String, String)> {
        let pattern = format!("%{}%", query);
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT key, value, updated_at FROM memories
             WHERE key LIKE ?1 OR value LIKE ?1
             ORDER BY updated_at DESC LIMIT ?2"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![pattern, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn delete(&self, key: &str) -> bool {
        // Find existing metadata before deleting from DB
        let (cat_opt, ts_opt) = {
            let conn = self.db.lock().unwrap();
            match conn.query_row(
                "SELECT category, updated_at FROM memories WHERE key = ?1", 
                params![key], 
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            ) {
                Ok(res) => (Some(res.0), Some(res.1)),
                Err(_) => (None, None),
            }
        };

        if let (Some(cat), Some(ts)) = (cat_opt, ts_opt) {
            let success = {
                let conn = self.db.lock().unwrap();
                conn.execute("DELETE FROM memories WHERE key = ?1", params![key]).map(|n| n > 0).unwrap_or(false)
            };
            if success {
                // Delete the specific file in its category subdir
                let date_pref = &ts[0..10];
                let filename = format!("{}_{}.md", date_pref, sanitize_filename(key));
                let filepath = self.get_category_dir(&cat).join(filename);
                let _g = self.file_lock.write().unwrap();
                let _ = fs::remove_file(filepath);
                
                self.regenerate_summary();
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
        
        let mut scored_memories = Vec::new();

        let subdirs = ["short-term", "long-term", "episodic"];
        for subdir in subdirs {
            let dir_path = self.base_dir.join(subdir);
            if let Ok(entries) = fs::read_dir(dir_path) {
                for path in entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|ext| ext == "md")) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let cat_name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                        let emb = engine_guard.encode(&content);
                        if emb.is_empty() { continue; }
                        
                        // Cosine similarity
                        let similarity: f32 = prompt_emb.iter().zip(emb.iter()).map(|(a, b)| a * b).sum();
                        if similarity >= threshold {
                            scored_memories.push((similarity, format!("{} ({})", subdir, cat_name), content));
                        }
                    }
                }
            }
        }

        scored_memories.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        
        // Include summary index context always at the top of RAG results
        if let Ok(summary) = fs::read_to_string(self.base_dir.join("memory.md")) {
            combined.push_str("## MEMORY SUMMARY & INDEX (RAG Context)\n");
            combined.push_str(&summary);
            combined.push_str("\n---\n\n");
        }

        for (_, cat_name, content) in scored_memories.into_iter().take(top_k) {
            combined.push_str(&format!("### Key: {}\n", cat_name));
            combined.push_str(&content);
            combined.push_str("\n\n");
        }
        
        combined.trim_end().to_string()
    }

    /// Loads all markdown files recursively and concatenates them for LLM injection.
    /// Injects `memory.md` summary at the top if it exists.
    pub fn load_for_prompt(&self) -> String {
        let _g = self.file_lock.read().unwrap();
        let mut combined = String::new();

        // 1. Try to load memory.md first
        if let Ok(summary) = fs::read_to_string(self.base_dir.join("memory.md")) {
            combined.push_str("## MEMORY SUMMARY & INDEX\n");
            combined.push_str(&summary);
            combined.push_str("\n---\n\n");
        }

        let subdirs = ["short-term", "long-term", "episodic"];
        for subdir in subdirs {
            let dir_path = self.base_dir.join(subdir);
            if let Ok(entries) = fs::read_dir(dir_path) {
                let mut paths: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
                    .collect();
                
                paths.sort();

                for path in paths {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let cat_name = path.file_stem().unwrap_or_default().to_string_lossy();
                        combined.push_str(&format!("### Category: {} ({})\n", subdir, cat_name));
                        combined.push_str(&content);
                        combined.push_str("\n\n");
                    }
                }
            }
        }
        
        combined.trim_end().to_string()
    }

    /// Writes a single memory entry to its date-prefixed markdown file in the correct subdirectory.
    fn write_entry_markdown(&self, key: &str, value: &str, category: &str, updated_at: &str) {
        let date_pref = if updated_at.len() >= 10 { &updated_at[0..10] } else { "unknown" };
        let sanitized_key = sanitize_filename(key);
        let filename = format!("{}_{}.md", date_pref, sanitized_key);
        let target_dir = self.get_category_dir(category);
        let filepath = target_dir.join(&filename);

        let _g = self.file_lock.write().unwrap();
        
        // Clean up any existing files for this key with DIFFERENT dates (to prevent duplicates)
        // Search in all subdirs since a category might have changed
        let subdirs = ["short-term", "long-term", "episodic"];
        for subdir in subdirs {
            let dir_path = self.base_dir.join(subdir);
            if let Ok(entries) = fs::read_dir(dir_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let entry_name = entry.file_name().to_string_lossy().to_string();
                    if entry_name.ends_with(&format!("_{}.md", sanitized_key)) && entry.path() != filepath {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }

        let content = format!("---\nkey: {}\ncategory: {}\nupdated_at: {}\n---\n\n## {} (Recorded at: {})\n{}\n",
            key,
            category,
            updated_at,
            key,
            updated_at,
            value
        );

        let _ = fs::write(filepath, content);
    }

    /// Regenerates the `memory.md` summary file.
    pub fn regenerate_summary(&self) {
        let now_ts = {
            let conn = self.db.lock().unwrap();
            conn.query_row("SELECT datetime('now', 'localtime')", [], |row| row.get::<_, String>(0)).unwrap_or_default()
        };

        let mut md = String::new();
        md.push_str("# TizenClaw Memory Summary\n\n");
        md.push_str(&format!("*Last Updated: {}*\n\n", now_ts));

        // 1. Recent Episodic (last 5)
        md.push_str("## Recent Episodes (Episodic)\n\n");
        let episodic = self.get_by_category("episodic", 5);
        if episodic.is_empty() {
            md.push_str("- No episodic memories yet.\n");
        } else {
            md.push_str("| Time | Event (Key) | Description |\n");
            md.push_str("|------|-------------|-------------|\n");
            for (key, value, ts) in episodic {
                let summary = if value.contains('\n') { value.split('\n').next().unwrap() } else { &value };
                let short_val = if summary.len() > 50 { format!("{}...", &summary[..47]) } else { summary.to_string() };
                md.push_str(&format!("| {} | {} | {} |\n", ts, key, short_val));
            }
        }
        md.push('\n');

        // 2. Key Facts (Long-term)
        md.push_str("## Core Facts & Preferences (Long-term)\n\n");
        let facts = self.get_by_category("facts", 10);
        let prefs = self.get_by_category("preferences", 10);
        let mut combined = facts;
        combined.extend(prefs);
        combined.sort_by(|a, b| b.2.cmp(&a.2));
        
        if combined.is_empty() {
            md.push_str("- No long-term records.\n");
        } else {
            for (key, value, _) in combined.into_iter().take(15) {
                let first_line = value.split('\n').next().unwrap_or("");
                md.push_str(&format!("- **{}**: {}\n", key, first_line));
            }
        }
        md.push('\n');

        let _g = self.file_lock.write().unwrap();
        let _ = fs::write(self.base_dir.join("memory.md"), md);
    }

    /// Legacy synchronization method (deprecated in favor of per-entry files)
    fn sync_markdown(&self, _category: &str) {
        // No-op: replaced by write_entry_markdown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_memory_store_subdirectories() {
        let tmp = tempdir().unwrap();
        let md_dir = tmp.path().join("memory");
        let db_path = tmp.path().join("mem.db");
        let model_dir = tmp.path().join("models");
        
        let store = MemoryStore::new(
            md_dir.to_str().unwrap(), 
            db_path.to_str().unwrap(),
            model_dir.to_str().unwrap()
        ).unwrap();

        // 1. Write memories of different categories
        store.set("fact::light", "Living room light is GPIO 17", "facts");
        store.set("pref::lang", "Use Korean", "preferences");
        store.set("action::exec", "Last ran ls", "episodic");
        store.set("chat::hello", "User said hi", "general");

        // 2. Verify files in correct subdirectories
        // Files are created with current date from SQL, so we scan for key
        let lt_entries = std::fs::read_dir(md_dir.join("long-term")).unwrap();
        let lt_files: Vec<_> = lt_entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert!(lt_files.iter().any(|f| f.contains("_fact_light.md")));
        assert!(lt_files.iter().any(|f| f.contains("_pref_lang.md")));

        let ep_entries = std::fs::read_dir(md_dir.join("episodic")).unwrap();
        let ep_files: Vec<_> = ep_entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert!(ep_files.iter().any(|f| f.contains("_action_exec.md")));

        let st_entries = std::fs::read_dir(md_dir.join("short-term")).unwrap();
        let st_files: Vec<_> = st_entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert!(st_files.iter().any(|f| f.contains("_chat_hello.md")));

        // 3. Verify summary exists
        assert!(md_dir.join("memory.md").exists());
        let summary = std::fs::read_to_string(md_dir.join("memory.md")).unwrap();
        assert!(summary.contains("# TizenClaw Memory Summary"));
        assert!(summary.contains("fact::light"));

        // 4. Load for prompt includes summary
        let all_memories = store.load_for_prompt();
        assert!(all_memories.contains("## MEMORY SUMMARY & INDEX"));
    }

    #[test]
    fn test_memory_migration_to_subdirs() {
        let tmp = tempdir().unwrap();
        let md_dir = tmp.path().join("memory");
        let db_path = tmp.path().join("mem.db");
        fs::create_dir_all(&md_dir).unwrap();

        // Create a fake date-prefixed file in the base dir
        let date = "2024-04-03";
        let filename = format!("{}_old_fact.md", date);
        fs::write(md_dir.join(&filename), "Old Content").unwrap();

        // Prepare DB so migration knows where to move it
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute("CREATE TABLE memories (key TEXT PRIMARY KEY, value TEXT, category TEXT, updated_at TEXT)", []).unwrap();
            conn.execute("INSERT INTO memories (key, value, category, updated_at) VALUES (?1, ?2, ?3, ?4)", 
                params!["old_fact", "Old Content", "facts", format!("{} 12:00:00", date)]).unwrap();
        }

        let store = MemoryStore::new(
            md_dir.to_str().unwrap(), 
            db_path.to_str().unwrap(),
            tmp.path().to_str().unwrap()
        ).unwrap();

        // Should have moved to long-term
        assert!(!md_dir.join(&filename).exists());
        assert!(md_dir.join("long-term").join(&filename).exists());
    }
}
