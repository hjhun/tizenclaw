use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::permissions::{PermissionLevel, PermissionScope};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub permission_scope: PermissionScope,
    #[serde(default)]
    pub minimum_permission_level: PermissionLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolExecutionOutput {
    pub tool_call_id: String,
    pub output: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolFailure {
    pub tool_call_id: String,
    pub name: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolRuntimeError {
    #[error("tool {tool_name} was denied: {message}")]
    PermissionDenied { tool_name: String, message: String },
    #[error("tool {tool_name} failed: {message}")]
    Execution { tool_name: String, message: String },
}

pub trait ToolExecutor {
    fn definitions(&self) -> Vec<ToolDefinition>;

    fn execute(&mut self, call: &ToolCallRequest) -> Result<ToolExecutionOutput, ToolRuntimeError>;
}
