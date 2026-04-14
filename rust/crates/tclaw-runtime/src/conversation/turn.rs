use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::usage::{TokenUsage, UsageSnapshot};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnUsageReport {
    #[serde(default)]
    pub snapshots: Vec<UsageSnapshot>,
    pub total_tokens: TokenUsage,
    pub total_cost_microunits: u64,
}

impl TurnUsageReport {
    pub(crate) fn record(&mut self, usage: UsageSnapshot) {
        self.total_tokens.input_tokens += usage.tokens.input_tokens;
        self.total_tokens.output_tokens += usage.tokens.output_tokens;
        self.total_cost_microunits += usage.cost_microunits;
        self.snapshots.push(usage);
    }

    pub(crate) fn latest(&self) -> Option<&UsageSnapshot> {
        self.snapshots.last()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnSummary {
    pub session_id: String,
    pub request_count: usize,
    pub tool_call_count: usize,
    #[serde(default)]
    pub tool_names: Vec<String>,
    pub assistant_text: String,
    pub final_message_count: usize,
    pub compacted: bool,
    pub summary: String,
    pub usage: TurnUsageReport,
}
