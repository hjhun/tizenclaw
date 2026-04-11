use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{config::RuntimeProfile, permissions::PermissionDecision, usage::UsageSnapshot};

const SESSION_FORMAT_VERSION: u32 = 1;

fn default_session_format_version() -> u32 {
    SESSION_FORMAT_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionContentBlock {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_call_id: String,
        output: Value,
    },
    Json {
        value: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConversationMessage {
    pub role: SessionMessageRole,
    #[serde(default)]
    pub content: Vec<SessionContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageSnapshot>,
}

impl ConversationMessage {
    pub fn new(role: SessionMessageRole) -> Self {
        Self {
            role,
            ..Self::default()
        }
    }

    pub fn with_text(role: SessionMessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![SessionContentBlock::Text { text: text.into() }],
            ..Self::default()
        }
    }

    pub fn with_content(mut self, block: SessionContentBlock) -> Self {
        self.content.push(block);
        self
    }
}

impl Default for SessionMessageRole {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Booting,
    Active,
    Suspended,
    Completed,
    Failed,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionCompactionMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub source_message_count: usize,
    #[serde(default)]
    pub retained_message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionForkMetadata {
    pub parent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_message_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionRecord {
    #[serde(default = "default_session_format_version")]
    pub format_version: u32,
    pub session_id: String,
    pub profile: RuntimeProfile,
    pub state: SessionState,
    #[serde(default)]
    pub messages: Vec<ConversationMessage>,
    #[serde(default)]
    pub permission_history: Vec<PermissionDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<SessionCompactionMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fork: Option<SessionForkMetadata>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl SessionRecord {
    pub fn new(session_id: impl Into<String>, profile: RuntimeProfile) -> Self {
        Self {
            format_version: default_session_format_version(),
            session_id: session_id.into(),
            profile,
            state: SessionState::Booting,
            messages: Vec::new(),
            permission_history: Vec::new(),
            summary: None,
            compaction: None,
            fork: None,
            metadata: BTreeMap::new(),
        }
    }

    pub fn push_message(&mut self, message: ConversationMessage) {
        self.messages.push(message);
    }

    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    pub fn set_summary(&mut self, summary: impl Into<String>) {
        self.summary = Some(summary.into());
    }

    pub fn record_compaction(&mut self, metadata: SessionCompactionMetadata) {
        self.compaction = Some(metadata);
    }

    pub fn record_fork(&mut self, metadata: SessionForkMetadata) {
        self.fork = Some(metadata);
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let mut record: SessionRecord =
            serde_json::from_str(&content).map_err(|source| SessionError::Serde {
                path: path.to_path_buf(),
                source,
            })?;
        if record.format_version == 0 {
            record.format_version = default_session_format_version();
        }
        Ok(record)
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), SessionError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let serialized = serde_json::to_vec_pretty(self).map_err(|source| SessionError::Serde {
            path: path.to_path_buf(),
            source,
        })?;
        let temp_path = temporary_write_path(path);
        fs::write(&temp_path, serialized).map_err(|source| SessionError::Io {
            path: temp_path.clone(),
            source,
        })?;
        fs::rename(&temp_path, path).map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SessionStore {
    #[serde(default)]
    pub active: Option<SessionRecord>,
    #[serde(default)]
    pub archived: Vec<SessionRecord>,
}

impl SessionStore {
    pub fn archive_active(&mut self) {
        if let Some(mut active) = self.active.take() {
            active.state = SessionState::Archived;
            self.archived.push(active);
        }
    }

    pub fn upsert_active(&mut self, record: SessionRecord) {
        self.active = Some(record);
    }

    pub fn active_mut(&mut self) -> Result<&mut SessionRecord, SessionError> {
        self.active
            .as_mut()
            .ok_or(SessionError::MissingActiveSession)
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        serde_json::from_str(&content).map_err(|source| SessionError::Serde {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), SessionError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let serialized = serde_json::to_vec_pretty(self).map_err(|source| SessionError::Serde {
            path: path.to_path_buf(),
            source,
        })?;
        let temp_path = temporary_write_path(path);
        fs::write(&temp_path, serialized).map_err(|source| SessionError::Io {
            path: temp_path.clone(),
            source,
        })?;
        fs::rename(&temp_path, path).map_err(|source| SessionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

fn temporary_write_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|file_name| file_name.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session path {path} could not be read or written: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("session path {path} could not be serialized or deserialized: {source}")]
    Serde {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("session store does not have an active session")]
    MissingActiveSession,
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::json;

    use super::*;
    use crate::permissions::{PermissionRequest, PermissionScope};

    fn temp_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tclaw-runtime-{test_name}-{unique}"));
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }

    fn cleanup_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn session_record_round_trips_serialization() {
        let mut record = SessionRecord::new("session-1", RuntimeProfile::Host);
        record.set_state(SessionState::Active);
        record.set_summary("condensed state");
        record.permission_history.push(PermissionDecision {
            request: PermissionRequest {
                scope: PermissionScope::Write,
                target: "session.json".to_string(),
                reason: "save session".to_string(),
            },
            allowed: true,
            rationale: "session persistence is allowed".to_string(),
        });
        record.push_message(
            ConversationMessage::with_text(SessionMessageRole::User, "hello").with_content(
                SessionContentBlock::Json {
                    value: json!({ "intent": "greet" }),
                },
            ),
        );
        record.push_message(ConversationMessage {
            role: SessionMessageRole::Assistant,
            content: vec![
                SessionContentBlock::Text {
                    text: "hi".to_string(),
                },
                SessionContentBlock::ToolCall {
                    id: "call-1".to_string(),
                    name: "search".to_string(),
                    input: json!({ "q": "TizenClaw" }),
                },
            ],
            name: Some("planner".to_string()),
            metadata: BTreeMap::from([(String::from("trace_id"), json!("trace-1"))]),
            usage: Some(UsageSnapshot {
                model: "gpt-test".to_string(),
                tokens: crate::usage::TokenUsage {
                    input_tokens: 12,
                    output_tokens: 8,
                },
                cost_microunits: 42,
            }),
        });
        record.record_compaction(SessionCompactionMetadata {
            compacted_at: Some("2026-04-12T04:00:00Z".to_string()),
            summary: Some("summary".to_string()),
            source_message_count: 24,
            retained_message_count: 8,
        });
        record.record_fork(SessionForkMetadata {
            parent_session_id: "parent-1".to_string(),
            parent_message_index: Some(3),
            forked_at: Some("2026-04-12T04:02:00Z".to_string()),
            reason: Some("retry from before tool call".to_string()),
        });
        record
            .metadata
            .insert("cwd".to_string(), json!("/workspace"));

        let json = serde_json::to_string_pretty(&record).expect("serialize session");
        let restored: SessionRecord = serde_json::from_str(&json).expect("deserialize session");

        assert_eq!(restored, record);
        assert_eq!(restored.format_version, SESSION_FORMAT_VERSION);
        assert_eq!(restored.messages.len(), 2);
    }

    #[test]
    fn session_record_load_save_round_trip() {
        let dir = temp_dir("session-record-load-save");
        let path = dir.join("session.json");

        let mut record = SessionRecord::new("session-2", RuntimeProfile::Test);
        record.set_state(SessionState::Completed);
        record.push_message(ConversationMessage::with_text(
            SessionMessageRole::Assistant,
            "done",
        ));

        record.save_to_path(&path).expect("save session");
        let restored = SessionRecord::load_from_path(&path).expect("load session");

        assert_eq!(restored, record);
        cleanup_dir(&dir);
    }

    #[test]
    fn session_store_archives_and_round_trips() {
        let dir = temp_dir("session-store-round-trip");
        let path = dir.join("store.json");

        let mut store = SessionStore::default();
        let mut active = SessionRecord::new("s-1", RuntimeProfile::Host);
        active.set_state(SessionState::Active);
        active.push_message(ConversationMessage::with_text(
            SessionMessageRole::User,
            "persist me",
        ));
        store.upsert_active(active);
        store.archive_active();

        assert!(store.active.is_none());
        assert_eq!(store.archived.len(), 1);
        assert_eq!(store.archived[0].state, SessionState::Archived);

        store.save_to_path(&path).expect("save store");
        let restored = SessionStore::load_from_path(&path).expect("load store");

        assert_eq!(restored, store);
        cleanup_dir(&dir);
    }
}
