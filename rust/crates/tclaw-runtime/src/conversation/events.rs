use serde::{Deserialize, Serialize};

use crate::{session::ConversationMessage, session::SessionCompactionMetadata, usage::UsageSnapshot};

use super::{ApiRequest, HookOutcome, ToolCallRequest, ToolExecutionOutput, ToolFailure, TurnSummary};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelResponseEvent {
    TextDelta { text: String },
    ToolCall { call: ToolCallRequest },
    Usage { usage: UsageSnapshot },
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantEvent {
    Delta { text: String },
    ToolCall { call: ToolCallRequest },
    Usage { usage: UsageSnapshot },
    Completed { message: ConversationMessage },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEvent {
    HookStarted {
        name: String,
        phase: crate::hooks::HookPhase,
    },
    HookCompleted {
        name: String,
        phase: crate::hooks::HookPhase,
        outcome: HookOutcome,
    },
    RequestPrepared {
        request: ApiRequest,
    },
    Assistant {
        event: AssistantEvent,
    },
    PermissionResolved {
        decision: crate::permissions::PermissionDecision,
    },
    ToolExecutionStarted {
        call: ToolCallRequest,
    },
    ToolExecutionFinished {
        result: ToolExecutionOutput,
    },
    ToolExecutionFailed {
        failure: ToolFailure,
    },
    CompactionApplied {
        metadata: SessionCompactionMetadata,
    },
    SummaryUpdated {
        summary: TurnSummary,
    },
    TurnCompleted {
        summary: TurnSummary,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationTurnResult {
    pub summary: TurnSummary,
    #[serde(default)]
    pub events: Vec<ConversationEvent>,
}
