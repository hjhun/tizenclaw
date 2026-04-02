//! Session store - manages conversation sessions via Markdown files.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::{Write, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

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

#[derive(Clone)]
pub struct SessionStore {
    base_dir: PathBuf,
    lock: Arc<RwLock<()>>,
}

impl SessionStore {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let base_dir = Path::new(db_path).parent().unwrap_or(Path::new(".")).to_path_buf();
        let sessions_dir = base_dir.join("sessions");
        let audit_dir = base_dir.join("audit");
        
        fs::create_dir_all(&sessions_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(&audit_dir).map_err(|e| e.to_string())?;

        Ok(SessionStore {
            base_dir,
            lock: Arc::new(RwLock::new(())),
        })
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join("sessions").join(format!("{}.md", session_id))
    }

    pub fn ensure_session(&self, session_id: &str) {
        let path = self.session_path(session_id);
        if !path.exists() {
            let _guard = self.lock.write().unwrap();
            let mut file = match OpenOptions::new().create(true).write(true).open(&path) {
                Ok(f) => f,
                Err(_) => return,
            };
            let frontmatter = format!("---\nid: {}\ntimestamp: {}\n---\n\n", session_id, get_timestamp());
            let _ = file.write_all(frontmatter.as_bytes());
        }
    }

    pub fn add_message(&self, session_id: &str, role: &str, content: &str) {
        self.ensure_session(session_id);
        let path = self.session_path(session_id);
        
        let _guard = self.lock.write().unwrap();
        if let Ok(mut file) = OpenOptions::new().append(true).open(&path) {
            let block = format!("## {}\n{}\n\n", role, content);
            let _ = file.write_all(block.as_bytes());
        }
    }

    pub fn get_messages(&self, session_id: &str, limit: usize) -> Vec<SessionMessage> {
        let path = self.session_path(session_id);
        let _guard = self.lock.read().unwrap();
        
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut messages = Vec::new();
        let mut current_role = String::new();
        let mut current_text = Vec::new();

        for line in content.lines() {
            if line.starts_with("## ") {
                if !current_role.is_empty() {
                    messages.push(SessionMessage {
                        role: current_role.clone(),
                        text: current_text.join("\n").trim().to_string(),
                        timestamp: get_timestamp(),
                    });
                    current_text.clear();
                }
                current_role = line[3..].trim().to_string();
            } else if !current_role.is_empty() {
                if !line.starts_with("---") { // Skip dangling frontmatter borders if any
                    current_text.push(line);
                }
            }
        }

        if !current_role.is_empty() {
            messages.push(SessionMessage {
                role: current_role,
                text: current_text.join("\n").trim().to_string(),
                timestamp: get_timestamp(),
            });
        }

        // Limit defaults to tail elements
        let skip = if messages.len() > limit { messages.len() - limit } else { 0 };
        messages.into_iter().skip(skip).collect()
    }

    pub fn clear_session(&self, session_id: &str) {
        let path = self.session_path(session_id);
        let _guard = self.lock.write().unwrap();
        let _ = fs::remove_file(path);
    }

    pub fn record_usage(&self, _session_id: &str, _prompt_tokens: i32, _completion_tokens: i32, _model: &str) {
        // Usage tracking left for audit logging integration later
    }

    pub fn load_token_usage(&self, _session_id: &str) -> TokenUsage {
        TokenUsage::default()
    }

    pub fn load_daily_usage(&self, _date: &str) -> TokenUsage {
        TokenUsage::default()
    }
}

fn get_timestamp() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{}", now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn get_store(dir: &Path) -> SessionStore {
        SessionStore::new(dir.join("test.db").to_str().unwrap()).unwrap()
    }

    #[test]
    fn test_create_store() {
        let dir = tempdir().unwrap();
        let _store = get_store(dir.path());
        assert!(dir.path().join("sessions").exists());
    }

    #[test]
    fn test_ensure_session() {
        let dir = tempdir().unwrap();
        let store = get_store(dir.path());
        store.ensure_session("s1");
        assert!(dir.path().join("sessions").join("s1.md").exists());
    }

    #[test]
    fn test_add_and_get_messages() {
        let dir = tempdir().unwrap();
        let store = get_store(dir.path());
        store.add_message("s1", "user", "Hello");
        store.add_message("s1", "assistant", "Hi there!");
        let msgs = store.get_messages("s1", 10);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].text, "Hello");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].text, "Hi there!");
    }

    #[test]
    fn test_message_limit() {
        let dir = tempdir().unwrap();
        let store = get_store(dir.path());
        for i in 0..10 {
            store.add_message("s1", "user", &format!("msg_{}", i));
        }
        let msgs = store.get_messages("s1", 3);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].text, "msg_7");
    }
}
