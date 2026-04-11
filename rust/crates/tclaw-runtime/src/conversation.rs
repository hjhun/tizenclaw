use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationTurn {
    pub role: MessageRole,
    pub content: String,
    pub metadata: BTreeMap<String, String>,
}

impl ConversationTurn {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConversationLog {
    pub session_id: String,
    pub turns: Vec<ConversationTurn>,
    pub summary: Option<String>,
}

impl ConversationLog {
    pub fn push(&mut self, turn: ConversationTurn) {
        self.turns.push(turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_round_trip_serializes_cleanly() {
        let mut log = ConversationLog {
            session_id: "session-1".to_string(),
            ..ConversationLog::default()
        };
        log.push(ConversationTurn::new(MessageRole::User, "hello"));

        let json = serde_json::to_string(&log).expect("serialize conversation");
        let restored: ConversationLog =
            serde_json::from_str(&json).expect("deserialize conversation");

        assert_eq!(restored.turns.len(), 1);
        assert_eq!(restored.turns[0].role, MessageRole::User);
        assert_eq!(restored.turns[0].content, "hello");
    }
}
