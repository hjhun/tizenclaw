use serde::{Deserialize, Serialize};

use crate::{
    config::RuntimeProfile, conversation::ConversationLog, permissions::PermissionDecision,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Booting,
    Active,
    Suspended,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub session_id: String,
    pub profile: RuntimeProfile,
    pub state: SessionState,
    pub conversation: ConversationLog,
    pub permission_history: Vec<PermissionDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionStore {
    pub active: Option<SessionRecord>,
    pub archived: Vec<SessionRecord>,
}

impl SessionStore {
    pub fn archive_active(&mut self) {
        if let Some(active) = self.active.take() {
            self.archived.push(active);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_store_archives_active_session() {
        let record = SessionRecord {
            session_id: "s-1".to_string(),
            profile: RuntimeProfile::Host,
            state: SessionState::Active,
            conversation: ConversationLog::default(),
            permission_history: Vec::new(),
        };
        let mut store = SessionStore {
            active: Some(record),
            archived: Vec::new(),
        };

        store.archive_active();

        assert!(store.active.is_none());
        assert_eq!(store.archived.len(), 1);
    }
}
