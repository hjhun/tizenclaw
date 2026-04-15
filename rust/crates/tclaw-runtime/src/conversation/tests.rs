use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::{
    config::{RuntimeConfig, RuntimeProfile},
    permissions::{PermissionDecisionSource, PermissionMode, PermissionOutcome},
    session::SessionRecord,
};

#[derive(Default)]
struct FakeModel {
    responses: Vec<Vec<ModelResponseEvent>>,
    seen_requests: Vec<ApiRequest>,
}

impl FakeModel {
    fn new(responses: Vec<Vec<ModelResponseEvent>>) -> Self {
        Self {
            responses,
            seen_requests: Vec::new(),
        }
    }
}

impl ModelTransport for FakeModel {
    fn stream(&mut self, request: &ApiRequest) -> Result<Vec<ModelResponseEvent>, ModelError> {
        self.seen_requests.push(request.clone());
        if self.responses.is_empty() {
            return Err(ModelError::Transport {
                message: "no scripted model response".to_string(),
            });
        }
        Ok(self.responses.remove(0))
    }
}

#[derive(Default)]
struct FakeTools {
    definitions: Vec<ToolDefinition>,
    results: BTreeMap<String, Result<ToolExecutionOutput, ToolRuntimeError>>,
}

impl ToolExecutor for FakeTools {
    fn definitions(&self) -> Vec<ToolDefinition> {
        self.definitions.clone()
    }

    fn execute(&mut self, call: &ToolCallRequest) -> Result<ToolExecutionOutput, ToolRuntimeError> {
        self.results.remove(&call.name).unwrap_or_else(|| {
            Err(ToolRuntimeError::Execution {
                tool_name: call.name.clone(),
                message: "missing scripted tool result".to_string(),
            })
        })
    }
}

#[derive(Default)]
struct FakePermissions {
    allowed: bool,
    seen: Vec<crate::permissions::PermissionRequest>,
}

impl PermissionResolver for FakePermissions {
    fn decide(
        &mut self,
        _config: &RuntimeConfig,
        request: crate::permissions::PermissionRequest,
    ) -> Result<crate::permissions::PermissionDecision, ConversationRuntimeError> {
        self.seen.push(request.clone());
        Ok(crate::permissions::PermissionDecision {
            request,
            allowed: self.allowed,
            outcome: if self.allowed {
                PermissionOutcome::Allowed
            } else {
                PermissionOutcome::Denied
            },
            rationale: if self.allowed {
                "allowed by test policy".to_string()
            } else {
                "blocked by test policy".to_string()
            },
            reasons: vec![if self.allowed {
                "allowed by test policy".to_string()
            } else {
                "blocked by test policy".to_string()
            }],
            source: PermissionDecisionSource::Mode,
            matched_rule: None,
            prompt: None,
        })
    }
}

#[derive(Default)]
struct FakeHooks {
    outcomes: BTreeMap<String, HookOutcome>,
}

impl HookRunner for FakeHooks {
    fn run(
        &mut self,
        hook: &crate::hooks::HookSpec,
        _context: &HookContext,
    ) -> Result<HookOutcome, HookRuntimeError> {
        Ok(self.outcomes.get(&hook.name).cloned().unwrap_or_default())
    }
}

fn host_config() -> RuntimeConfig {
    RuntimeConfig {
        profile: RuntimeProfile::Host,
        permission_mode: PermissionMode::AllowAll,
        ..RuntimeConfig::default()
    }
}

fn prompt() -> crate::prompt::PromptAssembly {
    crate::prompt::PromptAssembly::default()
}

fn session_with_user_message() -> SessionRecord {
    let mut session = SessionRecord::new("session-1", RuntimeProfile::Host);
    session.push_message(crate::session::ConversationMessage::with_text(
        crate::session::SessionMessageRole::User,
        "hello runtime",
    ));
    session
}

fn event_names(events: &[ConversationEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| match event {
            ConversationEvent::HookStarted { name, .. } => format!("hook_started:{name}"),
            ConversationEvent::HookCompleted { name, .. } => format!("hook_completed:{name}"),
            ConversationEvent::RequestPrepared { .. } => "request_prepared".to_string(),
            ConversationEvent::Assistant { event } => match event {
                AssistantEvent::Delta { .. } => "assistant_delta".to_string(),
                AssistantEvent::ToolCall { .. } => "assistant_tool_call".to_string(),
                AssistantEvent::Usage { .. } => "assistant_usage".to_string(),
                AssistantEvent::Completed { .. } => "assistant_completed".to_string(),
            },
            ConversationEvent::PermissionResolved { .. } => "permission_resolved".to_string(),
            ConversationEvent::ToolExecutionStarted { .. } => "tool_execution_started".to_string(),
            ConversationEvent::ToolExecutionFinished { .. } => {
                "tool_execution_finished".to_string()
            }
            ConversationEvent::ToolExecutionFailed { .. } => "tool_execution_failed".to_string(),
            ConversationEvent::CompactionApplied { .. } => "compaction_applied".to_string(),
            ConversationEvent::SummaryUpdated { .. } => "summary_updated".to_string(),
            ConversationEvent::TurnCompleted { .. } => "turn_completed".to_string(),
        })
        .collect()
}

#[test]
fn conversation_round_trip_serializes_cleanly() {
    let mut log = ConversationLog {
        session_id: "session-1".to_string(),
        ..ConversationLog::default()
    };
    log.push(ConversationTurn::new(MessageRole::User, "hello"));

    let json = serde_json::to_string(&log).expect("serialize conversation");
    let restored: ConversationLog = serde_json::from_str(&json).expect("deserialize conversation");

    assert_eq!(restored.turns.len(), 1);
    assert_eq!(restored.turns[0].role, MessageRole::User);
    assert_eq!(restored.turns[0].content, "hello");
}

#[test]
fn runs_normal_assistant_only_turn() {
    let model = FakeModel::new(vec![vec![
        ModelResponseEvent::TextDelta {
            text: "Hello from the assistant.".to_string(),
        },
        ModelResponseEvent::Usage {
            usage: crate::usage::UsageSnapshot {
                model: "gpt-test".to_string(),
                tokens: crate::usage::TokenUsage {
                    input_tokens: 11,
                    output_tokens: 7,
                },
                cost_microunits: 19,
            },
        },
        ModelResponseEvent::Completed,
    ]]);
    let tools = FakeTools::default();
    let permissions = FakePermissions {
        allowed: true,
        ..FakePermissions::default()
    };
    let hooks = FakeHooks::default();
    let mut engine =
        ConversationEngine::new(host_config(), Vec::new(), model, tools, permissions, hooks);
    let mut session = session_with_user_message();

    let result = engine
        .run_turn(&mut session, &prompt(), |_| {})
        .expect("assistant-only turn succeeds");

    assert_eq!(session.state, crate::session::SessionState::Completed);
    assert_eq!(session.messages.len(), 2);
    assert_eq!(result.summary.assistant_text, "Hello from the assistant.");
    assert_eq!(result.summary.usage.total_tokens.input_tokens, 11);
    assert_eq!(
        event_names(&result.events),
        vec![
            "request_prepared",
            "assistant_delta",
            "assistant_usage",
            "assistant_completed",
            "summary_updated",
            "turn_completed",
        ]
    );
}

#[test]
fn runs_tool_execution_turn_and_reenters_model_loop() {
    let model = FakeModel::new(vec![
        vec![
            ModelResponseEvent::TextDelta {
                text: "Checking the workspace.".to_string(),
            },
            ModelResponseEvent::ToolCall {
                call: ToolCallRequest {
                    id: "tool-1".to_string(),
                    name: "list_files".to_string(),
                    input: json!({ "path": "." }),
                },
            },
            ModelResponseEvent::Usage {
                usage: crate::usage::UsageSnapshot {
                    model: "gpt-test".to_string(),
                    tokens: crate::usage::TokenUsage {
                        input_tokens: 9,
                        output_tokens: 4,
                    },
                    cost_microunits: 13,
                },
            },
            ModelResponseEvent::Completed,
        ],
        vec![
            ModelResponseEvent::TextDelta {
                text: "Found the expected files.".to_string(),
            },
            ModelResponseEvent::Completed,
        ],
    ]);
    let tools = FakeTools {
        definitions: vec![ToolDefinition {
            name: "list_files".to_string(),
            description: "List files in a directory".to_string(),
            permission_scope: crate::permissions::PermissionScope::Read,
            minimum_permission_level: crate::permissions::PermissionLevel::Low,
        }],
        results: BTreeMap::from([(
            "list_files".to_string(),
            Ok(ToolExecutionOutput {
                tool_call_id: "tool-1".to_string(),
                output: json!({ "entries": ["a.rs", "b.rs"] }),
                summary: Some("Listed files successfully.".to_string()),
            }),
        )]),
    };
    let permissions = FakePermissions {
        allowed: true,
        ..FakePermissions::default()
    };
    let hooks = FakeHooks::default();
    let mut engine =
        ConversationEngine::new(host_config(), Vec::new(), model, tools, permissions, hooks);
    let mut session = session_with_user_message();

    let result = engine
        .run_turn(&mut session, &prompt(), |_| {})
        .expect("tool turn succeeds");

    assert_eq!(session.messages.len(), 4);
    assert_eq!(result.summary.request_count, 2);
    assert_eq!(result.summary.tool_names, vec!["list_files".to_string()]);
    assert_eq!(result.summary.assistant_text, "Found the expected files.");
    assert_eq!(
        event_names(&result.events),
        vec![
            "request_prepared",
            "assistant_delta",
            "assistant_tool_call",
            "assistant_usage",
            "assistant_completed",
            "permission_resolved",
            "tool_execution_started",
            "tool_execution_finished",
            "request_prepared",
            "assistant_delta",
            "assistant_completed",
            "summary_updated",
            "turn_completed",
        ]
    );
}

#[test]
fn records_tool_failure_and_keeps_turn_stable() {
    let model = FakeModel::new(vec![
        vec![
            ModelResponseEvent::ToolCall {
                call: ToolCallRequest {
                    id: "tool-1".to_string(),
                    name: "broken_tool".to_string(),
                    input: json!({}),
                },
            },
            ModelResponseEvent::Completed,
        ],
        vec![
            ModelResponseEvent::TextDelta {
                text: "The tool failed, but the turn completed.".to_string(),
            },
            ModelResponseEvent::Completed,
        ],
    ]);
    let tools = FakeTools {
        definitions: vec![ToolDefinition {
            name: "broken_tool".to_string(),
            description: "Always fails".to_string(),
            permission_scope: crate::permissions::PermissionScope::Execute,
            minimum_permission_level: crate::permissions::PermissionLevel::Standard,
        }],
        results: BTreeMap::from([(
            "broken_tool".to_string(),
            Err(ToolRuntimeError::Execution {
                tool_name: "broken_tool".to_string(),
                message: "simulated failure".to_string(),
            }),
        )]),
    };
    let permissions = FakePermissions {
        allowed: true,
        ..FakePermissions::default()
    };
    let hooks = FakeHooks::default();
    let mut engine =
        ConversationEngine::new(host_config(), Vec::new(), model, tools, permissions, hooks);
    let mut session = session_with_user_message();

    let result = engine
        .run_turn(&mut session, &prompt(), |_| {})
        .expect("tool failure is represented as a recoverable turn event");

    assert_eq!(session.messages.len(), 4);
    assert!(matches!(
        result
            .events
            .iter()
            .find(|event| matches!(event, ConversationEvent::ToolExecutionFailed { .. })),
        Some(_)
    ));
    assert_eq!(
        result.summary.assistant_text,
        "The tool failed, but the turn completed."
    );
}

#[test]
fn applies_post_session_summary_and_compaction_hooks() {
    let model = FakeModel::new(vec![vec![
        ModelResponseEvent::TextDelta {
            text: "Verbose assistant output that will be summarized.".to_string(),
        },
        ModelResponseEvent::Completed,
    ]]);
    let tools = FakeTools::default();
    let permissions = FakePermissions {
        allowed: true,
        ..FakePermissions::default()
    };
    let hooks = FakeHooks {
        outcomes: BTreeMap::from([(
            "compact_turn".to_string(),
            HookOutcome {
                summary_override: Some("Stable hook summary".to_string()),
                compaction: Some(crate::compact::CompactionPlan {
                    target: "hook:compact_turn".to_string(),
                    max_items: 2,
                    preserve_latest: 2,
                }),
                metadata: BTreeMap::new(),
            },
        )]),
    };
    let mut engine = ConversationEngine::new(
        host_config(),
        vec![crate::hooks::HookSpec {
            name: "compact_turn".to_string(),
            phase: crate::hooks::HookPhase::PostSession,
            command: "compact".to_string(),
            enabled: true,
            env: BTreeMap::new(),
        }],
        model,
        tools,
        permissions,
        hooks,
    );
    let mut session = session_with_user_message();
    session.push_message(crate::session::ConversationMessage::with_text(
        crate::session::SessionMessageRole::Assistant,
        "older assistant message",
    ));
    session.push_message(crate::session::ConversationMessage::with_text(
        crate::session::SessionMessageRole::Tool,
        "older tool result",
    ));

    let result = engine
        .run_turn(&mut session, &prompt(), |_| {})
        .expect("hooked turn succeeds");

    assert_eq!(result.summary.summary, "Stable hook summary");
    assert!(result.summary.compacted);
    assert_eq!(session.summary.as_deref(), Some("Stable hook summary"));
    assert_eq!(session.messages.len(), 2);
    assert!(matches!(session.compaction, Some(_)));
    assert!(event_names(&result.events).contains(&"compaction_applied".to_string()));
}

#[test]
fn preserves_explicit_event_order_for_cli_subscribers() {
    let model = FakeModel::new(vec![
        vec![
            ModelResponseEvent::TextDelta {
                text: "Need a tool.".to_string(),
            },
            ModelResponseEvent::ToolCall {
                call: ToolCallRequest {
                    id: "tool-1".to_string(),
                    name: "lookup".to_string(),
                    input: json!({ "q": "runtime" }),
                },
            },
            ModelResponseEvent::Completed,
        ],
        vec![
            ModelResponseEvent::TextDelta {
                text: "Finished.".to_string(),
            },
            ModelResponseEvent::Completed,
        ],
    ]);
    let tools = FakeTools {
        definitions: vec![ToolDefinition {
            name: "lookup".to_string(),
            description: "lookup".to_string(),
            permission_scope: crate::permissions::PermissionScope::Read,
            minimum_permission_level: crate::permissions::PermissionLevel::Low,
        }],
        results: BTreeMap::from([(
            "lookup".to_string(),
            Ok(ToolExecutionOutput {
                tool_call_id: "tool-1".to_string(),
                output: json!({ "result": "ok" }),
                summary: None,
            }),
        )]),
    };
    let permissions = FakePermissions {
        allowed: true,
        ..FakePermissions::default()
    };
    let hooks = FakeHooks::default();
    let mut engine =
        ConversationEngine::new(host_config(), Vec::new(), model, tools, permissions, hooks);
    let mut session = session_with_user_message();
    let mut observed = Vec::new();

    let result = engine
        .run_turn(&mut session, &prompt(), |event| {
            observed.push(format!("{event:?}"))
        })
        .expect("ordered turn succeeds");

    assert_eq!(observed.len(), result.events.len());
    assert_eq!(
        event_names(&result.events),
        vec![
            "request_prepared",
            "assistant_delta",
            "assistant_tool_call",
            "assistant_completed",
            "permission_resolved",
            "tool_execution_started",
            "tool_execution_finished",
            "request_prepared",
            "assistant_delta",
            "assistant_completed",
            "summary_updated",
            "turn_completed",
        ]
    );
}
