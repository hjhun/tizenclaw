pub mod bash;
pub mod bash_validation;
pub mod bootstrap;
pub mod branch_lock;
pub mod compact;
pub mod config;
pub mod config_validate;
pub mod conversation;
pub mod file_ops;
pub mod git_context;
pub mod green_contract;
pub mod hooks;
pub mod json;
pub mod lane_events;
pub mod lsp_client;
pub mod mcp;
pub mod mcp_client;
pub mod mcp_lifecycle_hardened;
pub mod mcp_server;
pub mod mcp_stdio;
pub mod mcp_tool_bridge;
pub mod oauth;
pub mod permission_enforcer;
pub mod permissions;
pub mod plugin_lifecycle;
pub mod policy_engine;
pub mod prompt;
pub mod recovery_recipes;
pub mod remote;
pub mod sandbox;
pub mod session;
pub mod session_control;
pub mod stale_base;
pub mod stale_branch;
pub mod summary_compression;
pub mod task_packet;
pub mod task_registry;
pub mod team_cron_registry;
pub mod trust_resolver;
pub mod usage;
pub mod worker_boot;

pub use bash::{BashCommand, BashExecutionPlan};
pub use bash_validation::{BashValidationResult, BashValidationViolation};
pub use bootstrap::{RuntimeBootstrap, RuntimeModuleMap};
pub use branch_lock::BranchLockState;
pub use compact::CompactionPlan;
pub use config::{McpRuntimeConfig, RuntimeConfig, RuntimeConfigPatch, RuntimePaths, RuntimeProfile};
pub use config_validate::{ConfigValidationIssue, ConfigValidationReport};
pub use conversation::{
    ApiRequest, AssistantEvent, ConversationEngine, ConversationEngineOptions, ConversationEvent,
    ConversationLog, ConversationRuntimeError, ConversationTurn, ConversationTurnResult,
    HookContext, HookOutcome, HookRunner, HookRuntimeError, MessageRole, ModelError,
    ModelResponseEvent, ModelTransport, PermissionResolver, ToolCallRequest, ToolDefinition,
    ToolExecutionOutput, ToolExecutor, ToolFailure, ToolRuntimeError, TurnSummary, TurnUsageReport,
};
pub use file_ops::{FileMutation, FileMutationKind};
pub use git_context::GitContextSnapshot;
pub use green_contract::GreenContract;
pub use hooks::{HookPhase, HookSpec};
pub use json::{JsonEnvelope, JsonEnvelopeError};
pub use lane_events::{LaneEvent, LaneEventKind};
pub use lsp_client::LspClientSpec;
pub use mcp::{
    bridged_tool_name, JsonRpcError, JsonRpcId, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, McpClientMetadata, McpContentBlock, McpInitializeParams,
    McpInitializeResult, McpListResourcesResult, McpListToolsResult, McpPeerCapabilities,
    McpReadResourceParams, McpReadResourceResult, McpResourceCapabilities,
    McpResourceContents, McpResourceDefinition, McpRuntimeState, McpServerInfo,
    McpToolCallParams, McpToolCallResult, McpToolCapabilities, McpToolDefinition,
};
pub use mcp_client::McpClientSpec;
pub use mcp_lifecycle_hardened::{ManagedMcpServer, McpLifecyclePolicy, McpServerHealth};
pub use mcp_server::{McpServerRegistration, McpServerState};
pub use mcp_stdio::{
    McpStdioServerSpec, McpTransport, McpTransportError, StdioMcpTransport, StdioTransportMode,
};
pub use mcp_tool_bridge::{McpBridgePolicy, McpBridgedToolExecutor, McpToolBridge};
pub use oauth::{OAuthProvider, OAuthState};
pub use permission_enforcer::{
    DenyAllPrompter, PermissionEnforcer, PermissionEnforcerState, PermissionPrompter,
    RecordingPrompter,
};
pub use permissions::{
    PermissionDecision, PermissionDecisionSource, PermissionLevel, PermissionMode,
    PermissionOutcome, PermissionPromptDecision, PermissionPromptRecord,
    PermissionPromptRequest, PermissionRequest, PermissionScope,
};
pub use plugin_lifecycle::{PluginLifecyclePhase, PluginLifecycleState};
pub use policy_engine::{
    PolicyContext, PolicyEffect, PolicyEngine, PolicyEngineState, PolicyEvaluation, PolicyRule,
};
pub use prompt::{
    collect_context_files, ContextFile, ProjectContext, PromptAssembly, PromptBuilder,
    PromptEnvironmentContext, PromptError, PromptFragment, PromptFragmentKind,
};
pub use recovery_recipes::RecoveryRecipe;
pub use remote::RemoteRuntimeSpec;
pub use sandbox::SandboxPolicy;
pub use session::{
    ConversationMessage, SessionCompactionMetadata, SessionContentBlock, SessionError,
    SessionForkMetadata, SessionMessageRole, SessionRecord, SessionState, SessionStore,
};
pub use session_control::{SessionControlCommand, SessionControlResult};
pub use stale_base::StaleBaseReport;
pub use stale_branch::StaleBranchReport;
pub use summary_compression::SummaryCompressionResult;
pub use task_packet::{TaskPacket, TaskPriority};
pub use task_registry::TaskRegistrySnapshot;
pub use tclaw_api::{canonical_surfaces, SurfaceDescriptor};
pub use team_cron_registry::{TeamCronEntry, TeamCronRegistry};
pub use trust_resolver::{TrustLevel, TrustResolution};
pub use usage::{TokenUsage, UsageSnapshot};
pub use worker_boot::{WorkerBootSpec, WorkerBootState, WorkerIdentity, WorkerKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_exposes_runtime_module_map() {
        let bootstrap = RuntimeBootstrap::new();

        assert_eq!(bootstrap.canonical_runtime, "rust");
        assert!(bootstrap.modules.modules.contains(&"config".to_string()));
        assert!(bootstrap
            .surfaces
            .iter()
            .any(|surface| surface.name == "runtime"));
    }
}
