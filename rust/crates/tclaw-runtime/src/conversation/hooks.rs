use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hooks::{HookPhase, HookSpec};

use super::{ToolCallRequest, TurnSummary};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookContext {
    pub phase: HookPhase,
    pub session_id: String,
    pub request_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<ToolCallRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<TurnSummary>,
    #[serde(default)]
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HookOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<crate::compact::CompactionPlan>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

pub trait HookRunner {
    fn run(
        &mut self,
        hook: &HookSpec,
        context: &HookContext,
    ) -> Result<HookOutcome, HookRuntimeError>;
}

#[derive(Debug, Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookRuntimeError {
    #[error("hook {name} in {phase:?} failed: {message}")]
    Execution {
        name: String,
        phase: HookPhase,
        message: String,
    },
}
