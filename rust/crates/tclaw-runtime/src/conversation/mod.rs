mod api_bridge;
mod engine;
mod events;
mod helpers;
mod hooks;
mod tool_exec;
mod turn;

pub use api_bridge::ApiRequest;
pub use engine::{
    ConversationEngine, ConversationEngineOptions, ConversationRuntimeError, ModelError,
    ModelTransport, PermissionResolver,
};
pub use events::{AssistantEvent, ConversationEvent, ConversationTurnResult, ModelResponseEvent};
pub use hooks::{HookContext, HookOutcome, HookRunner, HookRuntimeError};
pub use tool_exec::{
    ToolCallRequest, ToolDefinition, ToolExecutionOutput, ToolExecutor, ToolFailure,
    ToolRuntimeError,
};
pub use turn::{ConversationLog, ConversationTurn, MessageRole, TurnSummary, TurnUsageReport};

#[cfg(test)]
mod tests;
